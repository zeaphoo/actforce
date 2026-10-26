use crate::types::AuPageDocument;
use anyhow::Result;
use std::path::Path;

/// Parse an AuPage document from a JSON file
pub fn parse_file(path: &Path) -> Result<AuPageDocument> {
    let content = std::fs::read_to_string(path)?;
    parse_json(&content)
}

/// Parse an AuPage document from a JSON string
pub fn parse_json(json: &str) -> Result<AuPageDocument> {
    let doc: AuPageDocument = serde_json::from_str(json)?;
    Ok(doc)
}

/// Validate basic document structure
pub fn validate(doc: &AuPageDocument) -> Result<()> {
    if doc.version != "1.0" {
        anyhow::bail!("Unsupported AuPage version: {}. Expected 1.0", doc.version);
    }
    if doc.title.is_empty() {
        anyhow::bail!("Document title cannot be empty");
    }
    if doc.elements.is_empty() {
        anyhow::bail!("Document must contain at least one element");
    }
    Ok(())
}
