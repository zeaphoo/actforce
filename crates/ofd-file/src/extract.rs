//! Text extraction & Markdown rendering.

use crate::document::{LoadedPage, OfdDocument};
use crate::error::Result;
use crate::model::{PageObject, TextObject};
use crate::types::{Boundary, Matrix};

/// A single positioned glyph run extracted from a page.
#[derive(Debug, Clone)]
pub struct TextRun {
    /// Page-absolute top-left X (mm).
    pub x: f64,
    /// Page-absolute top-left Y (mm) of the text baseline.
    pub y: f64,
    pub font_size: f64,
    pub font_id: u64,
    pub text: String,
}

impl OfdDocument {
    /// Extract a flat list of positioned text runs for a given page (by index).
    pub fn extract_text_runs(&self, page_index: usize) -> Vec<TextRun> {
        let Some(page) = self.pages.get(page_index) else { return Vec::new() };
        let mut runs = Vec::new();
        for obj in page.page.flatten_objects() {
            if let PageObject::TextObject(t) = obj {
                push_runs_for_text(&t, page, &mut runs);
            }
        }
        // Also include annotations (e.g. stamps with TextObjects — rare but spec-valid).
        for annot in self.annotations_for(page.id) {
            for obj in &annot.objects {
                if let PageObject::TextObject(t) = obj {
                    push_runs_for_text_with_offset(t, page, annot.boundary, &mut runs);
                }
            }
        }
        runs
    }

    /// Extract all pages' text as plain UTF-8, page-break delimited with `\f`.
    pub fn extract_text(&self) -> String {
        let mut out = String::new();
        for i in 0..self.pages.len() {
            if i > 0 {
                out.push('\u{000C}');
            }
            out.push_str(&runs_to_text(&self.extract_text_runs(i)));
        }
        out
    }

    /// Convert the OFD into a Markdown document.
    ///
    /// Strategy: for each page, group text runs into lines using y-proximity
    /// (threshold derived from font size), within a line sort by x, then join
    /// with a single space if gaps are big. Pages separated by horizontal rule.
    pub fn to_markdown(&self) -> Result<String> {
        let mut md = String::new();
        let title = self.doc_info.title.trim();
        if !title.is_empty() {
            md.push_str(&format!("# {title}\n\n"));
        }

        for (idx, _page) in self.pages.iter().enumerate() {
            if idx > 0 {
                md.push_str("\n\n---\n\n");
            }
            md.push_str(&format!("<!-- page {} -->\n\n", idx + 1));
            let runs = self.extract_text_runs(idx);
            md.push_str(&runs_to_markdown_body(&runs));
        }
        Ok(md)
    }
}

// ---------------------------------------------------------------------------
// Layout helpers
// ---------------------------------------------------------------------------

fn push_runs_for_text(t: &TextObject, _page: &LoadedPage, out: &mut Vec<TextRun>) {
    push_runs_for_text_with_offset(t, _page, Boundary::default(), out);
}

fn push_runs_for_text_with_offset(
    t: &TextObject,
    _page: &LoadedPage,
    offset: Boundary,
    out: &mut Vec<TextRun>,
) {
    let Ok(bound) = Boundary::parse(&t.boundary) else { return };
    let ctm = t
        .ctm
        .as_deref()
        .and_then(|s| Matrix::parse(s).ok())
        .unwrap_or_default();

    for tc in &t.text_code {
        if tc.value.is_empty() {
            continue;
        }
        let deltas = parse_delta(tc.delta_x.as_deref());
        let base_x = tc.x.unwrap_or(0.0);
        let base_y = tc.y.unwrap_or(0.0);

        // Compose absolute coordinates. OFD text objects place text by CTM
        // relative to the object's boundary origin. We approximate by using
        // Boundary.x/y + CTM translation + TextCode.X/Y.
        let abs_x = offset.x + bound.x + ctm.e + base_x;
        let abs_y = offset.y + bound.y + ctm.f + base_y;

        out.push(TextRun {
            x: abs_x,
            y: abs_y,
            font_size: t.size,
            font_id: t.font,
            text: normalize_text(&tc.value, &deltas),
        });
    }
}

