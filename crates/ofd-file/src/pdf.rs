//! PDF export using `krilla` (pure Rust).
//!
//! Coordinate systems: OFD uses a top-left origin with millimetre units.
//! krilla works in PDF points (1 pt = 1/72 inch) with the same top-left
//! orientation, so we push a single `mm -> pt` scale transform per page and
//! keep all OFD-native coordinates unchanged afterwards.

#![cfg(feature = "pdf")]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use krilla::color::rgb;
use krilla::geom::{PathBuilder, Point, Transform as KTransform};
use krilla::num::NormalizedF32;
use krilla::page::PageSettings;
use krilla::paint::{Fill, Stroke};
use krilla::text::{Font, TextDirection};
use krilla::{Document, SerializeSettings};

use crate::archive::resolve;
use crate::document::OfdDocument;
use crate::error::{OfdError, Result};
use crate::font::discover_fallback_font;
use crate::model::{ColorNode, ImageObject, PageObject, PathObject, TextObject};
use crate::path::{parse_abbreviated, PathCmd};
use crate::types::{Boundary, Matrix, RgbColor};

const MM_TO_PT: f32 = 72.0 / 25.4;

/// Options controlling PDF export.
#[derive(Debug, Clone, Default)]
pub struct PdfOptions {
    /// TTF/OTF/TTC path used for text that has no embedded OFD font.
    /// If `None`, [`discover_fallback_font`] is used. If nothing is found,
    /// text is skipped and a warning is emitted.
    pub fallback_font: Option<PathBuf>,
    /// Draw stamp / signature overlays from `Annotations`.
    pub include_stamps: bool,
}

impl PdfOptions {
    pub fn new() -> Self {
        Self { fallback_font: None, include_stamps: true }
    }

    pub fn with_fallback_font<P: Into<PathBuf>>(mut self, path: P) -> Self {
        self.fallback_font = Some(path.into());
        self
    }
}

impl OfdDocument {
    pub fn to_pdf(&mut self) -> Result<Vec<u8>> {
        self.to_pdf_with_options(&PdfOptions::new())
    }

    pub fn to_pdf_file<P: AsRef<Path>>(&mut self, out: P) -> Result<()> {
        self.to_pdf_file_with_options(out, &PdfOptions::new())
    }

    pub fn to_pdf_file_with_options<P: AsRef<Path>>(
        &mut self,
        out: P,
        options: &PdfOptions,
    ) -> Result<()> {
        let bytes = self.to_pdf_with_options(options)?;
        std::fs::write(out, bytes)?;
        Ok(())
    }

    pub fn to_pdf_with_options(&mut self, options: &PdfOptions) -> Result<Vec<u8>> {
        let mut document = Document::new_with(SerializeSettings::default());

        // ---- Fonts ----
        let mut embedded_fonts: std::collections::HashMap<u64, Font> =
            std::collections::HashMap::new();
        let font_entries: Vec<(u64, String)> = self
            .fonts
            .iter()
            .filter_map(|(id, f)| {
                f.font_file
                    .as_ref()
                    .map(|ff| (*id, resolve(&self.doc_root_path, ff)))
            })
            .collect();
        for (fid, path) in font_entries {
            if let Ok(bytes) = self.archive.read_bytes(&path) {
                if let Some(font) = Font::new(Arc::new(bytes).into(), 0) {
                    embedded_fonts.insert(fid, font);
                }
            }
        }

        let fallback_font_bytes = options
            .fallback_font
            .clone()
            .or_else(discover_fallback_font)
            .and_then(|p| std::fs::read(&p).ok())
            .and_then(|bytes| Font::new(Arc::new(bytes).into(), 0));

        if fallback_font_bytes.is_none() && embedded_fonts.is_empty() {
            log::warn!(
                "no embedded OFD fonts and no system fallback font found: \
                 text objects will be skipped in the PDF output. \
                 Set OFD_FILE_CJK_FONT or use PdfOptions::fallback_font."
            );
        }

        // Snapshot page ids for annotation lookup.
        let page_ids: Vec<u64> = self.pages.iter().map(|p| p.id).collect();
        let page_areas: Vec<Boundary> = self.pages.iter().map(|p| p.area).collect();

        // ---- Emit pages ----
        for (idx, area) in page_areas.iter().enumerate() {
            let settings = PageSettings::from_wh(
                area.width as f32 * MM_TO_PT,
                area.height as f32 * MM_TO_PT,
            )
            .ok_or_else(|| OfdError::Pdf(format!("invalid page size {:?}", area)))?;
            let mut page = document.start_page_with(settings);
            let mut surface = page.surface();

            // Switch to OFD-native millimetre coordinates for the rest of the page.
            surface.push_transform(&KTransform::from_scale(MM_TO_PT, MM_TO_PT));

            // Objects from the page itself.
            let objs: Vec<(Boundary, PageObject)> = self.pages[idx]
                .page
                .flatten_objects()
                .into_iter()
                .map(|o| (Boundary::default(), o))
                .collect();

            for (off, obj) in &objs {
                self.emit_object(&mut surface, obj, *off, &embedded_fonts, fallback_font_bytes.as_ref(), false)?;
            }

            // Annotations (stamps).
            if options.include_stamps {
                let page_id = page_ids[idx];
                let annots = self.annotations_by_page.get(&page_id).cloned().unwrap_or_default();
                for annot in annots {
                    for obj in &annot.objects {
                        self.emit_object(
                            &mut surface,
                            obj,
                            annot.boundary,
                            &embedded_fonts,
                            fallback_font_bytes.as_ref(),
                            true,
                        )?;
                    }
                }
            }

            surface.pop();
            surface.finish();
            page.finish();
        }

        let pdf = document
            .finish()
            .map_err(|e| OfdError::Pdf(format!("{e:?}")))?;
        Ok(pdf)
    }

