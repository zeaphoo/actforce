use thiserror::Error;

#[derive(Error, Debug)]
pub enum InstallerError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Docker command failed: {0}")]
    DockerCommand(String),

    #[error("Docker Compose command failed: {0}")]
    DockerComposeCommand(String),

    #[error("Failed to extract package: {0}")]
    PackageExtraction(String),

    #[error("Directory not found: {0}")]
    DirectoryNotFound(String),

    #[error("File not found: {0}")]
    FileNotFound(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Configuration error: {0}")]
    Configuration(String),

    #[error("Configuration file not found in {0}: {1}")]
    ConfigNotFound(String, String),

    #[error("User cancelled operation")]
    UserCancelled,
    
    #[error("User aborted operation: {0}")]
    UserAbort(String),

    #[error("Unknown error: {0}")]
    Unknown(String),
    
    #[error("Command failed: {0}")]
    CommandFailed(String),

    #[error("Not a directory: {0}")]
    NotADirectory(String),

    #[error("Docker is not available")]
    DockerNotAvailable,

    #[error("{0}")]
    AnyhowError(String),
}

pub type Result<T> = std::result::Result<T, InstallerError>;

impl From<anyhow::Error> for InstallerError {
    fn from(error: anyhow::Error) -> Self {
        InstallerError::AnyhowError(error.to_string())
    }
}
