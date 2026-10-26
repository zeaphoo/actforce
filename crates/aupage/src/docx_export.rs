use crate::types::*;
use anyhow::{Result, Context};
use docx_rs::*;
use std::fs::File;
use std::path::Path;

/// Export an AuPage document to DOCX format
pub fn export(doc: &AuPageDocument, images: &[EmbeddedImage], output_path: &Path) -> Result<()> {
    let mut docx = Docx::default();

    // Document title
    let title_para = Paragraph::new().add_run(
        Run::new()
            .add_text(&doc.title)
            .bold()
            .size(36),
    );
    docx = docx.add_paragraph(title_para);
    docx = docx.add_paragraph(Paragraph::new());

    // Render elements
    for el in &doc.elements {
        render_element_to_docx(&mut docx, el, images);
    }

    // Write to file
    let mut file = File::create(output_path)
        .with_context(|| format!("Failed to create DOCX file: {}", output_path.display()))?;
    docx.build().pack(&mut file)
        .with_context(|| format!("Failed to write DOCX to {}", output_path.display()))?;

    Ok(())
}

fn render_element_to_docx(docx: &mut Docx, el: &Element, images: &[EmbeddedImage]) {
    match el {
        Element::Section(e) => {
            if let Some(title) = &e.common.title {
                let para = Paragraph::new()
                    .add_run(Run::new()
                        .add_text(title)
                        .bold()
                        .size(28)
                        .color("1F2937"));
                *docx = std::mem::take(docx).add_paragraph(para);
            }
            for child in &e.elements {
                render_element_to_docx(docx, child, images);
            }
            *docx = std::mem::take(docx).add_paragraph(Paragraph::new());
        }
        Element::Row(e) => {
            for child in &e.elements {
                render_element_to_docx(docx, child, images);
            }
        }
        Element::Column(e) => {
            for child in &e.elements {
                render_element_to_docx(docx, child, images);
            }
        }
        Element::Heading(e) => {
            let size = match e.level {
                1 => 32,
                2 => 28,
                3 => 24,
                _ => 22,
            };
            let run = Run::new().add_text(&e.content).size(size);
            let run = if e.level <= 3 { run.bold() } else { run };
            let para = Paragraph::new().add_run(run);
            *docx = std::mem::take(docx).add_paragraph(para);
        }
        Element::Paragraph(e) => {
            let para = Paragraph::new().add_run(Run::new().add_text(&e.content));
            *docx = std::mem::take(docx).add_paragraph(para);
        }
        Element::Markdown(e) => {
            for line in e.content.lines() {
                let para = Paragraph::new().add_run(Run::new().add_text(line));
                *docx = std::mem::take(docx).add_paragraph(para);
            }
        }
        Element::SummaryCards(e) => {
            for card in &e.cards {
                let para = Paragraph::new()
                    .add_run(Run::new()
                        .add_text(&format!("{}: {}", card.label, card.value))
                        .bold()
                        .size(22));
                *docx = std::mem::take(docx).add_paragraph(para);

                if let Some(trend) = &card.trend {
                    let color = if trend.starts_with('+') { "10B981" } else { "EF4444" };
                    let para = Paragraph::new()
                        .add_run(Run::new().add_text(trend).size(18).color(color));
                    *docx = std::mem::take(docx).add_paragraph(para);
                }
            }
        }
        Element::Chart(e) => {
            if let Some(title) = &e.title {
                let para = Paragraph::new()
                    .add_run(Run::new().add_text(title).bold().size(22));
                *docx = std::mem::take(docx).add_paragraph(para);
            }

            let mut png_data: Option<Vec<u8>> = None;

            // Tier 1: CLI --image pre-rendered chart
            let chart_key = format!("chart-{}", e.common.id.as_deref().unwrap_or(""));
            if let Some(img) = images.iter().find(|i| i.key == chart_key || i.key == e.common.id.as_deref().unwrap_or("")) {
                png_data = Some(img.data.clone());
            }

            // Tier 2: charton rendering
            if png_data.is_none() {
                if let Ok(Some(data)) = crate::chart_render::render_chart(e) {
                    png_data = Some(data);
                }
            }

            if let Some(data) = png_data {
                let pic = Pic::new(&data);
                let pic = scale_pic_to_page(pic);
                let para = Paragraph::new().add_run(Run::new().add_image(pic));
                *docx = std::mem::take(docx).add_paragraph(para);
            } else {
                // Fallback: render chart data as text
                let mut text = String::from("Chart Data:");
                for series in &e.data.series {
                    text.push_str(&format!("\n  {}: {:?}", series.name, series.values));
                }
                let para = Paragraph::new().add_run(Run::new().add_text(&text));
                *docx = std::mem::take(docx).add_paragraph(para);
            }
        }
        Element::Table(e) => {
            if let Some(title) = &e.title {
                let para = Paragraph::new()
                    .add_run(Run::new().add_text(title).bold().size(22));
                *docx = std::mem::take(docx).add_paragraph(para);
            }

            // Build table rows
            let mut table_rows: Vec<TableRow> = Vec::new();

            // Header row
            let header_cells: Vec<TableCell> = e.columns.iter().map(|c| {
                let para = Paragraph::new().add_run(Run::new().add_text(&c.label).bold());
                TableCell::new().add_paragraph(para)
            }).collect();
            table_rows.push(TableRow::new(header_cells));

            // Data rows
            for row in &e.rows {
                let cells: Vec<TableCell> = e.columns.iter().map(|c| {
                    let val = row.get(&c.key)
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let para = Paragraph::new().add_run(Run::new().add_text(&val));
                    TableCell::new().add_paragraph(para)
                }).collect();
                table_rows.push(TableRow::new(cells));
            }

            let table = Table::new(table_rows);
            *docx = std::mem::take(docx).add_table(table);
        }
        Element::List(e) => {
            for (i, item) in e.items.iter().enumerate() {
                let prefix = if matches!(e.list_type, ListType::Ordered) {
                    format!("{}. ", i + 1)
                } else {
                    "\u{2022} ".to_string()
                };
                let para = Paragraph::new()
                    .add_run(Run::new().add_text(&format!("{}{}", prefix, item)));
                *docx = std::mem::take(docx).add_paragraph(para);
            }
        }
        Element::Alert(e) => {
            let alert_type = match e.alert_type {
                AlertType::Error => "ERROR",
                AlertType::Warning => "WARNING",
                AlertType::Info => "INFO",
                AlertType::Success => "SUCCESS",
            };

            let mut text = format!("[{}] ", alert_type);
            if let Some(title) = &e.title {
                text.push_str(&format!("{}\n", title));
            }
            text.push_str(&e.content);
            if let Some(conf) = e.confidence {
                text.push_str(&format!("\n置信度: {:.0}%", conf * 100.0));
            }

            let color = match e.alert_type {
                AlertType::Error => "EF4444",
                AlertType::Warning => "F59E0B",
                AlertType::Info => "3B82F6",
                AlertType::Success => "10B981",
            };
            let para = Paragraph::new()
                .add_run(Run::new().add_text(&text).color(color));
            *docx = std::mem::take(docx).add_paragraph(para);
        }
        Element::Divider(_) => {
            let para = Paragraph::new()
                .add_run(Run::new().add_text("\u{2500}".repeat(50)).color("E5E7EB"));
            *docx = std::mem::take(docx).add_paragraph(para);
        }
        Element::Image(e) => {
            if let Ok(img_data) = std::fs::read(&e.src) {
                let pic = Pic::new(&img_data);
                let para = Paragraph::new().add_run(Run::new().add_image(pic));
                *docx = std::mem::take(docx).add_paragraph(para);
            } else {
                let para = Paragraph::new()
                    .add_run(Run::new().add_text(&format!("[Image: {}]", e.src)));
                *docx = std::mem::take(docx).add_paragraph(para);
            }

            if let Some(caption) = &e.caption {
                let para = Paragraph::new()
                    .add_run(Run::new().add_text(caption).size(16).color("6B7280").italic());
                *docx = std::mem::take(docx).add_paragraph(para);
            }
        }
    }
}

/// Scale a Pic so its width does not exceed 80% of A4 page width.
/// A4 = 21cm, 1cm = 360000 EMU. 80% of A4 = 0.8 * 21 * 360000 = 6,048,000 EMU.
fn scale_pic_to_page(pic: Pic) -> Pic {
    const MAX_WIDTH_EMU: u32 = 6_048_000; // 80% of A4 page width
    let (w, h) = pic.size;
    if w > MAX_WIDTH_EMU {
        let scale = MAX_WIDTH_EMU as f64 / w as f64;
        let new_h = (h as f64 * scale) as u32;
        pic.size(MAX_WIDTH_EMU, new_h)
    } else {
        pic
    }
}
