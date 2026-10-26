//! CLI for ofd-file: convert OFD -> Markdown / PDF, inspect structure.

#![cfg(feature = "cli")]

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use ofd_file::OfdDocument;

#[derive(Parser)]
#[command(name = "ofd-file", version, about = "OFD parser and converter (Markdown / PDF)")]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Print high-level info about the OFD file.
    Info {
        #[arg(value_name = "OFD_FILE")]
        input: PathBuf,
    },

    /// List zip entries inside the OFD.
    Ls {
        #[arg(value_name = "OFD_FILE")]
        input: PathBuf,
    },

    /// Extract page text as plain UTF-8.
    Text {
        #[arg(value_name = "OFD_FILE")]
        input: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Convert OFD -> Markdown.
    #[command(name = "to-md")]
    ToMd {
        #[arg(value_name = "OFD_FILE")]
        input: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Convert OFD -> PDF (vector, via krilla).
    #[cfg(feature = "pdf")]
    #[command(name = "to-pdf")]
    ToPdf {
        #[arg(value_name = "OFD_FILE")]
        input: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Fallback TTF font (recommended for CJK OFDs without embedded fonts).
        #[arg(long, value_name = "TTF")]
        font: Option<PathBuf>,
    },

    /// Render OFD pages to PNG (raster, via vello_cpu). Outputs
    /// `<prefix>-page-001.png`, etc.
    #[cfg(feature = "render-cpu")]
    #[command(name = "to-png")]
    ToPng {
        #[arg(value_name = "OFD_FILE")]
        input: PathBuf,
        /// Output prefix. Default: `<input_stem>`.
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Target DPI.
        #[arg(long, default_value_t = 150.0)]
        dpi: f32,
        /// Which page (1-indexed). If omitted, renders all.
        #[arg(long)]
        page: Option<usize>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Cmd::Info { input } => {
            let doc = OfdDocument::open(&input)
                .with_context(|| format!("open {}", input.display()))?;
            let info = doc.doc_info();
            println!("doc_id         : {}", info.doc_id);
            println!("title          : {}", info.title);
            println!("author         : {}", info.author);
            println!("creator        : {} {}", info.creator, info.creator_version);
            println!("created        : {}", info.creation_date);
            println!("modified       : {}", info.mod_date);
            println!("pages          : {}", doc.page_count());
            if let Some(cd) = &info.custom_datas {
                if !cd.custom_data.is_empty() {
                    println!("custom_data:");
                    for c in &cd.custom_data {
                        println!(
                            "  - {} = {}",
                            c.name.as_deref().unwrap_or("?"),
                            c.value.trim()
                        );
                    }
                }
            }
            for (i, p) in doc.pages().iter().enumerate() {
                println!(
                    "  page {:>3}: id={} size={:.2}x{:.2}mm entry={}",
                    i + 1,
                    p.id,
                    p.area.width,
                    p.area.height,
                    p.full_path
                );
            }
        }
        Cmd::Ls { input } => {
            let doc = OfdDocument::open(&input)?;
            for name in doc.list_entries() {
                println!("{name}");
            }
        }
        Cmd::Text { input, output } => {
            let doc = OfdDocument::open(&input)?;
            let txt = doc.extract_text();
            write_out(output.as_deref(), txt.as_bytes())?;
        }
        Cmd::ToMd { input, output } => {
            let doc = OfdDocument::open(&input)?;
            let md = doc.to_markdown()?;
            write_out(output.as_deref(), md.as_bytes())?;
        }
        #[cfg(feature = "pdf")]
        Cmd::ToPdf { input, output, font } => {
            let mut doc = OfdDocument::open(&input)?;
            let opts = {
                let mut o = ofd_file::PdfOptions::new();
                if let Some(f) = font { o.fallback_font = Some(f); }
                o
            };
            let bytes = doc.to_pdf_with_options(&opts)?;
            let path = output.unwrap_or_else(|| input.with_extension("pdf"));
            std::fs::write(&path, bytes)
                .with_context(|| format!("writing {}", path.display()))?;
            eprintln!("wrote {}", path.display());
        }
        #[cfg(feature = "render-cpu")]
        Cmd::ToPng { input, output, dpi, page } => {
            let mut doc = OfdDocument::open(&input)?;
            let opts = ofd_file::PngOptions { dpi, ..Default::default() };
            let stem = output.unwrap_or_else(|| {
                input.with_extension("").file_name().map_or_else(
                    || PathBuf::from("page"),
                    PathBuf::from,
                )
            });
            let indices: Vec<usize> = match page {
                Some(p) if p > 0 => vec![p - 1],
                _ => (0..doc.page_count()).collect(),
            };
            for i in indices {
                let bytes = doc.to_png_page(i, &opts)?;
                let path = if doc.page_count() <= 1 {
                    stem.with_extension("png")
                } else {
                    let mut p = stem.clone();
                    p.set_file_name(format!(
                        "{}-page-{:03}.png",
                        stem.file_name().and_then(|s| s.to_str()).unwrap_or("page"),
                        i + 1,
                    ));
                    p
                };
                std::fs::write(&path, bytes)
                    .with_context(|| format!("writing {}", path.display()))?;
                eprintln!("wrote {}", path.display());
            }
        }
    }
    Ok(())
}

fn write_out(path: Option<&std::path::Path>, bytes: &[u8]) -> Result<()> {
    match path {
        Some(p) => {
            std::fs::write(p, bytes)
                .with_context(|| format!("writing {}", p.display()))?;
            eprintln!("wrote {}", p.display());
        }
        None => {
            use std::io::Write;
            std::io::stdout().write_all(bytes)?;
        }
    }
    Ok(())
}
