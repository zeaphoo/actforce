//! CPU raster renderer using `vello_cpu` → PNG.
//!
//! Renders vector paths and embedded bitmap images. Text rendering is not
//! yet implemented (OFD uses explicit per-glyph `DeltaX` placement which
//! benefits from a separate shaping pass); if you need text in the PNG,
//! consider using [`crate::pdf`] to produce a PDF and rasterise that, or
//! contribute a text backend.

#![cfg(feature = "render-cpu")]

use std::path::Path;
use std::sync::Arc;

use std::io::Cursor;

use vello_cpu::color::{AlphaColor, Srgb};
use vello_cpu::kurbo::{Affine, BezPath, Rect};
use vello_cpu::peniko::{Extend, Fill as FillRule, ImageSampler, ImageQuality};
use vello_cpu::{Image, ImageSource, Level, Pixmap, RenderContext, RenderMode, RenderSettings};

use crate::document::OfdDocument;
use crate::error::{OfdError, Result};
use crate::model::{ColorNode, ImageObject, PageObject, PathObject};
use crate::path::{parse_abbreviated, PathCmd};
use crate::types::{Boundary, Matrix, RgbColor};

/// Options for raster PNG export.
#[derive(Debug, Clone, Copy)]
pub struct PngOptions {
    /// Target DPI. 96 gives ~A4 ≈ 794x1123 px.
    pub dpi: f32,
    /// Background color (RGBA 0..255). Default: opaque white.
    pub background: [u8; 4],
    /// Maximum rendered pixel dimension per page (safety clamp).
    pub max_pixels: u32,
}

impl Default for PngOptions {
    fn default() -> Self {
        Self { dpi: 150.0, background: [255, 255, 255, 255], max_pixels: 8192 }
    }
}

impl PngOptions {
    pub fn new() -> Self { Self::default() }
    pub fn with_dpi(mut self, dpi: f32) -> Self { self.dpi = dpi; self }
}

impl OfdDocument {
    /// Render all pages to PNG byte buffers.
    pub fn to_png_pages(&mut self, options: &PngOptions) -> Result<Vec<Vec<u8>>> {
        let page_count = self.pages.len();
        let mut out = Vec::with_capacity(page_count);
        for i in 0..page_count {
            out.push(self.to_png_page(i, options)?);
        }
        Ok(out)
    }

    /// Render a specific page to a PNG byte buffer.
    pub fn to_png_page(&mut self, page_index: usize, options: &PngOptions) -> Result<Vec<u8>> {
        let page_area = self
            .pages
            .get(page_index)
            .ok_or_else(|| OfdError::Other(format!("page {page_index} out of range")))?
            .area;

        let scale = options.dpi / 25.4; // px per mm
        let w = ((page_area.width as f32) * scale).round() as u32;
        let h = ((page_area.height as f32) * scale).round() as u32;
        let w = w.clamp(1, options.max_pixels) as u16;
        let h = h.clamp(1, options.max_pixels) as u16;

        let settings = RenderSettings {
            level: Level::new(),
            num_threads: 0,
            render_mode: RenderMode::OptimizeSpeed,
        };
        let mut ctx = RenderContext::new_with(w, h, settings);

        // Background.
        let bg = options.background;
        ctx.set_paint(color_u8(bg[0], bg[1], bg[2], bg[3]));
        ctx.fill_rect(&Rect::new(0.0, 0.0, w as f64, h as f64));

        // mm -> px scale for everything that follows.
        ctx.set_transform(Affine::scale(scale as f64));

        // Gather page objects.
        let objs: Vec<PageObject> = self.pages[page_index].page.flatten_objects();
        for obj in &objs {
            draw_object(&mut ctx, obj, Boundary::default(), self, page_index)?;
        }

        // Stamps.
        let pid = self.pages[page_index].id;
        if let Some(annots) = self.annotations_by_page.get(&pid).cloned() {
            for annot in annots {
                for obj in &annot.objects {
                    draw_object(&mut ctx, obj, annot.boundary, self, page_index)?;
                }
            }
        }

        ctx.flush();

        let mut pixmap = Pixmap::new(w, h);
        ctx.render_to_pixmap(&mut pixmap);
        let png = pixmap
            .into_png()
            .map_err(|e| OfdError::Other(format!("png encode: {e}")))?;
        Ok(png)
    }

    /// Render a specific page to a PNG file.
    pub fn to_png_file<P: AsRef<Path>>(
        &mut self,
        page_index: usize,
        out: P,
        options: &PngOptions,
    ) -> Result<()> {
        let bytes = self.to_png_page(page_index, options)?;
        std::fs::write(out, bytes)?;
        Ok(())
    }
}

fn draw_object(
    ctx: &mut RenderContext,
    obj: &PageObject,
    offset: Boundary,
    doc: &mut OfdDocument,
    _page_index: usize,
) -> Result<()> {
    match obj {
        PageObject::PathObject(p) => draw_path(ctx, p, offset),
        PageObject::ImageObject(img) => draw_image(ctx, img, offset, doc),
        PageObject::TextObject(_) => Ok(()), // text not implemented (see module docs)
        PageObject::PageBlock(_) => Ok(()),
    }
}