fn parse_delta(s: Option<&str>) -> Vec<f64> {
    let Some(s) = s else { return Vec::new() };
    // OFD supports "g" notation: e.g. "3 g 2 4" == "3 3 4"
    let tokens: Vec<&str> = s.split_whitespace().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let tok = tokens[i];
        if tok.eq_ignore_ascii_case("g") {
            // g N V => repeat V N times
            if i + 2 < tokens.len() {
                if let (Ok(n), Ok(v)) =
                    (tokens[i + 1].parse::<usize>(), tokens[i + 2].parse::<f64>())
                {
                    for _ in 0..n {
                        out.push(v);
                    }
                    i += 3;
                    continue;
                }
            }
            i += 1;
        } else if let Ok(v) = tok.parse::<f64>() {
            out.push(v);
            i += 1;
        } else {
            i += 1;
        }
    }
    out
}

fn normalize_text(s: &str, _deltas: &[f64]) -> String {
    // DeltaX/Y describe per-glyph advance; text ordering itself is the raw
    // string. We simply trim CDATA wrappers / collapse internal whitespace.
    s.trim().to_string()
}

// Threshold for "same line" — fraction of font size.
const LINE_Y_TOL: f64 = 0.6;
// Threshold for adding a space between two runs on the same line (mm).
const GAP_SPACE_MM: f64 = 0.8;

fn runs_to_lines(runs: &[TextRun]) -> Vec<Vec<TextRun>> {
    if runs.is_empty() {
        return Vec::new();
    }
    let mut sorted: Vec<TextRun> = runs.iter().filter(|r| !r.text.is_empty()).cloned().collect();
    sorted.sort_by(|a, b| {
        a.y.partial_cmp(&b.y)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal))
    });

    let mut lines: Vec<Vec<TextRun>> = Vec::new();
    for run in sorted {
        let tol = (run.font_size.max(1.0)) * LINE_Y_TOL;
        let mut placed = false;
        for line in lines.iter_mut() {
            let ly = line[0].y;
            if (run.y - ly).abs() <= tol {
                line.push(run.clone());
                placed = true;
                break;
            }
        }
        if !placed {
            lines.push(vec![run]);
        }
    }

    for line in lines.iter_mut() {
        line.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal));
    }
    lines.sort_by(|a, b| a[0].y.partial_cmp(&b[0].y).unwrap_or(std::cmp::Ordering::Equal));
    lines
}

fn line_to_string(line: &[TextRun]) -> String {
    let mut out = String::new();
    let mut prev_end: Option<f64> = None;
    for run in line {
        if let Some(pe) = prev_end {
            if run.x - pe > GAP_SPACE_MM {
                out.push(' ');
            }
        }
        out.push_str(&run.text);
        // Rough: end x = x + char_count * font_size * 0.5 (approx width per glyph).
        let est_w = run.text.chars().count() as f64 * run.font_size * 0.5;
        prev_end = Some(run.x + est_w);
    }
    out
}

fn runs_to_text(runs: &[TextRun]) -> String {
    runs_to_lines(runs)
        .iter()
        .map(|l| line_to_string(l))
        .collect::<Vec<_>>()
        .join("\n")
}

fn runs_to_markdown_body(runs: &[TextRun]) -> String {
    let lines = runs_to_lines(runs);
    let mut out = String::new();
    for line in lines {
        let s = line_to_string(&line);
        let s = escape_md(&s);
        if s.trim().is_empty() {
            continue;
        }
        out.push_str(&s);
        out.push_str("  \n"); // markdown hard break
    }
    out
}

fn escape_md(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\\' | '`' | '*' | '_' | '{' | '}' | '[' | ']' | '(' | ')' | '#' | '+' | '!' | '|'
            | '>' => {
                out.push('\\');
                out.push(ch);
            }
            _ => out.push(ch),
        }
    }
    out
}
