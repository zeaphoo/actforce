//! Manual `quick-xml` parser for OFD page-content documents.
//!
//! serde-xml-rs cannot cope with the *interleaved* repetition of
//! `TextObject`, `PathObject`, `ImageObject`, and `PageBlock` siblings that
//! OFD authors in the wild routinely emit. We therefore walk the XML event
//! stream ourselves and populate the flat vectors on the model structs.

use quick_xml::events::attributes::Attributes;
use quick_xml::events::Event;
use quick_xml::name::QName;
use quick_xml::reader::Reader;

use crate::error::{OfdError, Result};
use crate::model::{
    Appearance, ColorNode, Content, ImageObject, Layer, Page, PageAnnot, PageArea, PageBlock,
    PathObject, TextCode, TextObject,
};

fn local(name: QName<'_>) -> String {
    let raw = name.as_ref();
    let bytes = if let Some(pos) = raw.iter().position(|&b| b == b':') {
        &raw[pos + 1..]
    } else {
        raw
    };
    String::from_utf8_lossy(bytes).into_owned()
}

fn attr_map(attrs: Attributes<'_>) -> std::collections::HashMap<String, String> {
    let mut m = std::collections::HashMap::new();
    let tmp_reader: Reader<&[u8]> = Reader::from_reader(&b""[..]);
    for a in attrs.flatten() {
        let key = local(a.key);
        let value = a
            .decode_and_unescape_value(tmp_reader.decoder())
            .map(|c| c.into_owned())
            .unwrap_or_else(|_| String::from_utf8_lossy(&a.value).into_owned());
        m.insert(key, value);
    }
    m
}

fn parse_f64(m: &std::collections::HashMap<String, String>, k: &str) -> f64 {
    m.get(k).and_then(|v| v.parse().ok()).unwrap_or(0.0)
}
fn parse_u64(m: &std::collections::HashMap<String, String>, k: &str) -> u64 {
    m.get(k).and_then(|v| v.parse().ok()).unwrap_or(0)
}
fn parse_bool(m: &std::collections::HashMap<String, String>, k: &str) -> Option<bool> {
    m.get(k)
        .map(|v| v == "true" || v == "1" || v.eq_ignore_ascii_case("yes"))
}

pub fn parse_page(xml: &str) -> Result<Page> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut page = Page::default();
    let mut stack: Vec<String> = Vec::new();
    // Track current path of "Area", "Content".
    let mut in_area = false;
    let mut in_content = false;
    let mut layer: Option<Layer> = None;

    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Err(e) => return Err(OfdError::Structure(format!("xml: {e}"))),
            Ok(Event::Eof) => break,
            Ok(Event::Start(e)) => {
                let tag = local(e.name());
                let attrs = attr_map(e.attributes());
                match tag.as_str() {
                    "Area" => in_area = true,
                    "PhysicalBox" | "ApplicationBox" if in_area => {
                        // Parse following Text event.
                    }
                    "Content" => in_content = true,
                    "Layer" if in_content => {
                        layer = Some(Layer {
                            id: attrs.get("ID").and_then(|v| v.parse().ok()),
                            r#type: attrs.get("Type").cloned(),
                            ..Default::default()
                        });
                    }
                    "PageBlock" | "TextObject" | "ImageObject" | "PathObject" if in_content => {
                        // For these we call the recursive object parser.
                        let obj = parse_object(&tag, attrs, &mut reader, &mut buf)?;
                        push_obj_to_layer(layer.as_mut(), obj);
                    }
                    _ => {}
                }
                stack.push(tag);
            }
            Ok(Event::Empty(e)) => {
                let tag = local(e.name());
                let attrs = attr_map(e.attributes());
                if in_content
                    && matches!(
                        tag.as_str(),
                        "PageBlock" | "TextObject" | "ImageObject" | "PathObject"
                    )
                {
                    let obj = build_leaf(&tag, &attrs);
                    push_obj_to_layer(layer.as_mut(), obj);
                }
            }
            Ok(Event::Text(t)) => {
                let top = stack.last().cloned().unwrap_or_default();
                if top == "PhysicalBox" && in_area {
                    let v = t
                        .unescape()
                        .map(|c| c.into_owned())
                        .unwrap_or_else(|_| String::from_utf8_lossy(&t).into_owned());
                    page.area
                        .get_or_insert_with(PageArea::default)
                        .physical_box = v;
                } else if top == "ApplicationBox" && in_area {
                    let v = t
                        .unescape()
                        .map(|c| c.into_owned())
                        .unwrap_or_else(|_| String::from_utf8_lossy(&t).into_owned());
                    page.area
                        .get_or_insert_with(PageArea::default)
                        .application_box = Some(v);
                }
            }
            Ok(Event::End(e)) => {
                stack.pop();
                let tag = local(e.name());
                match tag.as_str() {
                    "Area" => in_area = false,
                    "Layer" => {
                        if let Some(l) = layer.take() {
                            page.content.get_or_insert_with(Content::default).layers.push(l);
                        }
                    }
                    "Content" => in_content = false,
                    _ => {}
                }
            }
            _ => {}
        }
        buf.clear();
    }
    Ok(page)
}

