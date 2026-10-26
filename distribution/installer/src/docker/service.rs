use std::path::{Path, PathBuf};
use crate::utils::docker::DockerExecutor;
use crate::utils::error::{InstallerError, Result};
use super::compose::ComposeConfig;
use crate::i18n::t;

pub struct ActForceService {
    docker_executor: DockerExecutor,
    service_dir: PathBuf,
    docker_dir: PathBuf,
    compose_config: ComposeConfig,
}

impl ActForceService {
    pub fn new(service_dir: &Path) -> Result<Self> {
        let docker_executor = DockerExecutor::new()?;

        let docker_dir = service_dir.join("docker");
        if !docker_dir.exists() {
            return Err(InstallerError::DirectoryNotFound(
                format!("{}: {}", t("docker_dir_not_found"), docker_dir.display())
            ));
        }

        let compose_config = ComposeConfig::new(&docker_dir)?;

        Ok(Self {
            docker_executor,
            service_dir: service_dir.to_path_buf(),
            docker_dir: docker_dir.to_path_buf(),
            compose_config,
        })
    }

    pub fn load_images(&self, images_dir: &Path) -> Result<()> {
        if !images_dir.exists() || !images_dir.is_dir() {
            return Err(InstallerError::DirectoryNotFound(
                images_dir.to_string_lossy().to_string()
            ).into());
        }

        log::info!("{}: {}", t("loading_images_from"), images_dir.display());

        let entries = std::fs::read_dir(images_dir)?;
        let mut image_files = Vec::new();

        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            if path.is_file() {
                let is_tar = path.extension().map_or(false, |ext| ext.to_string_lossy().to_lowercase() == "tar");

                let is_compressed = path.extension().map_or(false, |ext| {
                    let ext_str = ext.to_string_lossy().to_lowercase();
                    ext_str == "gz" || ext_str == "gzip" || ext_str == "tgz"
                });

                let is_tar_gz = if is_compressed {
                    if let Some(stem) = path.file_stem() {
                        let stem_str = stem.to_string_lossy();
                        stem_str.ends_with(".tar") || path.extension().map_or(false, |ext| ext.to_string_lossy() == "tgz")
                    } else {
                        false
                    }
                } else {
                    false
                };

                if is_tar || is_tar_gz {
                    log::debug!("Found image file: {}", path.display());
                    image_files.push(path);
                }
            }
        }

        let total_images = image_files.len();

        if total_images == 0 {
            log::warn!("{}: {}", t("no_image_files"), images_dir.display());
            return Ok(());
        }

        log::info!("{}: {}", t("found_image_files"), total_images);

        log::info!("{}", t("loading_images_manually"));

        for (index, path) in image_files.iter().enumerate() {
            log::info!("{}", t("loading_image"));
            self.docker_executor.load_image(path)?;
        }

        log::info!("{}", t("loaded_images"));

        Ok(())
    }

    pub fn start(&self) -> Result<()> {
        log::info!("{}", t("starting_actforce_service"));

        self.compose_config.validate()?;

        self.docker_executor.compose_up(
            &self.docker_dir,
            self.compose_config.compose_file()
        )?;

        log::info!("{}", t("service_started_successfully"));
        Ok(())
    }

    pub fn stop(&self) -> Result<()> {
        log::info!("{}", t("stopping_actforce_service"));

        self.docker_executor.compose_down(
            &self.docker_dir,
            self.compose_config.compose_file()
        )?;

        log::info!("{}", t("service_stopped_successfully"));
        Ok(())
    }

    pub fn is_running(&self) -> Result<bool> {
        log::info!("{}", t("checking_service_status"));

        Ok(self.docker_executor.compose_is_running(
            &self.docker_dir,
            self.compose_config.compose_file()
        )?)
    }
}
