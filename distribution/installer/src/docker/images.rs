use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::io::Write;
use std::process::Command;
use anyhow::{Context, Result};
use colored::*;
use walkdir::WalkDir;
use log::{info, warn, error, debug};
use crate::i18n::t;

pub struct ImageMap {
    map: HashMap<String, String>,
}

impl Default for ImageMap {
    fn default() -> Self {
        let mut map = HashMap::new();

        let is_arm64 = cfg!(target_arch = "aarch64");
        let suffix = if is_arm64 { "-arm64" } else { "" };

        map.insert("actforce".to_string(), format!("actforce:latest{}", suffix));
        map.insert("actforce-arm64".to_string(), "actforce:latest-arm64".to_string());

        Self { map }
    }
}

impl ImageMap {
    pub fn get(&self, key: &str) -> Option<&String> {
        self.map.get(key)
    }
}

#[derive(Debug)]
pub struct ImageFile {
    pub path: PathBuf,
    pub name: String,
    pub size: String,
    pub target_image: String,
}

fn infer_image_name_from_filename(filename: &str) -> String {
    let name = filename.replace('_', "/");

    if let Some(pos) = name.rfind('-') {
        let (repo, tag) = name.split_at(pos);
        return format!("{}{}", repo, tag.replacen('-', ":", 1));
    }

    name
}

pub fn get_available_images(images_dir: &Path) -> Result<Vec<ImageFile>> {
    let image_map = ImageMap::default();
    let mut images = Vec::new();

    if !images_dir.exists() {
        return Ok(images);
    }

    for entry in WalkDir::new(images_dir)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path().is_file() &&
            e.path().extension().map_or(false, |ext| ext == "gz") &&
            e.path().to_string_lossy().ends_with(".tar.gz")
        })
    {
        let path = entry.path().to_path_buf();
        let filename = path.file_stem().unwrap_or_default().to_string_lossy();
        let filename = filename.trim_end_matches(".tar");

        let metadata = fs::metadata(&path)?;
        let size = format!("{:.1} MB", metadata.len() as f64 / 1_048_576.0);

        let target_image = match image_map.get(&filename.to_string()) {
            Some(target) => target.clone(),
            None => {
                let inferred = infer_image_name_from_filename(&filename);
                debug!("Inferred image name '{}' from filename '{}'", inferred, filename);
                inferred
            }
        };

        images.push(ImageFile {
            path: path.clone(),
            name: filename.to_string(),
            size,
            target_image,
        });
    }

    images.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(images)
}

pub fn load_image(image_file: &ImageFile, loaded_images: &mut HashSet<PathBuf>) -> Result<()> {
    let path = &image_file.path;

    if loaded_images.contains(path) {
        println!("{}", format!("Skipping: Image file {} already loaded", path.display()).yellow());
        return Ok(());
    }

    println!("{}", format!("Loading image: {}", path.display()).blue());

    let output = Command::new("gunzip")
        .arg("-c")
        .arg(path)
        .stdout(std::process::Stdio::piped())
        .spawn()
        .context("Failed to start gunzip command")?;

    let docker_load = Command::new("docker")
        .arg("load")
        .stdin(output.stdout.unwrap())
        .output()
        .context("Failed to execute docker load command")?;

    if !docker_load.status.success() {
        let error_msg = String::from_utf8_lossy(&docker_load.stderr);
        error!("Docker load failed: {}", error_msg);
        return Err(anyhow::anyhow!("Failed to load image: {}", error_msg));
    }

    let output_str = String::from_utf8_lossy(&docker_load.stdout);
    println!("{}", output_str);

    let loaded_image = if let Some(captures) = regex::Regex::new(r"Loaded image: (.+)")
        .unwrap()
        .captures(&output_str)
    {
        captures.get(1).map(|m| m.as_str().to_string())
    } else {
        None
    };

    if let Some(img) = &loaded_image {
        println!("{}", format!("Successfully loaded image: {}", img).green());
    }

    let loaded_id = if let Some(img) = &loaded_image {
        let output = Command::new("docker")
            .args(["images", "--format", "{{.ID}}", "--filter", &format!("reference={}", img)])
            .output()
            .context("Failed to get image ID")?;

        String::from_utf8_lossy(&output.stdout).trim().to_string()
    } else {
        println!("{}", t("could_not_extract_image_name").yellow());
        let output = Command::new("docker")
            .args(["images", "--sort", "created", "--format", "{{.ID}}"])
            .output()
            .context("Failed to get latest image ID")?;

        String::from_utf8_lossy(&output.stdout).lines().next().unwrap_or("").to_string()
    };

    if loaded_id.is_empty() {
        return Err(anyhow::anyhow!("Failed to get image ID"));
    }

    if !image_file.target_image.is_empty() {
        println!("{}", format!("Tagging image {} as {}", loaded_id, image_file.target_image).yellow());

        let tag_result = Command::new("docker")
            .args(["tag", &loaded_id, &image_file.target_image])
            .output()
            .context("Failed to tag image")?;

        if tag_result.status.success() {
            println!("{}", format!("Successfully tagged image as {}", image_file.target_image).green());
        } else {
            let error_msg = String::from_utf8_lossy(&tag_result.stderr);
            println!("{}", format!("Failed to tag image: {}", error_msg).red());
            return Err(anyhow::anyhow!("Failed to tag image: {}", error_msg));
        }
    } else {
        println!("{}", t("no_target_image_specified").yellow());
    }

    loaded_images.insert(path.clone());

    Ok(())
}

