//! OFD archive (zip) reader with path-resolution helpers.

use std::io::{BufReader, Cursor, Read, Seek};
use std::path::{Path, PathBuf};

use zip::ZipArchive;

use crate::error::{OfdError, Result};

/// A generic seekable reader wrapper for zip.
pub enum ArchiveSource {
    File(BufReader<std::fs::File>),
    Memory(Cursor<Vec<u8>>),
}

impl Read for ArchiveSource {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            ArchiveSource::File(r) => r.read(buf),
            ArchiveSource::Memory(r) => r.read(buf),
        }
    }
}

impl Seek for ArchiveSource {
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        match self {
            ArchiveSource::File(r) => r.seek(pos),
            ArchiveSource::Memory(r) => r.seek(pos),
        }
    }
}

pub struct OfdArchive {
    zip: ZipArchive<ArchiveSource>,
}

impl OfdArchive {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file = std::fs::File::open(path)?;
        let src = ArchiveSource::File(BufReader::new(file));
        Ok(Self { zip: ZipArchive::new(src)? })
    }

    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self> {
        let src = ArchiveSource::Memory(Cursor::new(bytes));
        Ok(Self { zip: ZipArchive::new(src)? })
    }

    pub fn file_names(&self) -> Vec<String> {
        self.zip.file_names().map(|s| s.to_string()).collect()
    }

    /// Read entry by normalized (forward-slash) path.
    pub fn read_bytes(&mut self, path: &str) -> Result<Vec<u8>> {
        let p = normalize(path);
        let mut f = self
            .zip
            .by_name(&p)
            .map_err(|_| OfdError::EntryNotFound(p.clone()))?;
        let mut buf = Vec::with_capacity(f.size() as usize);
        f.read_to_end(&mut buf)?;
        Ok(buf)
    }

    pub fn read_string(&mut self, path: &str) -> Result<String> {
        let bytes = self.read_bytes(path)?;
        // Strip UTF-8 BOM if any.
        let s = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
            String::from_utf8_lossy(&bytes[3..]).into_owned()
        } else {
            String::from_utf8_lossy(&bytes).into_owned()
        };
        Ok(s)
    }
}

/// Normalize a path inside an OFD zip: strip leading slashes and
/// convert backslashes to forward slashes.
pub fn normalize(p: &str) -> String {
    let mut s = p.replace('\\', "/");
    while let Some(stripped) = s.strip_prefix('/') {
        s = stripped.to_string();
    }
    s
}

/// Resolve a relative path against a base xml file's directory, OFD-style.
/// If `relative` begins with '/' it is treated as rooted in the zip.
pub fn resolve(base_xml: &str, relative: &str) -> String {
    if relative.starts_with('/') {
        return normalize(relative);
    }
    let base = Path::new(base_xml);
    let parent = base.parent().unwrap_or_else(|| Path::new(""));
    let joined: PathBuf = parent.join(relative);
    normalize(&joined.to_string_lossy())
}