enum ObjKind {
    Text(TextObject),
    Image(ImageObject),
    Path(PathObject),
    Block(PageBlock),
}

fn push_obj_to_layer(layer: Option<&mut Layer>, o: ObjKind) {
    let Some(l) = layer else { return };
    match o {
        ObjKind::Text(t) => l.text_objects.push(t),
        ObjKind::Image(i) => l.image_objects.push(i),
        ObjKind::Path(p) => l.path_objects.push(p),
        ObjKind::Block(b) => l.page_blocks.push(b),
    }
}

fn push_obj_to_block(block: &mut PageBlock, o: ObjKind) {
    match o {
        ObjKind::Text(t) => block.text_objects.push(t),
        ObjKind::Image(i) => block.image_objects.push(i),
        ObjKind::Path(p) => block.path_objects.push(p),
        ObjKind::Block(b) => block.page_blocks.push(b),
    }
}

fn build_leaf(tag: &str, a: &std::collections::HashMap<String, String>) -> ObjKind {
    match tag {
        "TextObject" => ObjKind::Text(TextObject {
            id: parse_u64(a, "ID"),
            boundary: a.get("Boundary").cloned().unwrap_or_default(),
            ctm: a.get("CTM").cloned(),
            font: parse_u64(a, "Font"),
            size: parse_f64(a, "Size"),
            fill_color: None,
            text_code: Vec::new(),
        }),
        "ImageObject" => ObjKind::Image(ImageObject {
            id: parse_u64(a, "ID"),
            boundary: a.get("Boundary").cloned().unwrap_or_default(),
            ctm: a.get("CTM").cloned(),
            resource_id: parse_u64(a, "ResourceID"),
        }),
        "PathObject" => ObjKind::Path(PathObject {
            id: parse_u64(a, "ID"),
            boundary: a.get("Boundary").cloned().unwrap_or_default(),
            ctm: a.get("CTM").cloned(),
            line_width: a.get("LineWidth").and_then(|v| v.parse().ok()),
            stroke: parse_bool(a, "Stroke"),
            fill: parse_bool(a, "Fill"),
            stroke_color: None,
            fill_color: None,
            abbreviated_data: None,
        }),
        "PageBlock" => ObjKind::Block(PageBlock {
            id: a.get("ID").and_then(|v| v.parse().ok()),
            ..Default::default()
        }),
        _ => ObjKind::Path(PathObject::default()),
    }
}

/// Parse an object whose opening tag has already been consumed.
/// Child `TextCode` / color / data elements are attached to the right fields.
fn parse_object<B: std::io::BufRead>(
    tag: &str,
    attrs: std::collections::HashMap<String, String>,
    reader: &mut Reader<B>,
    buf: &mut Vec<u8>,
) -> Result<ObjKind> {
    let mut obj = build_leaf(tag, &attrs);
    // State for child elements:
    let mut cur_child: Option<String> = None;
    let mut cur_child_attrs: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    let mut cur_child_text = String::new();

    let mut depth: i32 = 1; // we are already inside <tag>
    loop {
        match reader
            .read_event_into(buf)
            .map_err(|e| OfdError::Structure(format!("xml: {e}")))?
        {
            Event::Eof => break,
            Event::Start(e) => {
                depth += 1;
                let t = local(e.name());
                let a = attr_map(e.attributes());
                // Recurse for nested objects inside PageBlock.
                if matches!(t.as_str(), "TextObject" | "ImageObject" | "PathObject" | "PageBlock") {
                    let inner = parse_object(&t, a, reader, buf)?;
                    depth -= 1; // parse_object consumed up to matching End
                    if let ObjKind::Block(b) = &mut obj {
                        push_obj_to_block(b, inner);
                    }
                } else {
                    cur_child = Some(t);
                    cur_child_attrs = a;
                    cur_child_text.clear();
                }
            }
            Event::Empty(e) => {
                let t = local(e.name());
                let a = attr_map(e.attributes());
                if matches!(t.as_str(), "TextObject" | "ImageObject" | "PathObject" | "PageBlock") {
                    if let ObjKind::Block(b) = &mut obj {
                        push_obj_to_block(b, build_leaf(&t, &a));
                    }
                } else {
                    apply_child(&mut obj, &t, &a, "");
                }
            }
            Event::Text(txt) => {
                if cur_child.is_some() {
                    let v = txt
                        .unescape()
                        .map(|c| c.into_owned())
                        .unwrap_or_else(|_| String::from_utf8_lossy(&txt).into_owned());
                    cur_child_text.push_str(&v);
                }
            }
            Event::CData(txt) => {
                if cur_child.is_some() {
                    let v = String::from_utf8_lossy(&txt).into_owned();
                    cur_child_text.push_str(&v);
                }
            }
            Event::End(_) => {
                depth -= 1;
                if let Some(child) = cur_child.take() {
                    apply_child(&mut obj, &child, &cur_child_attrs, &cur_child_text);
                    cur_child_attrs.clear();
                    cur_child_text.clear();
                }
                if depth == 0 {
                    break;
                }
            }
            _ => {}
        }
        buf.clear();
    }
    Ok(obj)
}