pub fn load_all_images(images_dir: &Path) -> Result<()> {
    let images = get_available_images(images_dir)?;

    if images.is_empty() {
        println!("{}", format!("No image files found in {}", images_dir.display()).red());
        return Ok(());
    }

    let mut loaded_images = HashSet::new();
    let mut success_count = 0;
    let mut failed_images = Vec::new();

    for image in &images {
        match load_image(image, &mut loaded_images) {
            Ok(_) => {
                success_count += 1;
            }
            Err(e) => {
                error!("Failed to load image {}: {}", image.path.display(), e);
                failed_images.push(image.path.clone());
            }
        }
    }

    println!("\n{}", t("loading_summary").bold());
    println!("{}", format!("Total images: {}", images.len()).bold());
    println!("{}", format!("Successfully loaded: {}", success_count).green().bold());

    if !failed_images.is_empty() {
        println!("{}", format!("Failed to load: {}", failed_images.len()).red().bold());
        println!("{}", t("failed_images").red());
        for path in failed_images {
            println!("  - {}", path.display());
        }
    }

    Ok(())
}

pub fn interactive_load_images(images_dir: &Path) -> Result<()> {
    let images = get_available_images(images_dir)?;

    if images.is_empty() {
        println!("{}", format!("No image files found in {}", images_dir.display()).red());
        return Ok(());
    }

    println!("{}", t("select_images_to_load").cyan().bold());
    println!("  0) {}", t("load_all_images"));

    for (i, image) in images.iter().enumerate() {
        println!("  {}) {} -> {} ({})",
            i + 1,
            image.name,
            image.target_image,
            image.size
        );
    }

    println!("  q) {}", t("quit"));
    println!();

    print!("{}: ", t("enter_options"));
    std::io::stdout().flush().unwrap();

    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;

    let input = input.trim();

    if input == "q" {
        println!("{}", t("operation_cancelled").yellow());
        return Ok(());
    }

    let mut loaded_images = HashSet::new();

    if input == "0" {
        load_all_images(images_dir)?;
    } else {
        for selection in input.split_whitespace() {
            if let Ok(index) = selection.parse::<usize>() {
                if index > 0 && index <= images.len() {
                    let selected_image = &images[index - 1];
                    if let Err(e) = load_image(selected_image, &mut loaded_images) {
                        error!("Failed to load image {}: {}", selected_image.path.display(), e);
                    }
                } else {
                    println!("{}", format!("Invalid option: {}", selection).red());
                }
            } else {
                println!("{}", format!("Invalid option: {}", selection).red());
            }
        }
    }

    Ok(())
}
