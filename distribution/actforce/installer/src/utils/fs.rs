use crate::utils::error::{InstallerError, Result};
use chrono::Local;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use tar::Archive;
use walkdir::WalkDir;

/// 提取 tar 包到指定目录
pub fn extract_package(package_path: &Path, target_dir: &Path) -> Result<PathBuf> {
    // 确保安装包存在
    if !package_path.exists() {
        return Err(InstallerError::FileNotFound(
            package_path.to_string_lossy().to_string(),
        ));
    }

    // 创建临时目录
    let temp_dir = target_dir.join("temp");
    log::debug!("Creating temporary directory at: {}", temp_dir.display());
    
    if temp_dir.exists() {
        log::debug!("Temporary directory already exists, removing: {}", temp_dir.display());
        fs::remove_dir_all(&temp_dir).map_err(|e| {
            log::debug!("Failed to remove existing temp directory: {}", e);
            InstallerError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to remove existing temp directory: {}", e),
            ))
        })?;
    }
    
    log::debug!("Creating temp directory: {}", temp_dir.display());
    fs::create_dir_all(&temp_dir).map_err(|e| {
        log::debug!("Failed to create temp directory: {}", e);
        InstallerError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Failed to create temp directory: {}", e),
        ))
    })?;

    // 打开并解压 tar 文件
    log::debug!("Opening tar file: {}", package_path.display());
    let tar_file = File::open(package_path).map_err(|e| {
        log::debug!("Failed to open tar file: {}", e);
        InstallerError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Failed to open tar file: {}", e),
        ))
    })?;

    log::debug!("Extracting tar file to: {}", temp_dir.display());
    let mut archive = Archive::new(tar_file);
    archive.unpack(&temp_dir).map_err(|e| {
        log::debug!("Failed to extract package: {}", e);
        InstallerError::PackageExtraction(format!("Failed to extract package: {}", e))
    })?;
    log::debug!("Package extraction completed successfully");

    // 查找解压后的目录
    let mut extracted_dir = temp_dir.clone();

    // 检查是否直接解压到了 temp 目录
    log::debug!("Checking if package was extracted directly to temp directory");
    if temp_dir.join("docker").exists() {
        log::info!("Package extracted directly to temp directory");
        log::debug!("Found docker directory at: {}", temp_dir.join("docker").display());
        return Ok(temp_dir);
    }
    log::debug!("Docker directory not found directly in temp directory, searching for subdirectories");

    // 查找第一个子目录
    log::debug!("Searching for first subdirectory in: {}", temp_dir.display());
    for entry in fs::read_dir(&temp_dir).map_err(|e| {
        log::debug!("Failed to read temp directory: {}", e);
        InstallerError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Failed to read temp directory: {}", e),
        ))
    })? {
        let entry = entry.map_err(|e| {
            log::debug!("Failed to read directory entry: {}", e);
            InstallerError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to read directory entry: {}", e),
            ))
        })?;
        let path = entry.path();
        log::debug!("Found entry: {}", path.display());
        if path.is_dir() {
            log::debug!("Found subdirectory: {}", path.display());
            extracted_dir = path;
            break;
        }
    }

    // 如果没有找到子目录，尝试查找 docker-compose 文件
    if extracted_dir == temp_dir && !extracted_dir.join("docker").exists() {
        log::info!("No subdirectory found, looking for docker-compose file");
        log::debug!("Searching for docker-compose.yml or docker-compose.yaml in: {}", temp_dir.display());
        
        // 查找 docker-compose 文件
        for entry in WalkDir::new(&temp_dir).max_depth(3) {
            let entry = entry.map_err(|e| {
                log::debug!("Failed to walk directory: {}", e);
                InstallerError::Io(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Failed to walk directory: {}", e),
                ))
            })?;
            let path = entry.path();
            if path.is_file() {
                let file_name = path.file_name().unwrap_or_default().to_string_lossy();
                log::debug!("Checking file: {}", path.display());
                if file_name == "docker-compose.yml" || file_name == "docker-compose.yaml" {
                    extracted_dir = path.parent().unwrap().to_path_buf();
                    log::info!(
                        "Found docker-compose file, using its parent directory: {}",
                        extracted_dir.display()
                    );
                    log::debug!("Found docker-compose file at: {}", path.display());
                    break;
                }
            }
        }
    }

    // 如果仍然没有找到有效目录，返回错误
    if extracted_dir == temp_dir && !extracted_dir.join("docker").exists() {
        log::debug!("Could not find valid directory structure in extracted package");
        
        // Log the contents of the temp directory for debugging
        if let Ok(entries) = fs::read_dir(&temp_dir) {
            let paths: Vec<String> = entries
                .filter_map(|e| e.ok())
                .map(|e| e.path().display().to_string())
                .collect();
            log::debug!("Contents of temp directory: {:?}", paths);
        } else {
            log::debug!("Failed to read temp directory for debugging");
        }
        
        return Err(InstallerError::PackageExtraction(
            "Could not determine extracted directory structure".to_string(),
        ));
    }

    log::debug!("Final extracted directory: {}", extracted_dir.display());
    Ok(extracted_dir)
}

