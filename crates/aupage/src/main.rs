use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use image::GenericImageView;
use std::path::PathBuf;
use aupage::types::{ExportFormat, EmbeddedImage, ShowMode};
use aupage::parser;

#[derive(Clone, ValueEnum, Debug, PartialEq)]
pub enum ExportBackend {
    /// Use native Rust implementation for export
    Native,
    /// Auto-select best backend (tries Chrome first, falls back to native)
    Auto,
    /// Use external third-party tools for export
    External,
}

#[derive(Parser)]
#[command(name = "aupage", about = "AuPage document exporter - export .aupage.json to PDF/DOCX/HTML")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Export an AuPage document to PDF, DOCX, or HTML
    Export {
        /// Input .aupage.json file
        input: PathBuf,

        /// Output format
        #[arg(short, long, value_enum, default_value = "html")]
        format: ExportFormat,

        /// Output file path (auto-generated if not specified)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Export backend: native (Rust impl), auto (auto-select), external (third-party tools)
        #[arg(long, value_enum, default_value = "auto")]
        backend: ExportBackend,

        /// Chart images to embed (key=value, can be repeated)
        /// Example: chart-0=/path/to/chart.png
        #[arg(long = "image")]
        images: Vec<String>,

        /// Section title visibility: compact (hide) or full (show)
        #[arg(long, default_value = "compact")]
        show: ShowMode,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Export { input, format, output, backend, images, show } => {
            export_command(&input, format, output, &backend, &images, &show)?;
        }
    }

    Ok(())
}

fn export_command(
    input: &PathBuf,
    format: ExportFormat,
    output: Option<PathBuf>,
    backend: &ExportBackend,
    image_args: &[String],
    show_mode: &ShowMode,
) -> Result<()> {
    if !input.exists() {
        anyhow::bail!("Input file not found: {}", input.display());
    }
    if !input.is_file() {
        anyhow::bail!("Input path is not a file: {}", input.display());
    }

    // Parse AuPage document
    let doc = parser::parse_file(input)?;
    eprintln!("Loaded AuPage document: {}", doc.title);

    // Load embedded images
    let images = load_images(image_args)?;

    // Determine output path
    let output_path = output.unwrap_or_else(|| {
        let ext = match format {
            ExportFormat::Pdf => "pdf",
            ExportFormat::Docx => "docx",
            ExportFormat::Html => "html",
        };
        input.with_extension(format!("aupage.{}", ext))
    });

    // Resolve backend
    let resolved_backend = match backend {
        ExportBackend::Auto => {
            match format {
                ExportFormat::Pdf => {
                    if has_chrome() { ExportBackend::External } else { ExportBackend::Native }
                }
                ExportFormat::Docx => ExportBackend::Native,
                ExportFormat::Html => ExportBackend::Native,
            }
        }
        ExportBackend::Native => ExportBackend::Native,
        ExportBackend::External => {
            match format {
                ExportFormat::Docx => ExportBackend::Native, // DOCX always native
                _ => ExportBackend::External,
            }
        }
    };

    // Validate external backend availability
    if resolved_backend == ExportBackend::External {
        if !has_chrome() {
            anyhow::bail!("External PDF backend requires Chrome/Chromium. Install it for best rendering.");
        }
    }

    // Export
    match format {
        ExportFormat::Html => {
            let html = aupage::html_export::export(&doc, &images, show_mode);
            std::fs::write(&output_path, html)?;
            eprintln!("HTML exported to: {}", output_path.display());
        }
        ExportFormat::Pdf => {
            match resolved_backend {
                ExportBackend::Native => {
                    aupage::pdf_export::export(&doc, &images, &output_path, show_mode)?;
                }
                ExportBackend::External => {
                    export_pdf_external(&doc, &images, &output_path, show_mode)?;
                }
                ExportBackend::Auto => unreachable!(),
            }
            eprintln!("PDF exported to: {} (backend: {:?})", output_path.display(), resolved_backend);
        }
        ExportFormat::Docx => {
            aupage::docx_export::export(&doc, &images, &output_path)?;
            eprintln!("DOCX exported to: {}", output_path.display());
        }
    }

    Ok(())
}

fn load_images(image_args: &[String]) -> Result<Vec<EmbeddedImage>> {
    let mut images = Vec::new();
    for arg in image_args {
        if let Some((key, path)) = arg.split_once('=') {
            let img = image::open(path)?;
            let mut png_bytes = Vec::new();
            let mut cursor = std::io::Cursor::new(&mut png_bytes);
            img.write_to(&mut cursor, image::ImageFormat::Png)?;

            let dimensions = img.dimensions();
            images.push(EmbeddedImage {
                key: key.to_string(),
                data: png_bytes,
                width: dimensions.0,
                height: dimensions.1,
            });
            eprintln!("Loaded image: {} ({}x{})", path, dimensions.0, dimensions.1);
        } else {
            eprintln!("Warning: skipping invalid image argument: {}", arg);
        }
    }
    Ok(images)
}

/// Check if headless Chrome (google-chrome or chromium) is available
fn has_chrome() -> bool {
    for browser in ["google-chrome", "google-chrome-stable", "chromium", "chromium-browser", "chrome"] {
        if std::process::Command::new(browser)
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            return true;
        }
    }
    false
}

/// Check if the current process is running as root (uid 0)
fn is_running_as_root() -> bool {
    unsafe { libc::getuid() == 0 }
}

/// Export PDF using html2pdf (headless Chrome - best rendering quality, ECharts charts)
fn export_pdf_external(
    doc: &aupage::types::AuPageDocument,
    _images: &[aupage::types::EmbeddedImage],
    output_path: &std::path::Path,
    show_mode: &ShowMode,
) -> Result<()> {
    let html = aupage::html_export::export_for_pdf_chrome(doc, show_mode);

    // Write HTML to a temp file in current directory (snap Chrome can't access /tmp)
    let temp_html = PathBuf::from(format!(".aupage_tmp_{}.html", std::process::id()));
    std::fs::write(&temp_html, &html)?;

    // html2pdf needs absolute path for file:// URL
    let abs_html = std::fs::canonicalize(&temp_html)?;

    use headless_chrome::types::PrintToPdfOptions;
    use headless_chrome::LaunchOptions;

    let mut launch_opts = LaunchOptions::default();
    // Docker 环境下以 root 运行，需要禁用 Chrome sandbox
    if is_running_as_root() {
        launch_opts.sandbox = false;
    }

    html2pdf::html_to_pdf(
        &abs_html,
        output_path,
        PrintToPdfOptions {
            print_background: Some(true),
            ..Default::default()
        },
        launch_opts,
        Some(std::time::Duration::from_millis(800)), // wait for ECharts to render
    )
    .map_err(|e| anyhow::anyhow!("html2pdf conversion failed: {}", e))?;

    let _ = std::fs::remove_file(&temp_html);
    eprintln!("PDF exported via html2pdf (headless Chrome)");
    Ok(())
}
