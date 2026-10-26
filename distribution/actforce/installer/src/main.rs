mod docker;
mod utils;
mod i18n;

use clap::{Parser, Subcommand, ValueEnum};
use colored::*;
use docker::service::ActForceService;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::fs::{create_dir_all, remove_dir_all};
use utils::error::{InstallerError, Result};
use utils::fs;
use utils::logger;
use i18n::{Language, set_language, t};

#[derive(Clone, ValueEnum)]
enum LanguageArg {
    En,
    Zh,
}

impl From<LanguageArg> for Language {
    fn from(lang: LanguageArg) -> Self {
        match lang {
            LanguageArg::En => Language::English,
            LanguageArg::Zh => Language::Chinese,
        }
    }
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[arg(short, long, value_enum)]
    language: Option<LanguageArg>,

    #[arg(short, long)]
    verbose: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Install ActForce
    Install {
        /// Path to the ActForce package file
        #[arg(required = true)]
        package_path: PathBuf,

        /// Installation directory
        #[arg(short, long)]
        install_dir: Option<PathBuf>,
    },

    /// Remove an existing ActForce installation
    Remove {
        /// Installation directory
        #[arg(short, long)]
        install_dir: Option<PathBuf>,
    },

    /// Load Docker images
    LoadImages {
        /// Directory containing Docker image files (.tar.gz)
        #[arg(short, long, default_value = "./docker-images")]
        images_dir: PathBuf,

        /// Load all images without interactive prompts
        #[arg(short, long)]
        all: bool,
    },
}

fn get_default_install_dir() -> PathBuf {
    if is_root() {
        PathBuf::from("/app")
    } else {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(home).join("app")
    }
}

fn is_root() -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        std::fs::metadata("/").map(|m| m.uid() == 0).unwrap_or(false)
    }

    #[cfg(not(unix))]
    {
        false
    }
}

fn ask_confirmation(prompt: &str) -> Result<bool> {
    print!("{} (y/n): ", prompt);
    io::stdout().flush().map_err(|e| {
        InstallerError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Failed to flush stdout: {}", e),
        ))
    })?;

    let mut input = String::new();
    io::stdin().read_line(&mut input).map_err(|e| {
        InstallerError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Failed to read input: {}", e),
        ))
    })?;

    let input = input.trim().to_lowercase();
    Ok(input == "y" || input == "yes")
}

fn check_existing_installation(install_dir: &Path) -> Result<bool> {
    let docker_dir = install_dir.join("actforce").join("docker");
    let compose_yml = docker_dir.join("docker-compose.yml");
    let compose_yaml = docker_dir.join("docker-compose.yaml");

    Ok(docker_dir.exists() && (compose_yml.exists() || compose_yaml.exists()))
}

