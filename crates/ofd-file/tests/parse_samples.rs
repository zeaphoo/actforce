//! End-to-end tests that parse bundled OFD samples from `.links/` (if present).
//!
//! The `.links/` directory contains symlinks/copies of reference OFD projects
//! and sample files, which are not part of this crate. If those paths do not
//! exist at test time, tests silently pass so CI without them still works.

use std::path::PathBuf;

use ofd_file::OfdDocument;

fn workspace_root() -> PathBuf {
    // crates/ofd-file -> crates -> <root>
    let cargo_manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    cargo_manifest.parent().unwrap().parent().unwrap().to_path_buf()
}

fn samples() -> Vec<PathBuf> {
    let root = workspace_root();
    let candidates = [
        "crates/ofd-file/tests/fixtures/sample.ofd",
        ".links/rofd/learning/test.ofd",
        ".links/ofdrs/ofd-test/assets/1/ofd.ofd",
        ".links/ofdrs/ofd-test/assets/2/ofd.ofd",
        ".links/ofdrs/ofd-test/assets/3/ofd.ofd",
    ];
    candidates
        .iter()
        .map(|p| root.join(p))
        .filter(|p| p.exists())
        .collect()
}

#[test]
fn opens_and_reports_pages() {
    let files = samples();
    if files.is_empty() {
        eprintln!("no sample OFD files found, skipping");
        return;
    }
    for f in files {
        let doc = OfdDocument::open(&f)
            .unwrap_or_else(|e| panic!("failed to open {}: {e}", f.display()));
        assert!(doc.page_count() > 0, "{} should have >=1 page", f.display());
    }
}

#[test]
fn extracts_markdown() {
    let Some(f) = samples().into_iter().next() else { return };
    let doc = OfdDocument::open(&f).unwrap();
    let md = doc.to_markdown().unwrap();
    assert!(!md.is_empty(), "markdown for {} must not be empty", f.display());
    assert!(md.contains("<!-- page 1 -->"), "must include page marker");
}

#[cfg(feature = "pdf")]
#[test]
fn writes_pdf() {
    let Some(f) = samples().into_iter().next() else { return };
    let mut doc = OfdDocument::open(&f).unwrap();
    let bytes = doc.to_pdf().unwrap();
    assert!(bytes.starts_with(b"%PDF-"), "output must be a PDF");
}

#[cfg(feature = "render-cpu")]
#[test]
fn writes_png() {
    let Some(f) = samples().into_iter().next() else { return };
    let mut doc = OfdDocument::open(&f).unwrap();
    let opts = ofd_file::PngOptions::new().with_dpi(96.0);
    let bytes = doc.to_png_page(0, &opts).unwrap();
    assert!(bytes.starts_with(&[0x89, b'P', b'N', b'G']), "output must be a PNG");
}
