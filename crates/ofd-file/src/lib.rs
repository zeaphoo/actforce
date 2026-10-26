//! `ofd-file` — parse and convert OFD (Open Fixed-layout Document) files.
//!
//! See [`OfdDocument`] for the main entry point.
//!
//! ```no_run
//! use ofd_file::OfdDocument;
//!
//! let mut doc = OfdDocument::open("invoice.ofd").unwrap();
//! let md = doc.to_markdown().unwrap();
//! std::fs::write("invoice.md", md).unwrap();
//!
//! # #[cfg(feature = "pdf")]
//! doc.to_pdf_file("invoice.pdf").unwrap();
//! ```

pub mod archive;
pub mod document;
pub mod error;
pub mod extract;
pub mod font;
pub mod model;
pub mod page_parser;
pub mod path;
pub mod types;

#[cfg(feature = "pdf")]
pub mod pdf;

#[cfg(feature = "render-cpu")]
pub mod render;

pub use document::{LoadedPage, OfdDocument, ResolvedMedia};
pub use error::{OfdError, Result};
pub use extract::TextRun;
pub use types::{Boundary, Matrix, RgbColor};

#[cfg(feature = "pdf")]
pub use pdf::PdfOptions;

#[cfg(feature = "render-cpu")]
pub use render::PngOptions;