fn install_actforce(package_path: &Path, install_dir: &Path) -> Result<()> {
    println!("{}", format!("\n=== {} ===", t("installing")).blue().bold());
    log::info!("{}: {}", t("installation_dir"), install_dir.display());

    std::fs::create_dir_all(install_dir).map_err(|e| {
        InstallerError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Failed to create installation directory: {}", e),
        ))
    })?;

    let has_existing = check_existing_installation(install_dir)?;
    let is_upgrade = if has_existing {
        println!(
            "\n{} {} {}",
            "[!]".yellow().bold(),
            t("existing_installation_detected").bold(),
            t("upgrade_or_fresh")
        );
        println!("{}", "1) Upgrade (migrate config)".green());
        println!("{}", "2) Fresh install (stop services, replace all)".yellow());
        println!("{}", "3) Cancel".red());

        let mut choice = String::new();
        print!("\n{}: ", t("enter_choice"));
        io::stdout().flush().unwrap();
        io::stdin().read_line(&mut choice).unwrap();

        match choice.trim() {
            "1" => true,
            "2" => false,
            _ => {
                println!("{}", t("installation_aborted").red());
                return Err(InstallerError::UserAbort(t("installation_aborted").to_string()));
            }
        }
    } else {
        false
    };

    log::info!("{}: {}", t("extracting_package"), package_path.display());
    let extracted_dir = fs::extract_package(package_path, install_dir)?;
    log::info!("{}: {}", t("package_extracted"), extracted_dir.display());

    let actforce_dir = install_dir.join("actforce");
    let new_docker_dir = extracted_dir.join("docker");

    if !new_docker_dir.exists() {
        return Err(InstallerError::DirectoryNotFound(format!(
            "Docker directory not found in package: {}",
            new_docker_dir.display()
        )));
    }

    if is_upgrade {
        println!("{}", format!("\n=== {} ===", t("upgrading")).blue().bold());

        let service = ActForceService::new(&actforce_dir)?;
        let docker_dir = actforce_dir.join("docker");

        // 1. Backup .env
        log::info!("{}", t("backing_up_config"));
        let env_file = docker_dir.join(".env");
        let env_backup = actforce_dir.join("env_backup.env");
        if env_file.exists() {
            fs::copy_file(&env_file, &env_backup)?;
            log::info!("Backed up .env to: {}", env_backup.display());
        }

        // 2. Stop services
        log::info!("{}", t("stopping_services"));
        if let Err(e) = service.stop() {
            log::warn!("Failed to stop services: {}", e);
        }

        // 3. Migrate .env configuration
        let new_env_file = new_docker_dir.join(".env");
        let new_env_example = new_docker_dir.join(".env.example");
        if env_backup.exists() && new_env_example.exists() {
            let old_env = std::fs::read_to_string(&env_backup)?;
            let new_example = std::fs::read_to_string(&new_env_example)?;

            let mut merged_env = old_env.clone();
            for line in new_example.lines() {
                if line.starts_with('#') || line.trim().is_empty() {
                    continue;
                }
                if let Some(key) = line.split('=').next() {
                    let key = key.trim();
                    if !old_env.lines().any(|l| l.starts_with(key) && l.contains('=')) {
                        merged_env.push_str("\n");
                        merged_env.push_str(line);
                    }
                }
            }
            std::fs::write(&new_env_file, merged_env)?;
            log::info!("Config migrated");
        } else if env_backup.exists() {
            fs::copy_file(&env_backup, &new_env_file)?;
        }

        // 4. Backup old docker directory, deploy new
        let backup_dir = fs::backup_directory(&actforce_dir)?;
        log::info!("Old version backed up to: {}", backup_dir.display());

        if docker_dir.exists() {
            remove_dir_all(&docker_dir)?;
        }
        fs::move_directory(&new_docker_dir, &actforce_dir.join("docker"))?;

        // 5. Restore .env
        if env_backup.exists() {
            fs::copy_file(&env_backup, &actforce_dir.join("docker/.env"))?;
            fs::remove_file(&env_backup)?;
        }

        // 6. Load new images and start
        let service = ActForceService::new(&actforce_dir)?;
        let images_dir = actforce_dir.join("docker").join("docker-images");
        if images_dir.exists() {
            service.load_images(&images_dir)?;
        }
        service.start()?;

        // 7. Clean up dangling images
        let docker_executor = utils::docker::DockerExecutor::new()?;
        if let Err(e) = docker_executor.prune_images(false) {
            log::warn!("Image cleanup failed: {}", e);
        }

        logger::log_success(&t("upgrade_completed"));
        log::info!("{}: {}", t("rollback_info"), backup_dir.display());
    } else {
        // Fresh install
        if has_existing {
            if let Ok(service) = ActForceService::new(&actforce_dir) {
                if let Err(e) = service.stop() {
                    log::warn!("Failed to stop services: {}", e);
                }
            }
            let backup_dir = fs::backup_directory(&actforce_dir)?;
            log::info!("Old installation backed up to: {}", backup_dir.display());

            let docker_dir = actforce_dir.join("docker");
            if docker_dir.exists() {
                remove_dir_all(&docker_dir)?;
            }
        }

        log::info!("{}", t("moving_files"));
        fs::move_directory(&new_docker_dir, &actforce_dir.join("docker"))?;

        let service = ActForceService::new(&actforce_dir)?;
        let images_dir = actforce_dir.join("docker").join("docker-images");
        if images_dir.exists() {
            service.load_images(&images_dir)?;
        }
        service.start()?;

        logger::log_success(&t("installation_completed"));
    }

    println!("{}", format!("\n=== {} ===", t("usage_instructions")).blue().bold());
    println!("{}: {}", t("installation_dir"), actforce_dir.display().to_string().green());
    println!(
        "{}: {}",
        t("start_command"),
        format!("cd {}/docker && docker compose up -d", actforce_dir.display()).yellow()
    );
    println!(
        "{}: {}",
        t("stop_command"),
        format!("cd {}/docker && docker compose down", actforce_dir.display()).yellow()
    );
    println!("{}: {}", t("access_url"), "http://localhost:5000".green());
    println!("{}", "=======================".blue().bold());

    Ok(())
}

