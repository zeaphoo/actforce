use crate::utils::error::{InstallerError, Result};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::str;

use std::collections::HashMap;

/// 检查 Docker 是否可用
pub fn is_docker_available() -> bool {
    Command::new("docker")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Docker 命令执行器
pub struct DockerExecutor {
    compose_command: String,
    image_map: HashMap<String, String>,
}

impl DockerExecutor {
    /// 创建新的 Docker 执行器
    pub fn new() -> Result<Self> {
        let compose_command = if Self::is_docker_compose_plugin_available() {
            "docker compose".to_string()
        } else {
            "docker-compose".to_string()
        };

        log::info!("Using Docker Compose command: {}", compose_command);
        
        // 检查 Docker 和 Docker Compose 是否已安装
        Self::check_docker_and_compose_installed()?;
        
        // 初始化镜像映射关系 (单镜像模式)
        let mut image_map = HashMap::new();
        image_map.insert("actforce".to_string(), "actforce:latest".to_string());
        image_map.insert("actforce-cpu".to_string(), "actforce:latest-cpu".to_string());
        image_map.insert("actforce-cuda".to_string(), "actforce:latest-cuda".to_string());
        
        Ok(Self {
            compose_command,
            image_map,
        })
    }

    /// 检查 Docker 是否已安装
    pub fn check_docker_installed() -> bool {
        log::debug!("Checking if Docker is installed");
        let result = Command::new("docker")
            .arg("--version")
            .output()
            .map(|output| {
                let success = output.status.success();
                if success {
                    let version = String::from_utf8_lossy(&output.stdout);
                    log::debug!("Docker is installed: {}", version.trim());
                } else {
                    let error = String::from_utf8_lossy(&output.stderr);
                    log::debug!("Docker check failed: {}", error.trim());
                }
                success
            })
            .unwrap_or_else(|e| {
                log::debug!("Docker command failed: {}", e);
                false
            });
        
        log::debug!("Docker installed: {}", result);
        result
    }

    /// 检查 Docker Compose 插件是否可用
    fn is_docker_compose_plugin_available() -> bool {
        log::debug!("Checking if Docker Compose plugin is available");
        let result = Command::new("docker")
            .args(["compose", "version"])
            .output()
            .map(|output| {
                let success = output.status.success();
                if success {
                    let version = String::from_utf8_lossy(&output.stdout);
                    log::debug!("Docker Compose plugin is available: {}", version.trim());
                } else {
                    let error = String::from_utf8_lossy(&output.stderr);
                    log::debug!("Docker Compose plugin check failed: {}", error.trim());
                }
                success
            })
            .unwrap_or_else(|e| {
                log::debug!("Docker Compose plugin command failed: {}", e);
                false
            });
        
        log::debug!("Docker Compose plugin available: {}", result);
        result
    }

    /// 检查 docker-compose 命令是否可用
    fn is_docker_compose_command_available() -> bool {
        log::debug!("Checking if docker-compose command is available");
        let result = Command::new("docker-compose")
            .arg("--version")
            .output()
            .map(|output| {
                let success = output.status.success();
                if success {
                    let version = String::from_utf8_lossy(&output.stdout);
                    log::debug!("docker-compose command is available: {}", version.trim());
                } else {
                    let error = String::from_utf8_lossy(&output.stderr);
                    log::debug!("docker-compose command check failed: {}", error.trim());
                }
                success
            })
            .unwrap_or_else(|e| {
                log::debug!("docker-compose command failed: {}", e);
                false
            });
        
        log::debug!("docker-compose command available: {}", result);
        result
    }

    /// 检查 Docker 和 Docker Compose 是否已安装
    pub fn check_docker_and_compose_installed() -> Result<()> {
        if !Self::check_docker_installed() {
            return Err(InstallerError::DockerCommand(
                "Docker is not installed".to_string(),
            ));
        }

        if !Self::is_docker_compose_plugin_available() && !Self::is_docker_compose_command_available() {
            return Err(InstallerError::DockerComposeCommand(
                "Docker Compose is not installed".to_string(),
            ));
        }

        Ok(())
    }

    /// 执行 Docker Compose 命令
    pub fn execute_compose_command(
        &self,
        working_dir: &Path,
        compose_file: &Path,
        args: &[&str],
    ) -> Result<Output> {
        let compose_file_abs = if compose_file.is_absolute() {
            compose_file.to_path_buf()
        } else {
            working_dir.join(compose_file)
        };

        if !compose_file_abs.exists() {
            return Err(InstallerError::FileNotFound(
                compose_file_abs.to_string_lossy().to_string(),
            ));
        }

        let command_parts = self.compose_command.split_whitespace().collect::<Vec<_>>();
        let mut command = Command::new(command_parts[0]);
        
        if command_parts.len() > 1 {
            command.args(&command_parts[1..]);
        }
        
        command.arg("-f").arg(&compose_file_abs);
        command.args(args);
        command.current_dir(working_dir);
        
        log::debug!(
            "Executing command: {:?} in directory: {}",
            command,
            working_dir.display()
        );
        
        let output = command.output().map_err(|e| {
            log::debug!("Failed to execute command: {}", e);
            InstallerError::DockerComposeCommand(format!("Failed to execute command: {}", e))
        })?;
        
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            log::debug!("Command succeeded with output: {}", stdout.trim());
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            log::debug!("Command failed with error: {}", stderr.trim());
        }
        
        Ok(output)
    }

    /// 加载单个 Docker 镜像
    pub fn load_image(&self, image_path: &Path) -> Result<()> {
        log::info!("Loading Docker image: {}", image_path.display());
        
        let file_name = image_path.file_name()
            .ok_or_else(|| InstallerError::DockerCommand("Invalid image path".to_string()))?
            .to_string_lossy();
            
        log::debug!("Processing image file: {}", file_name);
        
        let is_compressed = image_path.extension()
            .map_or(false, |ext| {
                let ext_str = ext.to_string_lossy().to_lowercase();
                ext_str == "gz" || ext_str == "gzip" || ext_str == "tgz"
            });
        
        let base_name = if is_compressed {
            if file_name.ends_with(".tar.gz") {
                file_name.strip_suffix(".tar.gz").unwrap_or(&file_name)
            } else if file_name.ends_with(".tgz") {
                file_name.strip_suffix(".tgz").unwrap_or(&file_name)
            } else {
                image_path.file_stem()
                    .and_then(|stem| stem.to_str())
                    .and_then(|stem| {
                        if stem.ends_with(".tar") {
                            Some(stem.strip_suffix(".tar").unwrap_or(stem))
                        } else {
                            Some(stem)
                        }
                    })
                    .unwrap_or(file_name.as_ref())
            }
        } else if file_name.ends_with(".tar") {
            file_name.strip_suffix(".tar").unwrap_or(&file_name)
        } else {
            &file_name
        };
        
        log::debug!("Extracted base name: {}", base_name);
        
        let target_image = self.image_map.get(base_name.to_string().as_str()).cloned();
        
        let target_image = match target_image {
            Some(image) => {
                log::info!("Found predefined image mapping: {} -> {}", base_name, image);
                Some(image)
            },
            None => {
                log::info!("Using base name as image name: {}", base_name);
                Some(format!("actforce:{}", base_name))
            }
        };
        
        let output = if is_compressed {
            log::info!("Loading compressed image file");
            Command::new("sh")
                .arg("-c")
                .arg(format!("gunzip -c '{}' | docker load", image_path.display()))
                .output()
                .map_err(|e| InstallerError::DockerCommand(format!("Failed to load compressed image: {}", e)))?
        } else {
            Command::new("docker")
                .args(["load", "-i"])
                .arg(image_path)
                .output()
                .map_err(|e| InstallerError::DockerCommand(format!("Failed to load image: {}", e)))?
        };
        
        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            log::error!("Failed to load image: {}", error);
            return Err(InstallerError::DockerCommand(format!("Failed to load image: {}", error)));
        }
        
        let stdout = String::from_utf8_lossy(&output.stdout);
        log::info!("Docker load output: {}", stdout.trim());
        
        let loaded_image = if let Some(captures) = regex::Regex::new(r"Loaded image:?\s+([^\s\n]+)").ok().and_then(|re| re.captures(&stdout)) {
            captures.get(1).map(|m| m.as_str().to_string())
        } else if let Some(captures) = regex::Regex::new(r"Loaded image ID: (sha256:[a-f0-9]+)").ok().and_then(|re| re.captures(&stdout)) {
            captures.get(1).map(|m| m.as_str().to_string())
        } else {
            log::warn!("Could not extract image information using regex patterns");
            None
        };
        
        if let Some(loaded_image) = loaded_image {
            log::info!("Extracted loaded image reference: {}", loaded_image);
            
            if let Some(target) = target_image {
                log::info!("Setting tag for image {} -> {}", loaded_image, target);
                
                let tag_output = Command::new("docker")
                    .args(["tag", &loaded_image, &target])
                    .output()
                    .map_err(|e| InstallerError::DockerCommand(format!("Failed to tag image: {}", e)))?;
                
                if !tag_output.status.success() {
                    let error = String::from_utf8_lossy(&tag_output.stderr);
                    log::warn!("Failed to tag image: {}", error);
                } else {
                    log::info!("Successfully tagged image as {}", target);
                }
            }
        }
        
        Ok(())
    }
    
    /// 启动 Docker Compose 服务
    pub fn compose_up(&self, working_dir: &Path, compose_file: &Path) -> Result<()> {
        log::debug!("Starting Docker Compose services in {} with config: {}", 
                   working_dir.display(), compose_file.display());
        let output = self.execute_compose_command(working_dir, compose_file, &["up", "-d"])?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(InstallerError::DockerComposeCommand(format!(
                "Failed to start services: {}",
                stderr
            )));
        }
        
        Ok(())
    }

    /// 停止 Docker Compose 服务
    pub fn compose_down(&self, working_dir: &Path, compose_file: &Path) -> Result<()> {
        log::debug!("Stopping Docker Compose services in {} with config: {}", 
                   working_dir.display(), compose_file.display());
        let output = self.execute_compose_command(working_dir, compose_file, &["down"])?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            log::warn!("Failed to stop services: {}", stderr);
        }
        
        Ok(())
    }

    /// 检查 Docker Compose 服务是否运行
    pub fn compose_is_running(&self, working_dir: &Path, compose_file: &Path) -> Result<bool> {
        let output = self.execute_compose_command(
            working_dir,
            compose_file,
            &["ps", "--services", "--filter", "status=running"]
        )?;

        let services = String::from_utf8_lossy(&output.stdout);
        let is_running = !services.trim().is_empty();
        
        log::info!("Docker Compose 服务状态: {}", if is_running { "运行中" } else { "未运行" });
        Ok(is_running)
    }
    
    /// 清理未使用的 Docker 镜像
    pub fn prune_images(&self, all: bool) -> Result<()> {
        log::info!("清理未使用的 Docker 镜像...");
        
        let mut cmd = Command::new("docker");
        cmd.arg("image").arg("prune").arg("-f");
        
        if all {
            cmd.arg("-a");
        }
        
        let output = cmd.output()
            .map_err(|e| {
                InstallerError::DockerCommand(format!("Failed to execute docker image prune: {}", e))
            })?;
        
        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(InstallerError::DockerCommand(format!("Docker image prune failed: {}", stderr)))
        }
    }
}