/// 备份目录 - 只备份docker子目录，不备份整个actforce目录
pub fn backup_directory(source_dir: &Path) -> Result<PathBuf> {
    if !source_dir.exists() {
        return Err(InstallerError::DirectoryNotFound(
            source_dir.to_string_lossy().to_string(),
        ));
    }

    // 检查docker子目录是否存在
    let docker_dir = source_dir.join("docker");
    if !docker_dir.exists() {
        return Err(InstallerError::DirectoryNotFound(
            format!("Docker directory not found: {}", docker_dir.display()),
        ));
    }

    let timestamp = Local::now().format("%Y%m%d%H%M%S").to_string();
    let backup_dir = source_dir.join(format!("docker_backup_{}", timestamp));

    // 如果备份目录已存在，先删除
    if backup_dir.exists() {
        log::debug!("Backup directory already exists, removing: {}", backup_dir.display());
        fs::remove_dir_all(&backup_dir).map_err(|e| {
            log::debug!("Failed to remove existing backup directory: {}", e);
            InstallerError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to remove existing backup directory: {}", e),
            ))
        })?;
    }

    // 创建备份目录
    log::debug!("Creating backup directory: {}", backup_dir.display());
    fs::create_dir_all(&backup_dir).map_err(|e| {
        log::debug!("Failed to create backup directory: {}", e);
        InstallerError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Failed to create backup directory: {}", e),
        ))
    })?;

    // 复制docker目录到备份目录
    log::debug!("Copying docker directory from {} to {}", docker_dir.display(), backup_dir.join("docker").display());
    copy_directory(&docker_dir, &backup_dir)?;
    log::debug!("Backup completed successfully to: {}", backup_dir.display());

    Ok(backup_dir)
}

/// 复制目录
pub fn copy_directory(source: &Path, destination: &Path) -> Result<()> {
    log::debug!("Copying directory from {} to {}", source.display(), destination.display());
    
    if !source.exists() {
        return Err(InstallerError::FileNotFound(
            source.to_string_lossy().to_string(),
        ));
    }

    if !source.is_dir() {
        return Err(InstallerError::NotADirectory(
            source.to_string_lossy().to_string(),
        ));
    }

    if !destination.exists() {
        log::debug!("Creating destination directory: {}", destination.display());
        fs::create_dir_all(destination).map_err(|e| {
            log::debug!("Failed to create destination directory: {}", e);
            InstallerError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to create destination directory: {}", e),
            ))
        })?;
    }

    for entry in fs::read_dir(source).map_err(|e| {
        log::debug!("Failed to read source directory: {}", e);
        InstallerError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Failed to read source directory: {}", e),
        ))
    })? {
        let entry = entry.map_err(|e| {
            log::debug!("Failed to read directory entry: {}", e);
            InstallerError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to read directory entry: {}", e),
            ))
        })?;
        let path = entry.path();
        let dest_path = destination.join(path.file_name().unwrap());

        if path.is_dir() {
            copy_directory(&path, &dest_path)?;
        } else {
            copy_file(&path, &dest_path)?;
        }
    }
    
    log::debug!("Directory copy completed: {} -> {}", source.display(), destination.display());
    Ok(())
}

/// 复制文件
pub fn copy_file(source: &Path, destination: &Path) -> Result<()> {    
    if !source.exists() {
        log::debug!("Source file does not exist: {}", source.display());
        return Err(InstallerError::FileNotFound(
            source.to_string_lossy().to_string(),
        ));
    }

    // 确保目标目录存在
    if let Some(parent) = destination.parent() {
        log::debug!("Creating parent directory: {}", parent.display());
        fs::create_dir_all(parent).map_err(|e| {
            log::debug!("Failed to create parent directory: {}", e);
            InstallerError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to create parent directory: {}", e),
            ))
        })?;
    }

    // 复制文件
    log::debug!("Copying file: {} -> {}", source.display(), destination.display());
    fs::copy(source, destination).map_err(|e| {
        log::debug!("Failed to copy file: {}", e);
        InstallerError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Failed to copy file: {}", e),
        ))
    })?;
    Ok(())
}

pub fn remove_file(path: &Path) -> Result<()> {
    log::debug!("Removing file: {}", path.display());
    fs::remove_file(path).map_err(|e| {
        log::debug!("Failed to remove file: {}", e);
        InstallerError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Failed to remove file: {}", e),
        ))
    })?;
    Ok(())
}

/// 移动目录
pub fn move_directory(source: &Path, destination: &Path) -> Result<()> {
    log::debug!("Moving directory from {} to {}", source.display(), destination.display());
    
    if !source.exists() {
        return Err(InstallerError::DirectoryNotFound(
            source.to_string_lossy().to_string(),
        ));
    }

    // 如果目标目录已存在，先删除
    if destination.exists() {
        fs::remove_dir_all(destination).map_err(|e| {
            InstallerError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to remove existing destination directory: {}", e),
            ))
        })?;
    }

    // 确保目标父目录存在
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            InstallerError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to create parent directory: {}", e),
            ))
        })?;
    }

    // 移动目录
    fs::rename(source, destination).map_err(|e| {
        InstallerError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Failed to move directory: {}", e),
        ))
    })?;

    Ok(())
}

/// 检查目录是否存在
pub fn directory_exists(path: &Path) -> bool {
    path.exists() && path.is_dir()
}

/// 检查文件是否存在
pub fn file_exists(path: &Path) -> bool {
    path.exists() && path.is_file()
}

/// 读取文件内容
pub fn read_file_content(path: &Path) -> Result<String> {
    if !path.exists() {
        return Err(InstallerError::FileNotFound(
            path.to_string_lossy().to_string(),
        ));
    }

    let mut file = File::open(path).map_err(|e| {
        InstallerError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Failed to open file: {}", e),
        ))
    })?;

    let mut content = String::new();
    file.read_to_string(&mut content).map_err(|e| {
        InstallerError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Failed to read file: {}", e),
        ))
    })?;

    Ok(content)
}

/// 写入文件内容
pub fn write_file_content(path: &Path, content: &str) -> Result<()> {
    // 确保目标目录存在
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            InstallerError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to create parent directory: {}", e),
            ))
        })?;
    }

    fs::write(path, content).map_err(|e| {
        InstallerError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Failed to write file: {}", e),
        ))
    })?;

    Ok(())
}