    fn emit_object(
        &mut self,
        surface: &mut krilla::surface::Surface<'_>,
        obj: &PageObject,
        offset: Boundary,
        embedded_fonts: &std::collections::HashMap<u64, Font>,
        fallback_font: Option<&Font>,
        _is_annot: bool,
    ) -> Result<()> {
        match obj {
            PageObject::PathObject(p) => emit_path(surface, p, offset),
            PageObject::TextObject(t) => {
                let font = embedded_fonts.get(&t.font).or(fallback_font).cloned();
                if let Some(font) = font {
                    emit_text(surface, t, offset, font);
                }
                Ok(())
            }
            PageObject::ImageObject(img) => self.emit_image(surface, img, offset),
            PageObject::PageBlock(_) => Ok(()), // already flattened
        }
    }

    fn emit_image(
        &mut self,
        surface: &mut krilla::surface::Surface<'_>,
        img: &ImageObject,
        offset: Boundary,
    ) -> Result<()> {
        let Ok(bound) = Boundary::parse(&img.boundary) else { return Ok(()) };
        let Some(media) = self.medias.get(&img.resource_id).cloned() else {
            return Ok(());
        };
        let bytes = match self.archive.read_bytes(&media.path) {
            Ok(b) => b,
            Err(_) => return Ok(()),
        };
        let fmt = media.meta.format.to_ascii_uppercase();
        let image = match fmt.as_str() {
            "PNG" => krilla::image::Image::from_png(Arc::new(bytes).into(), false).ok(),
            "JPG" | "JPEG" => krilla::image::Image::from_jpeg(Arc::new(bytes).into(), false).ok(),
            _ => None,
        };
        let Some(image) = image else {
            log::debug!("skipping unsupported image format: {fmt}");
            return Ok(());
        };

        // Compose: translate(offset + boundary_xy) · CTM · scale(1/unit -> boundary size)
        // OFD ImageObject CTM maps the image's unit square [0,1]² to the boundary.
        // We therefore push translate(b.x, b.y) and, when CTM is absent, a
        // scale(w, h) so that draw_image(size=(1,1)) fills the boundary.
        let tx = offset.x + bound.x;
        let ty = offset.y + bound.y;
        surface.push_transform(&KTransform::from_translate(tx as f32, ty as f32));

        if let Some(ctm) = img.ctm.as_deref().and_then(|s| Matrix::parse(s).ok()) {
            surface.push_transform(&matrix_to_ktransform(ctm));
            // Per spec, CTM maps the unit square, so size is (1, 1).
            let size = krilla::geom::Size::from_wh(1.0, 1.0).unwrap();
            surface.draw_image(image, size);
            surface.pop();
        } else {
            // No CTM – assume the image fills the boundary.
            let size =
                krilla::geom::Size::from_wh(bound.width as f32, bound.height as f32).unwrap();
            surface.draw_image(image, size);
        }
        surface.pop();
        Ok(())
    }
}

