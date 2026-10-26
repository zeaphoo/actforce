use crate::types::*;
use std::fmt::Write;

/// Helper macro: writeln! to a String never fails, so discard the Result.
macro_rules! wln {
    ($dst:expr, $($arg:tt)*) => {{
        let _ = $dst.write_fmt(format_args!($($arg)*));
    }};
}

/// Export an AuPage document to HTML for screen viewing (with .page wrapper and shadows)
pub fn export(doc: &AuPageDocument, images: &[EmbeddedImage], show_mode: &ShowMode) -> String {
    render_html(doc, images, false, true, show_mode)
}

/// Export an AuPage document to HTML for PDF conversion (no .page wrapper, no ECharts — for ironpress/charton)
pub fn export_for_pdf(doc: &AuPageDocument, images: &[EmbeddedImage], show_mode: &ShowMode) -> String {
    render_html(doc, images, true, false, show_mode)
}

/// Export for Chrome headless PDF: PDF-friendly CSS (no .page wrapper) + ECharts charts
pub fn export_for_pdf_chrome(doc: &AuPageDocument, show_mode: &ShowMode) -> String {
    render_html(doc, &[], true, true, show_mode)
}

fn render_html(doc: &AuPageDocument, images: &[EmbeddedImage], for_pdf: bool, use_echarts: bool, show_mode: &ShowMode) -> String {
    let mut html = String::new();
    let mut charts: Vec<(String, serde_json::Value)> = Vec::new();
    let mut charts_ref: Option<&mut Vec<(String, serde_json::Value)>> = if use_echarts { Some(&mut charts) } else { None };
    html.push_str("<!DOCTYPE html>\n<html lang=\"zh-CN\">\n<head>\n");
    html.push_str("<meta charset=\"utf-8\">\n");
    html.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    html.push_str(&format!("<title>{}</title>\n", escape_html(&doc.title)));

    let margin = doc.meta.as_ref().and_then(|m| m.margin.as_ref());
    let page_width = doc.meta.as_ref().and_then(|m| m.page_width);
    let font_family = doc.meta.as_ref().and_then(|m| m.font_family.as_deref());
    let font_size = doc.meta.as_ref().and_then(|m| m.font_size);

    wln!(html, "<style>
:root {{
    --page-width: {}px;
    --margin-top: {}px;
    --margin-right: {}px;
    --margin-bottom: {}px;
    --margin-left: {}px;
    --font-family: {};
    --font-size: {}pt;
}}
* {{ box-sizing: border-box; margin: 0; padding: 0; }}
body {{
    font-family: \"Noto Sans CJK SC\", \"WenQuanYi Micro Hei\", var(--font-family), 'Microsoft YaHei', SimSun, sans-serif;
    font-size: var(--font-size);
    line-height: 1.6;
    color: #333;
    background: {};
    padding: {};
}}
.page {{
    max-width: var(--page-width);
    margin: 0 auto;
    background: white;
    padding: calc(var(--margin-top) / 3) calc(var(--margin-right) / 3) calc(var(--margin-bottom) / 3) calc(var(--margin-left) / 3);
    box-shadow: 0 1px 3px rgba(0,0,0,0.1);
}}
.section {{ margin-bottom: 24px; page-break-inside: avoid; }}
.section-title {{
    font-size: 1.2em;
    font-weight: 600;
    color: #1a1a1a;
    border-bottom: 2px solid #e5e7eb;
    padding-bottom: 8px;
    margin-bottom: 16px;
}}
h1 {{ font-size: 1.6em; margin: 16px 0 12px; color: #111; page-break-after: avoid; }}
h2 {{ font-size: 1.3em; margin: 14px 0 10px; color: #222; page-break-after: avoid; }}
h3 {{ font-size: 1.1em; margin: 12px 0 8px; color: #333; page-break-after: avoid; }}
p {{ margin: 8px 0; }}
.row {{
    display: flex;
    flex-direction: row;
    gap: 16px;
    margin: 12px 0;
    page-break-inside: avoid;
}}
.column {{
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
}}
.cards {{
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(140px, 1fr));
    gap: 8px;
    margin: 12px 0;
    page-break-inside: avoid;
}}
.card {{
    background: #f9fafb;
    border: 1px solid #e5e7eb;
    border-radius: 8px;
    padding: 6px 10px;
}}
.card-label {{ font-size: 0.75em; color: #6b7280; }}
.card-value {{ font-size: 1.15em; font-weight: 600; color: #111; margin-top: 4px; }}
.card-trend {{ font-size: 0.85em; margin-top: 2px; }}
.card-trend.up {{ color: #10b981; }}
.card-trend.down {{ color: #ef4444; }}
.chart-container {{
    margin: 12px 0;
    text-align: center;
    page-break-inside: avoid;
}}
.chart-container img {{
    max-width: 100%;
    height: auto;
    border: 1px solid #e5e7eb;
    border-radius: 4px;
}}
	.chart-img {{
	    max-width: 100%;
	    height: auto;
	    border: 1px solid #e5e7eb;
	    border-radius: 4px;
	    margin: 8px 0;
	}}
	.echarts-chart {{
	    width: 100%;
	    min-height: 400px;
	}}
.chart-title {{
    font-size: 1em;
    font-weight: 500;
    color: #374151;
    margin-bottom: 8px;
}}
table {{
    width: 100%;
    border-collapse: collapse;
    margin: 12px 0;
    page-break-inside: auto;
}}
tr {{ page-break-inside: avoid; page-break-after: auto; }}
th, td {{
    border: 1px solid #e5e7eb;
    padding: 6px 10px;
    text-align: left;
    font-size: 0.875em;
}}
th {{ background: #f9fafb; font-weight: 600; }}
.alert {{
    border-radius: 8px;
    padding: 12px 16px;
    margin: 8px 0;
    border-left: 4px solid;
    page-break-inside: avoid;
}}
.alert-error {{ background: #fef2f2; border-color: #ef4444; }}
.alert-warning {{ background: #fffbeb; border-color: #f59e0b; }}
.alert-info {{ background: #eff6ff; border-color: #3b82f6; }}
.alert-success {{ background: #f0fdf4; border-color: #10b981; }}
.alert-title {{ font-weight: 600; margin-bottom: 4px; }}
.alert-content {{ font-size: 0.9em; color: #4b5563; }}
.alert-confidence {{ font-size: 0.8em; color: #9ca3af; margin-top: 4px; }}
ul, ol {{ margin: 8px 0; padding-left: 24px; }}
li {{ margin: 4px 0; }}
hr {{ border: none; border-top: 1px solid #e5e7eb; margin: 16px 0; }}
img {{ max-width: 100%; height: auto; }}
img.captioned {{ margin-bottom: 4px; }}
.image-caption {{ font-size: 0.85em; color: #6b7280; text-align: center; font-style: italic; }}
.divider {{ border: none; border-top: 1px solid #e5e7eb; margin: 16px 0; }}
@media print {{
    body {{ background: white; padding: 0; }}
    .page {{ box-shadow: none; padding: calc(var(--margin-top) / 3) calc(var(--margin-right) / 3) calc(var(--margin-bottom) / 3) calc(var(--margin-left) / 3); }}
}}
</style>\n",
        page_width.unwrap_or(794),
        margin.map(|m| m.top).unwrap_or(72),
        margin.map(|m| m.right).unwrap_or(72),
        margin.map(|m| m.bottom).unwrap_or(72),
        margin.map(|m| m.left).unwrap_or(72),
        font_family.unwrap_or("SimSun"),
        font_size.unwrap_or(12),
        if for_pdf { "white" } else { "#f5f5f5" },
        if for_pdf { "0" } else { "20px" },
    );

    html.push_str("</style>\n</head>\n<body>\n");

    if !for_pdf {
        html.push_str("<div class=\"page\">\n");
    }

    // Document title
    wln!(html, "<h1>{}</h1>", escape_html(&doc.title));

    // Render elements
    for el in &doc.elements {
        render_element(&mut html, el, images, charts_ref.as_deref_mut(), show_mode);
    }

    if use_echarts && !charts.is_empty() {
        html.push_str("<script>");
        html.push_str(include_str!("echarts.min.js"));
        html.push_str("</script>\n");
        html.push_str("<script>");
        html.push_str("(function(){var cs=");
        html.push_str(&serde_json::to_string(&charts.iter().map(|(id, opt)| {
            serde_json::json!({"id": id, "option": opt})
        }).collect::<Vec<_>>()).unwrap_or_default());
        html.push_str(";cs.forEach(function(c){var d=document.getElementById(c.id);if(d){var ch=echarts.init(d);ch.setOption(c.option);window.addEventListener('resize',function(){ch.resize();});}});})();");
        html.push_str("</script>\n");
    }

    if !for_pdf {
        html.push_str("</div>\n");
    }
    html.push_str("</body>\n</html>");
    html
}

fn render_element(html: &mut String, el: &Element, images: &[EmbeddedImage], charts: Option<&mut Vec<(String, serde_json::Value)>>, show_mode: &ShowMode) {
    match el {
        Element::Section(e) => render_section(html, e, images, charts, show_mode),
        Element::Row(e) => render_row(html, e, images, charts, show_mode),
        Element::Column(e) => render_column(html, e, images, charts, show_mode),
        Element::Heading(e) => render_heading(html, e),
        Element::Paragraph(e) => render_paragraph(html, e),
        Element::Markdown(e) => render_markdown(html, e),
        Element::SummaryCards(e) => render_summary_cards(html, e),
        Element::Chart(e) => render_chart(html, e, images, charts),
        Element::Table(e) => render_table(html, e),
        Element::List(e) => render_list(html, e),
        Element::Alert(e) => render_alert(html, e),
        Element::Divider(e) => render_divider(html, e),
        Element::Image(e) => render_image(html, e),
    }
}

fn render_section(html: &mut String, el: &ElementSection, images: &[EmbeddedImage], mut charts: Option<&mut Vec<(String, serde_json::Value)>>, show_mode: &ShowMode) {
    html.push_str("<div class=\"section\">");
    if matches!(show_mode, ShowMode::Full) {
        if let Some(title) = &el.common.title {
            wln!(html, "<div class=\"section-title\">{}</div>", escape_html(title));
        }
    }
    for child in &el.elements {
        render_element(html, child, images, charts.as_deref_mut(), show_mode);
    }
    html.push_str("</div>\n");
}

fn render_row(html: &mut String, el: &ElementRow, images: &[EmbeddedImage], mut charts: Option<&mut Vec<(String, serde_json::Value)>>, show_mode: &ShowMode) {
    let gap = el.common.gap.unwrap_or(8);
    html.push_str(&format!("<div class=\"row\" style=\"gap:{}px\">", gap));
    for child in &el.elements {
        render_element(html, child, images, charts.as_deref_mut(), show_mode);
    }
    html.push_str("</div>\n");
}

fn render_column(html: &mut String, el: &ElementColumn, images: &[EmbeddedImage], mut charts: Option<&mut Vec<(String, serde_json::Value)>>, show_mode: &ShowMode) {
    let flex = el.common.flex.as_ref().map(|f| match f {
        FlexValue::Number(n) => format!("flex:{}", n),
        FlexValue::String(s) => format!("flex:{}", s),
    }).unwrap_or_else(|| "flex:1".to_string());
    html.push_str(&format!("<div class=\"column\" style=\"{}\">", flex));
    for child in &el.elements {
        render_element(html, child, images, charts.as_deref_mut(), show_mode);
    }
    html.push_str("</div>\n");
}

fn render_heading(html: &mut String, el: &ElementHeading) {
    let tag = match el.level {
        1 => "h1",
        2 => "h2",
        3 => "h3",
        _ => "h4",
    };
    wln!(html, "<{}>{}</{}>", tag, escape_html(&el.content), tag);
}

fn render_paragraph(html: &mut String, el: &ElementParagraph) {
    wln!(html, "<p>{}</p>", escape_html(&el.content));
}

fn render_markdown(html: &mut String, el: &ElementMarkdown) {
    // Simple markdown-to-HTML: just wrap in a div, full conversion needs a crate
    wln!(html, "<div class=\"markdown\"><pre>{}</pre></div>", escape_html(&el.content));
}

fn render_summary_cards(html: &mut String, el: &ElementSummaryCards) {
    html.push_str("<div class=\"cards\">");
    for card in &el.cards {
        html.push_str("<div class=\"card\">");
        wln!(html, "<div class=\"card-label\">{}</div>", escape_html(&card.label));
        wln!(html, "<div class=\"card-value\">{}</div>", escape_html(&card.value));
        if let Some(trend) = &card.trend {
            let cls = card.trend_type.as_deref().unwrap_or("");
            wln!(html, "<div class=\"card-trend {}\">{}</div>", cls, escape_html(trend));
        }
        html.push_str("</div>\n");
    }
    html.push_str("</div>\n");
}

fn render_chart(html: &mut String, el: &ElementChart, images: &[EmbeddedImage], charts: Option<&mut Vec<(String, serde_json::Value)>>) {
    // ECharts path (screen HTML)
    if let Some(charts) = charts {
        let id = format!("chart-{}", charts.len());
        html.push_str("<div class=\"chart-container\">");
        if let Some(title) = &el.title {
            wln!(html, "<div class=\"chart-title\">{}</div>", escape_html(title));
        }
        wln!(html, "<div id=\"{}\" class=\"echarts-chart\"></div>", id);
        html.push_str("</div>\n");
        charts.push((id, crate::echarts_render::build_echarts_option(el)));
        return;
    }

    // charton PNG path (PDF) — wrapper div forces ironpress to use block-layout
    // for the img tag, avoiding FlexTextRunCollector which silently drops img elements.
    html.push_str("<div class=\"chart-container\">");
    if let Some(title) = &el.title {
        wln!(html, "<div class=\"chart-title\">{}</div>", escape_html(title));
    }

    // Try to find embedded image for this chart
    let chart_key = format!("chart-{}", el.common.id.as_deref().unwrap_or(""));
    if let Some(img) = images.iter().find(|i| i.key == chart_key || i.key == el.common.id.as_deref().unwrap_or("")) {
        let data_url = format!("data:image/png;base64,{}", base64_encode(&img.data));
        wln!(html, "<img class=\"chart-img\" src=\"{}\" width=\"600\" height=\"375\" alt=\"chart\">", data_url);
    } else if let Ok(Some(png_data)) = crate::chart_render::render_chart(el) {
        let data_url = format!("data:image/png;base64,{}", base64_encode(&png_data));
        wln!(html, "<img class=\"chart-img\" src=\"{}\" width=\"600\" height=\"375\" alt=\"chart\">", data_url);
    } else {
        render_chart_fallback(html, el);
    }
    html.push_str("</div>\n");
}

fn render_chart_fallback(html: &mut String, el: &ElementChart) {
    // Simple text representation for charts without images
    wln!(html, "<table class=\"chart-fallback\">");
    wln!(html, "<tr><th>{}</th>", el.data.categories.first().map(|_| "Category").unwrap_or(""));
    for series in &el.data.series {
        wln!(html, "<th>{}</th>", escape_html(&series.name));
    }
    wln!(html, "</tr>");
    for (i, cat) in el.data.categories.iter().enumerate() {
        wln!(html, "<tr><td>{}</td>", escape_html(cat));
        for series in &el.data.series {
            let val = series.values.get(i).copied().unwrap_or(0.0);
            wln!(html, "<td>{:.2}</td>", val);
        }
        wln!(html, "</tr>");
    }
    wln!(html, "</table>");
}

fn render_table(html: &mut String, el: &ElementTable) {
    if let Some(title) = &el.title {
        wln!(html, "<div class=\"chart-title\">{}</div>", escape_html(title));
    }
    html.push_str("<table>");
    html.push_str("<tr>");
    for col in &el.columns {
        let align = col.align.as_ref().map(|a| match a {
            Align::Left => "text-align:left",
            Align::Right => "text-align:right",
            Align::Center => "text-align:center",
        }).unwrap_or("");
        if align.is_empty() {
            wln!(html, "<th>{}</th>", escape_html(&col.label));
        } else {
            wln!(html, "<th style=\"{}\">{}</th>", align, escape_html(&col.label));
        }
    }
    html.push_str("</tr>\n");
    for row in &el.rows {
        html.push_str("<tr>");
        for col in &el.columns {
            let align = col.align.as_ref().map(|a| match a {
                Align::Left => "text-align:left",
                Align::Right => "text-align:right",
                Align::Center => "text-align:center",
            }).unwrap_or("");
            let val = row.get(&col.key)
                .map(|v| v.as_str().map(|s| s.to_string()).unwrap_or_else(|| v.to_string()))
                .unwrap_or_default();
            if align.is_empty() {
                wln!(html, "<td>{}</td>", escape_html(&val));
            } else {
                wln!(html, "<td style=\"{}\">{}</td>", align, escape_html(&val));
            }
        }
        html.push_str("</tr>\n");
    }
    html.push_str("</table>\n");
}

fn render_list(html: &mut String, el: &ElementList) {
    let tag = match el.list_type {
        ListType::Ordered => "ol",
        ListType::Unordered => "ul",
    };
    wln!(html, "<{}>", tag);
    for item in &el.items {
        wln!(html, "<li>{}</li>", escape_html(item));
    }
    wln!(html, "</{}>", tag);
}

fn render_alert(html: &mut String, el: &ElementAlert) {
    let cls = match el.alert_type {
        AlertType::Error => "alert-error",
        AlertType::Warning => "alert-warning",
        AlertType::Info => "alert-info",
        AlertType::Success => "alert-success",
    };
    html.push_str(&format!("<div class=\"alert {}\">", cls));
    if let Some(title) = &el.title {
        wln!(html, "<div class=\"alert-title\">{}</div>", escape_html(title));
    }
    wln!(html, "<div class=\"alert-content\">{}</div>", escape_html(&el.content));
    if let Some(conf) = el.confidence {
        wln!(html, "<div class=\"alert-confidence\">置信度: {:.0}%</div>", conf * 100.0);
    }
    html.push_str("</div>\n");
}

fn render_divider(html: &mut String, _el: &ElementDivider) {
    html.push_str("<hr class=\"divider\">\n");
}

fn render_image(html: &mut String, el: &ElementImage) {
    let width = el.width.map(|w| format!("width=\"{}\"", w)).unwrap_or_default();
    let caption_class = if el.caption.is_some() { " captioned" } else { "" };
    wln!(html, "<img src=\"{}\" {} alt=\"{}\" class=\"{}\">",
        escape_html(&el.src), width,
        el.alt.as_deref().unwrap_or(""), caption_class);
    if let Some(caption) = &el.caption {
        wln!(html, "<div class=\"image-caption\">{}</div>", escape_html(caption));
    }
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn base64_encode(data: &[u8]) -> String {
    use std::fmt::Write;
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
    let mut i = 0;
    while i < data.len() {
        let b0 = data[i] as u32;
        let b1 = if i + 1 < data.len() { data[i + 1] as u32 } else { 0 };
        let b2 = if i + 2 < data.len() { data[i + 2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        write!(out, "{}{}{}{}",
            CHARS[((triple >> 18) & 0x3f) as usize] as char,
            CHARS[((triple >> 12) & 0x3f) as usize] as char,
            if i + 1 < data.len() { CHARS[((triple >> 6) & 0x3f) as usize] as char } else { '=' },
            if i + 2 < data.len() { CHARS[(triple & 0x3f) as usize] as char } else { '=' },
        ).unwrap();
        i += 3;
    }
    out
}
