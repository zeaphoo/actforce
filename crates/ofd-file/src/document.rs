//! High-level `OfdDocument` API.

use std::collections::HashMap;
use std::path::Path;

use crate::archive::{resolve, OfdArchive};
use crate::error::{OfdError, Result};
use crate::model::{Annotations, Document, DocInfo, Font, MultiMedia, OfdRoot, Page, Res};
use crate::page_parser::{parse_page, parse_page_annot};
use crate::types::Boundary;

/// A loaded, parsed OFD document. Owns the zip archive + all parsed metadata.
pub struct OfdDocument {
    pub(crate) archive: OfdArchive,

    pub(crate) doc_info: DocInfo,
    #[allow(dead_code)]
    pub(crate) doc_root_path: String,
    #[allow(dead_code)]
    pub(crate) document: Document,

    pub(crate) fonts: HashMap<u64, Font>,
    pub(crate) medias: HashMap<u64, ResolvedMedia>,

    pub(crate) pages: Vec<LoadedPage>,
    pub(crate) annotations_by_page: HashMap<u64, Vec<AnnotObject>>,
}

#[derive(Debug, Clone)]
pub struct ResolvedMedia {
    pub meta: MultiMedia,
    /// Full path inside the zip.
    pub path: String,
}

#[derive(Debug)]
pub struct LoadedPage {
    pub id: u64,
    pub base_loc: String,
    pub full_path: String,
    pub area: Boundary,
    pub page: Page,
}

#[derive(Debug, Clone)]
pub struct AnnotObject {
    pub boundary: Boundary,
    pub objects: Vec<crate::model::PageObject>,
}

impl OfdDocument {
    /// Open an OFD file on disk.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let archive = OfdArchive::open(path)?;
        Self::from_archive(archive)
    }

    /// Open an OFD from raw bytes.
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self> {
        let archive = OfdArchive::from_bytes(bytes)?;
        Self::from_archive(archive)
    }

    fn from_archive(mut archive: OfdArchive) -> Result<Self> {
        // 1. OFD.xml
        let ofd_xml = archive.read_string("OFD.xml")?;
        let ofd_root = OfdRoot::from_xml(&ofd_xml)?;
        let doc_body = ofd_root
            .doc_body
            .into_iter()
            .next()
            .ok_or_else(|| OfdError::Structure("OFD.xml has no DocBody".into()))?;
        let doc_root_path = doc_body.doc_root.clone();
        let doc_info = doc_body.doc_info;

        // 2. Document.xml
        let document_xml = archive.read_string(&doc_root_path)?;
        let document = Document::from_xml(&document_xml)?;

        // 3. Resources
        let mut fonts = HashMap::new();
        let mut medias: HashMap<u64, ResolvedMedia> = HashMap::new();

        for res_rel in document
            .common_data
            .public_res
            .iter()
            .chain(document.common_data.document_res.iter())
        {
            let res_path = resolve(&doc_root_path, res_rel);
            let res_xml = match archive.read_string(&res_path) {
                Ok(x) => x,
                Err(_) => continue,
            };
            let res = match Res::from_xml(&res_xml) {
                Ok(r) => r,
                Err(e) => {
                    log::warn!("failed to parse Res {}: {}", res_path, e);
                    continue;
                }
            };
            let base_dir = res
                .base_loc
                .as_deref()
                .map(|b| resolve(&res_path, b))
                .unwrap_or_else(|| {
                    Path::new(&res_path)
                        .parent()
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_default()
                });

            if let Some(fs) = res.fonts {
                for f in fs.font {
                    fonts.insert(f.id, f);
                }
            }
            if let Some(mm) = res.multi_medias {
                for m in mm.multi_media {
                    let full = if base_dir.is_empty() {
                        m.media_file.clone()
                    } else {
                        format!("{}/{}", base_dir.trim_end_matches('/'), m.media_file)
                    };
                    medias.insert(
                        m.id,
                        ResolvedMedia {
                            path: crate::archive::normalize(&full),
                            meta: m,
                        },
                    );
                }
            }
        }

        // 4. Pages
        let page_area = Boundary::parse(&document.common_data.page_area.physical_box)
            .unwrap_or_default();

        let mut pages = Vec::new();
        for pref in &document.pages.page {
            let (Some(base_loc), Some(id)) = (pref.base_loc.as_ref(), pref.id) else {
                continue;
            };
            let full_path = resolve(&doc_root_path, base_loc);
            let xml = archive.read_string(&full_path)?;
            let page = parse_page(&xml)?;
            let area = page
                .area
                .as_ref()
                .and_then(|a| Boundary::parse(&a.physical_box).ok())
                .unwrap_or(page_area);
            pages.push(LoadedPage {
                id,
                base_loc: base_loc.clone(),
                full_path,
                area,
                page,
            });
        }

        // 5. Annotations
        let mut annotations_by_page: HashMap<u64, Vec<AnnotObject>> = HashMap::new();
        if let Some(annots_rel) = &document.annotations {
            let annots_path = resolve(&doc_root_path, annots_rel);
            if let Ok(xml) = archive.read_string(&annots_path) {
                if let Ok(annots) = Annotations::from_xml(&xml) {
                    for p in annots.page {
                        let annot_path = resolve(&annots_path, &p.file_loc);
                        let Ok(xml) = archive.read_string(&annot_path) else { continue };
                        let Ok(pa) = parse_page_annot(&xml) else { continue };
                        let mut entries = Vec::new();
                        for a in pa.annot {
                            if let Some(app) = a.appearance {
                                let b = Boundary::parse(&app.boundary).unwrap_or_default();
                                let objects = app.flatten_objects();
                                entries.push(AnnotObject { boundary: b, objects });
                            }
                        }
                        annotations_by_page.entry(p.page_id).or_default().extend(entries);
                    }
                }
            }
        }

        Ok(Self {
            archive,
            doc_info,
            doc_root_path,
            document,
            fonts,
            medias,
            pages,
            annotations_by_page,
        })
    }

    // ---------- Accessors ----------

    pub fn doc_info(&self) -> &DocInfo { &self.doc_info }
    pub fn pages(&self) -> &[LoadedPage] { &self.pages }
    pub fn page_count(&self) -> usize { self.pages.len() }

    pub fn font(&self, id: u64) -> Option<&Font> { self.fonts.get(&id) }
    pub fn media(&self, id: u64) -> Option<&ResolvedMedia> { self.medias.get(&id) }

    pub fn annotations_for(&self, page_id: u64) -> &[AnnotObject] {
        self.annotations_by_page
            .get(&page_id)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Read raw bytes for a resolved media resource.
    pub fn read_media_bytes(&mut self, id: u64) -> Result<Vec<u8>> {
        let path = self
            .medias
            .get(&id)
            .ok_or_else(|| OfdError::Other(format!("media {id} not found")))?
            .path
            .clone();
        self.archive.read_bytes(&path)
    }

    /// Read raw bytes by zip entry name.
    pub fn read_entry(&mut self, path: &str) -> Result<Vec<u8>> {
        self.archive.read_bytes(path)
    }

    pub fn list_entries(&self) -> Vec<String> { self.archive.file_names() }
}