fn remove_actforce(install_dir: &Path) -> Result<()> {
    println!("{}", format!("\n=== {} ===", t("removing")).blue().bold());
    log::info!("{}: {}", t("installation_dir"), install_dir.display());

    let actforce_dir = install_dir.join("actforce");
    if !actforce_dir.exists() {
        log::error!("{}", t("dir_not_found"));
        return Err(InstallerError::DirectoryNotFound(actforce_dir.to_string_lossy().to_string()));
    }

    let docker_dir = actforce_dir.join("docker");
    if !docker_dir.exists() {
        log::error!("Docker directory not found: {}", docker_dir.display());
        return Err(InstallerError::DirectoryNotFound(docker_dir.to_string_lossy().to_string()));
    }

    let env_file = docker_dir.join(".env");
    if env_file.exists() {
        let backup_dir = actforce_dir.join("backups");
        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S").to_string();
        let backup_env_file = backup_dir.join(format!("env.backup_{}", timestamp));

        log::info!("Backing up .env file to {}", backup_env_file.display());

        if !backup_dir.exists() {
            create_dir_all(&backup_dir)?;
        }

        std::fs::copy(&env_file, &backup_env_file)?;
        log::info!("Environment file backed up");
    }

    log::info!("Stopping Docker services...");
    let docker_executor = utils::docker::DockerExecutor::new()?;

    let compose_file = if docker_dir.join("docker-compose.yaml").exists() {
        docker_dir.join("docker-compose.yaml")
    } else if docker_dir.join("docker-compose.yml").exists() {
        docker_dir.join("docker-compose.yml")
    } else {
        log::warn!("docker-compose config file not found, skipping service stop");
        PathBuf::new()
    };

    if !compose_file.as_os_str().is_empty() {
        match docker_executor.compose_down(&docker_dir, &compose_file) {
            Ok(_) => log::info!("Docker services stopped"),
            Err(e) => log::warn!("Error stopping Docker services: {}", e),
        }
    }

    log::info!("Removing Docker directory: {}", docker_dir.display());
    match remove_dir_all(&docker_dir) {
        Ok(_) => log::info!("Docker directory removed"),
        Err(e) => {
            log::error!("Failed to remove Docker directory: {}", e);
            return Err(InstallerError::Io(e));
        }
    }

    log::info!("Cleaning up unused Docker images...");
    match docker_executor.prune_images(true) {
        Ok(_) => log::info!("Unused Docker images cleaned up"),
        Err(e) => log::warn!("Error cleaning up Docker images: {}", e),
    }

    logger::log_success("ActForce Docker components removed successfully");
    log::info!("Other data remains in {}", actforce_dir.display());
    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    logger::init(cli.verbose);

    i18n::init();

    if let Some(lang) = cli.language {
        set_language(lang.into());
    }

    match cli.command {
        Commands::Install {
            package_path,
            install_dir,
        } => {
            let install_dir = install_dir.unwrap_or_else(get_default_install_dir);
            install_actforce(&package_path, &install_dir)
        }
        Commands::Remove {
            install_dir,
        } => {
            let install_dir = install_dir.unwrap_or_else(get_default_install_dir);
            remove_actforce(&install_dir)
        }
        Commands::LoadImages {
            images_dir,
            all,
        } => {
            if !utils::docker::is_docker_available() {
                return Err(InstallerError::DockerNotAvailable.into());
            }

            if all {
                Ok(docker::images::load_all_images(&images_dir)?)
            } else {
                Ok(docker::images::interactive_load_images(&images_dir)?)
            }
        }
    }
}