fn emit_path(
    surface: &mut krilla::surface::Surface<'_>,
    p: &PathObject,
    offset: Boundary,
) -> Result<()> {
    let Ok(bound) = Boundary::parse(&p.boundary) else { return Ok(()) };
    let Some(data) = p.abbreviated_data.as_deref() else { return Ok(()) };
    let cmds = parse_abbreviated(data);
    if cmds.is_empty() {
        return Ok(());
    }

    let krilla_path = cmds_to_krilla_path(&cmds);
    let Some(krilla_path) = krilla_path else { return Ok(()) };

    let tx = offset.x + bound.x;
    let ty = offset.y + bound.y;
    surface.push_transform(&KTransform::from_translate(tx as f32, ty as f32));
    if let Some(ctm) = p.ctm.as_deref().and_then(|s| Matrix::parse(s).ok()) {
        surface.push_transform(&matrix_to_ktransform(ctm));
    }

    let fill = p.fill.unwrap_or(false);
    let stroke = p.stroke.unwrap_or(true);

    if fill {
        let color = color_from_node(p.fill_color.as_ref());
        surface.set_fill(Some(Fill {
            paint: rgb_to_paint(color),
            opacity: normalized(color.alpha as f32 / 255.0),
            rule: Default::default(),
        }));
    } else {
        surface.set_fill(None);
    }

    if stroke {
        let color = color_from_node(p.stroke_color.as_ref());
        let mut s = Stroke::default();
        s.paint = rgb_to_paint(color);
        if let Some(w) = p.line_width {
            s.width = w as f32;
        }
        surface.set_stroke(Some(s));
    } else {
        surface.set_stroke(None);
    }

    surface.draw_path(&krilla_path);

    surface.set_fill(None);
    surface.set_stroke(None);

    if p.ctm.is_some() {
        surface.pop();
    }
    surface.pop();
    Ok(())
}

fn emit_text(
    surface: &mut krilla::surface::Surface<'_>,
    t: &TextObject,
    offset: Boundary,
    font: Font,
) {
    let Ok(bound) = Boundary::parse(&t.boundary) else { return };

    let tx = offset.x + bound.x;
    let ty = offset.y + bound.y;
    surface.push_transform(&KTransform::from_translate(tx as f32, ty as f32));
    if let Some(ctm) = t.ctm.as_deref().and_then(|s| Matrix::parse(s).ok()) {
        surface.push_transform(&matrix_to_ktransform(ctm));
    }

    let color = color_from_node(t.fill_color.as_ref());
    surface.set_fill(Some(Fill {
        paint: rgb_to_paint(color),
        opacity: normalized(color.alpha as f32 / 255.0),
        rule: Default::default(),
    }));
    surface.set_stroke(None);

    for tc in &t.text_code {
        let text = tc.value.trim();
        if text.is_empty() {
            continue;
        }
        let x = tc.x.unwrap_or(0.0);
        let y = tc.y.unwrap_or(0.0);
        surface.draw_text(
            Point::from_xy(x as f32, y as f32),
            font.clone(),
            t.size as f32,
            text,
            false,
            TextDirection::Auto,
        );
    }

    surface.set_fill(None);
    if t.ctm.is_some() {
        surface.pop();
    }
    surface.pop();
}

// ---- helpers ----

fn cmds_to_krilla_path(cmds: &[PathCmd]) -> Option<krilla::geom::Path> {
    let mut pb = PathBuilder::new();
    let mut started = false;
    let mut cur = (0.0_f32, 0.0_f32);
    let mut start = (0.0_f32, 0.0_f32);
    for c in cmds {
        match *c {
            PathCmd::MoveTo { x, y } => {
                pb.move_to(x as f32, y as f32);
                cur = (x as f32, y as f32);
                start = cur;
                started = true;
            }
            PathCmd::LineTo { x, y } => {
                if !started {
                    pb.move_to(0.0, 0.0);
                    started = true;
                }
                pb.line_to(x as f32, y as f32);
                cur = (x as f32, y as f32);
            }
            PathCmd::QuadTo { x1, y1, x, y } => {
                if !started {
                    pb.move_to(cur.0, cur.1);
                    started = true;
                }
                pb.quad_to(x1 as f32, y1 as f32, x as f32, y as f32);
                cur = (x as f32, y as f32);
            }
            PathCmd::CurveTo { x1, y1, x2, y2, x, y } => {
                if !started {
                    pb.move_to(cur.0, cur.1);
                    started = true;
                }
                pb.cubic_to(x1 as f32, y1 as f32, x2 as f32, y2 as f32, x as f32, y as f32);
                cur = (x as f32, y as f32);
            }
            PathCmd::ArcTo { x, y, .. } => {
                // Rough approximation: straight line. Arcs in OFD are rarely
                // used for invoices/public docs.
                pb.line_to(x as f32, y as f32);
                cur = (x as f32, y as f32);
            }
            PathCmd::Close => {
                pb.close();
                cur = start;
            }
        }
    }
    pb.finish()
}

fn matrix_to_ktransform(m: Matrix) -> KTransform {
    KTransform::from_row(m.a as f32, m.b as f32, m.c as f32, m.d as f32, m.e as f32, m.f as f32)
}

fn color_from_node(n: Option<&ColorNode>) -> RgbColor {
    match n {
        Some(c) => RgbColor::parse(&c.value, c.alpha).unwrap_or_default(),
        None => RgbColor::default(),
    }
}

fn rgb_to_paint(c: RgbColor) -> krilla::paint::Paint {
    rgb::Color::new(c.r, c.g, c.b).into()
}

fn normalized(v: f32) -> NormalizedF32 {
    NormalizedF32::new(v.clamp(0.0, 1.0)).unwrap_or(NormalizedF32::ONE)
}
