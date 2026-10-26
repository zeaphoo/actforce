use thiserror::Error;

#[derive(Debug, Error)]
pub enum OfdError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Zip error: {0}")]
    Zip(#[from] zip::result::ZipError),

    #[error("XML parse error: {0}")]
    Xml(#[from] serde_xml_rs::Error),

    #[error("OFD structure invalid: {0}")]
    Structure(String),

    #[error("Entry not found in OFD archive: {0}")]
    EntryNotFound(String),

    #[error("Unsupported: {0}")]
    Unsupported(String),

    #[cfg(feature = "pdf")]
    #[error("PDF error: {0}")]
    Pdf(String),

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, OfdError>;
