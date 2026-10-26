use std::path::{Path, PathBuf};
use std::fs;
use crate::utils::error::{InstallerError, Result};
use crate::i18n::t;

pub struct ComposeConfig {
    compose_file: PathBuf,
    env_file: Option<PathBuf>,
}

impl ComposeConfig {
    pub fn new(dir: &Path) -> Result<Self> {
        let compose_file = if dir.join("docker-compose.yml").exists() {
            dir.join("docker-compose.yml")
        } else if dir.join("docker-compose.yaml").exists() {
            dir.join("docker-compose.yaml")
        } else {
            return Err(InstallerError::ConfigNotFound(
                dir.to_string_lossy().to_string(),
                "docker-compose.yml or docker-compose.yaml".to_string(),
            ).into());
        };

        log::info!("{}: {}", t("checking_env_file"), dir.display());

        let env_path = dir.join(".env");
        let example_path = dir.join(".env.example");

        let env_file = if env_path.exists() {
            log::info!("{}: {}", t("env_file_found"), env_path.display());
            Some(env_path)
        } else if example_path.exists() {
            log::info!("{}: {}", t("env_example_found"), example_path.display());
            log::info!("{}", t("creating_env_file"));

            let example_content = match fs::read_to_string(&example_path) {
                Ok(content) => content,
                Err(e) => {
                    log::error!("{}: {}", t("failed_read_env_example"), e);
                    return Err(InstallerError::Io(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("{}: {}", t("failed_read_env_example"), e),
                    )));
                }
            };

            match fs::write(&env_path, &example_content) {
                Ok(_) => {
                    log::info!("{}", t("env_file_created"));
                    Some(env_path)
                },
                Err(e) => {
                    log::error!("{}: {}", t("failed_create_env"), e);
                    return Err(InstallerError::Io(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("{}: {}", t("failed_create_env"), e),
                    )));
                }
            }
        } else {
            log::warn!("{}: {}", t("no_env_files"), dir.display());
            log::warn!("{}", t("services_may_not_start"));
            None
        };

        Ok(Self {
            compose_file,
            env_file,
        })
    }

    pub fn compose_file(&self) -> &Path {
        &self.compose_file
    }

    pub fn env_file(&self) -> Option<&Path> {
        self.env_file.as_deref()
    }

    pub fn validate(&self) -> Result<()> {
        if !self.compose_file.exists() {
            return Err(InstallerError::FileNotFound(
                self.compose_file.to_string_lossy().to_string()
            ).into());
        }

        if let Some(env_file) = &self.env_file {
            if !env_file.exists() {
                return Err(InstallerError::FileNotFound(
                    env_file.to_string_lossy().to_string()
                ).into());
            }
        }

        Ok(())
    }
}