fn apply_child(
    obj: &mut ObjKind,
    child: &str,
    attrs: &std::collections::HashMap<String, String>,
    text: &str,
) {
    match obj {
        ObjKind::Text(t) => match child {
            "FillColor" => {
                t.fill_color = Some(ColorNode {
                    value: attrs.get("Value").cloned().unwrap_or_default(),
                    alpha: attrs.get("Alpha").and_then(|v| v.parse().ok()),
                });
            }
            "TextCode" => {
                t.text_code.push(TextCode {
                    x: attrs.get("X").and_then(|v| v.parse().ok()),
                    y: attrs.get("Y").and_then(|v| v.parse().ok()),
                    delta_x: attrs.get("DeltaX").cloned(),
                    delta_y: attrs.get("DeltaY").cloned(),
                    value: text.to_string(),
                });
            }
            _ => {}
        },
        ObjKind::Path(p) => match child {
            "StrokeColor" => {
                p.stroke_color = Some(ColorNode {
                    value: attrs.get("Value").cloned().unwrap_or_default(),
                    alpha: attrs.get("Alpha").and_then(|v| v.parse().ok()),
                });
            }
            "FillColor" => {
                p.fill_color = Some(ColorNode {
                    value: attrs.get("Value").cloned().unwrap_or_default(),
                    alpha: attrs.get("Alpha").and_then(|v| v.parse().ok()),
                });
            }
            "AbbreviatedData" => {
                p.abbreviated_data = Some(text.to_string());
            }
            _ => {}
        },
        _ => {}
    }
}

/// Parse `<ofd:PageAnnot>` documents (annotations per-page). Uses the same
/// strategy as page content so stamps with interleaved children work.
pub fn parse_page_annot(xml: &str) -> Result<PageAnnot> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut annot_out = PageAnnot::default();
    let mut buf = Vec::new();
    let mut cur_annot: Option<crate::model::Annot> = None;
    let mut cur_app: Option<Appearance> = None;

    loop {
        match reader
            .read_event_into(&mut buf)
            .map_err(|e| OfdError::Structure(format!("xml: {e}")))?
        {
            Event::Eof => break,
            Event::Start(e) => {
                let tag = local(e.name());
                let attrs = attr_map(e.attributes());
                match tag.as_str() {
                    "Annot" => {
                        cur_annot = Some(crate::model::Annot {
                            id: parse_u64(&attrs, "ID"),
                            r#type: attrs.get("Type").cloned().unwrap_or_default(),
                            appearance: None,
                        });
                    }
                    "Appearance" => {
                        cur_app = Some(Appearance {
                            boundary: attrs.get("Boundary").cloned().unwrap_or_default(),
                            ..Default::default()
                        });
                    }
                    "TextObject" | "ImageObject" | "PathObject" | "PageBlock" => {
                        let obj = parse_object(&tag, attrs, &mut reader, &mut buf)?;
                        if let Some(app) = cur_app.as_mut() {
                            push_obj_to_appearance(app, obj);
                        }
                    }
                    _ => {}
                }
            }
            Event::Empty(e) => {
                let tag = local(e.name());
                let attrs = attr_map(e.attributes());
                if matches!(tag.as_str(), "TextObject" | "ImageObject" | "PathObject" | "PageBlock")
                {
                    if let Some(app) = cur_app.as_mut() {
                        push_obj_to_appearance(app, build_leaf(&tag, &attrs));
                    }
                }
            }
            Event::End(e) => {
                let tag = local(e.name());
                match tag.as_str() {
                    "Appearance" => {
                        if let (Some(app), Some(annot)) = (cur_app.take(), cur_annot.as_mut()) {
                            annot.appearance = Some(app);
                        }
                    }
                    "Annot" => {
                        if let Some(a) = cur_annot.take() {
                            annot_out.annot.push(a);
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        buf.clear();
    }
    Ok(annot_out)
}

fn push_obj_to_appearance(app: &mut Appearance, o: ObjKind) {
    match o {
        ObjKind::Text(t) => app.text_objects.push(t),
        ObjKind::Image(i) => app.image_objects.push(i),
        ObjKind::Path(p) => app.path_objects.push(p),
        ObjKind::Block(b) => app.page_blocks.push(b),
    }
}