fn draw_path(ctx: &mut RenderContext, p: &PathObject, offset: Boundary) -> Result<()> {
    let Ok(bound) = Boundary::parse(&p.boundary) else { return Ok(()) };
    let Some(data) = p.abbreviated_data.as_deref() else { return Ok(()) };
    let cmds = parse_abbreviated(data);
    if cmds.is_empty() {
        return Ok(());
    }

    let base = *ctx.transform();
    let translate = Affine::translate((offset.x + bound.x, offset.y + bound.y));
    let mut local = base * translate;
    if let Some(ctm) = p.ctm.as_deref().and_then(|s| Matrix::parse(s).ok()) {
        local *= matrix_to_affine(ctm);
    }
    ctx.set_transform(local);

    let bezpath = cmds_to_bezpath(&cmds);

    let stroke_width = p.line_width.unwrap_or(0.3);

    if p.fill.unwrap_or(false) {
        let c = color_from_node(p.fill_color.as_ref());
        ctx.set_paint(rgb_to_alpha(c));
        ctx.set_fill_rule(FillRule::NonZero);
        ctx.fill_path(&bezpath);
    }
    if p.stroke.unwrap_or(true) {
        let c = color_from_node(p.stroke_color.as_ref());
        ctx.set_paint(rgb_to_alpha(c));
        let s = vello_cpu::kurbo::Stroke::new(stroke_width)
            .with_caps(vello_cpu::kurbo::Cap::Butt);
        ctx.set_stroke(s);
        ctx.stroke_path(&bezpath);
    }

    ctx.set_transform(base);
    Ok(())
}

fn draw_image(
    ctx: &mut RenderContext,
    img: &ImageObject,
    offset: Boundary,
    doc: &mut OfdDocument,
) -> Result<()> {
    let Ok(bound) = Boundary::parse(&img.boundary) else { return Ok(()) };
    let Some(media) = doc.medias.get(&img.resource_id).cloned() else { return Ok(()) };
    let bytes = match doc.archive.read_bytes(&media.path) {
        Ok(b) => b,
        Err(_) => return Ok(()),
    };

    // Decode PNG / JPEG using vello_common re-exports (png feature is on).
    // We fall back to a rudimentary decoder via the `png` / `zune_jpeg` crates
    // pulled in transitively. To keep scope tight, rely on peniko's image
    // decoding via vello_common::pixmap::Pixmap::from_png if available; else
    // skip the image.
    let pix = match media.meta.format.to_ascii_uppercase().as_str() {
        "PNG" => Pixmap::from_png(Cursor::new(&bytes[..])).ok(),
        _ => None,
    };
    let Some(pix) = pix else {
        log::debug!("render-cpu: skipping image in unsupported format: {}", media.meta.format);
        return Ok(());
    };
    let (pw, ph) = (pix.width() as f64, pix.height() as f64);

    let base = *ctx.transform();
    let translate = Affine::translate((offset.x + bound.x, offset.y + bound.y));
    let mut local = base * translate;
    if let Some(ctm) = img.ctm.as_deref().and_then(|s| Matrix::parse(s).ok()) {
        // Per spec, CTM maps unit square -> page; image data is the unit square.
        local *= matrix_to_affine(ctm);
        local *= Affine::scale_non_uniform(1.0 / pw, 1.0 / ph);
    } else {
        local *= Affine::scale_non_uniform(bound.width / pw, bound.height / ph);
    }
    ctx.set_transform(local);

    let image = Image {
        image: ImageSource::Pixmap(Arc::new(pix)),
        sampler: ImageSampler {
            x_extend: Extend::Pad,
            y_extend: Extend::Pad,
            quality: ImageQuality::Medium,
            alpha: 1.0,
        },
    };
    ctx.set_paint(image);
    ctx.fill_rect(&Rect::new(0.0, 0.0, pw, ph));

    ctx.set_transform(base);
    Ok(())
}

fn cmds_to_bezpath(cmds: &[PathCmd]) -> BezPath {
    let mut p = BezPath::new();
    let mut started = false;
    let mut cur = (0.0_f64, 0.0_f64);
    let mut start = (0.0_f64, 0.0_f64);
    for c in cmds {
        match *c {
            PathCmd::MoveTo { x, y } => {
                p.move_to((x, y));
                cur = (x, y);
                start = cur;
                started = true;
            }
            PathCmd::LineTo { x, y } => {
                if !started {
                    p.move_to((0.0, 0.0));
                    started = true;
                }
                p.line_to((x, y));
                cur = (x, y);
            }
            PathCmd::QuadTo { x1, y1, x, y } => {
                if !started {
                    p.move_to(cur);
                    started = true;
                }
                p.quad_to((x1, y1), (x, y));
                cur = (x, y);
            }
            PathCmd::CurveTo { x1, y1, x2, y2, x, y } => {
                if !started {
                    p.move_to(cur);
                    started = true;
                }
                p.curve_to((x1, y1), (x2, y2), (x, y));
                cur = (x, y);
            }
            PathCmd::ArcTo { x, y, .. } => {
                p.line_to((x, y));
                cur = (x, y);
            }
            PathCmd::Close => {
                p.close_path();
                cur = start;
            }
        }
    }
    p
}

fn matrix_to_affine(m: Matrix) -> Affine {
    Affine::new([m.a, m.b, m.c, m.d, m.e, m.f])
}

fn color_from_node(n: Option<&ColorNode>) -> RgbColor {
    match n {
        Some(c) => RgbColor::parse(&c.value, c.alpha).unwrap_or_default(),
        None => RgbColor::default(),
    }
}

fn color_u8(r: u8, g: u8, b: u8, a: u8) -> AlphaColor<Srgb> {
    AlphaColor::<Srgb>::from_rgba8(r, g, b, a)
}

fn rgb_to_alpha(c: RgbColor) -> AlphaColor<Srgb> {
    color_u8(c.r, c.g, c.b, c.alpha)
}
