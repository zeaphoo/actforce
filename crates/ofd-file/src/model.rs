//! OFD XML serde model. Only the fields useful for extraction / conversion are
//! modelled; everything else is kept optional so we stay forward-compatible
//! with real-world OFD variants.

use serde::Deserialize;

use crate::error::Result;

// ---------------------------------------------------------------------------
// OFD.xml (root)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct OfdRoot {
    #[serde(rename = "DocBody", default)]
    pub doc_body: Vec<DocBody>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct DocBody {
    #[serde(default)]
    pub doc_info: DocInfo,
    pub doc_root: String,
    #[serde(default)]
    pub signatures: Option<String>,
}

#[derive(Debug, Deserialize, Default, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct DocInfo {
    #[serde(rename = "DocID", default)]
    pub doc_id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub subject: String,
    #[serde(default)]
    pub creator: String,
    #[serde(default)]
    pub creator_version: String,
    #[serde(default)]
    pub creation_date: String,
    #[serde(default)]
    pub mod_date: String,
    #[serde(default, rename = "CustomDatas")]
    pub custom_datas: Option<CustomDatas>,
}

#[derive(Debug, Deserialize, Default, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct CustomDatas {
    #[serde(default, rename = "CustomData")]
    pub custom_data: Vec<CustomData>,
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct CustomData {
    #[serde(rename = "Name")]
    pub name: Option<String>,
    #[serde(rename = "$value", default)]
    pub value: String,
}

impl OfdRoot {
    pub fn from_xml(xml: &str) -> Result<Self> {
        Ok(serde_xml_rs::from_str(xml)?)
    }
}

// ---------------------------------------------------------------------------
// Document.xml
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct Document {
    pub common_data: CommonData,
    #[serde(default)]
    pub pages: PageList,
    #[serde(default)]
    pub annotations: Option<String>,
    #[serde(default)]
    pub custom_tags: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct CommonData {
    #[serde(default)]
    pub page_area: PageArea,
    #[serde(default, rename = "PublicRes")]
    pub public_res: Vec<String>,
    #[serde(default, rename = "DocumentRes")]
    pub document_res: Vec<String>,
    #[serde(default, rename = "MaxUnitID")]
    pub max_unit_id: u64,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct PageArea {
    pub physical_box: String,
    #[serde(default)]
    pub application_box: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct PageList {
    #[serde(default, rename = "Page")]
    pub page: Vec<PageRef>,
}

#[derive(Debug, Deserialize, Default)]
pub struct PageRef {
    #[serde(rename = "BaseLoc")]
    pub base_loc: Option<String>,
    #[serde(rename = "ID")]
    pub id: Option<u64>,
}

impl Document {
    pub fn from_xml(xml: &str) -> Result<Self> {
        Ok(serde_xml_rs::from_str(xml)?)
    }
}

// ---------------------------------------------------------------------------
// Res (PublicRes.xml / DocumentRes.xml)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct Res {
    #[serde(rename = "BaseLoc", default)]
    pub base_loc: Option<String>,
    #[serde(default)]
    pub fonts: Option<Fonts>,
    #[serde(default)]
    pub multi_medias: Option<MultiMedias>,
    #[serde(default, rename = "ColorSpaces")]
    pub color_spaces: Option<ColorSpaces>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct Fonts {
    #[serde(default, rename = "Font")]
    pub font: Vec<Font>,
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct Font {
    #[serde(rename = "ID")]
    pub id: u64,
    #[serde(rename = "FamilyName", default)]
    pub family_name: String,
    #[serde(rename = "FontName", default)]
    pub font_name: String,
    #[serde(rename = "FontFile", default)]
    pub font_file: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct MultiMedias {
    #[serde(default, rename = "MultiMedia")]
    pub multi_media: Vec<MultiMedia>,
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct MultiMedia {
    #[serde(rename = "ID")]
    pub id: u64,
    #[serde(rename = "Type", default)]
    pub r#type: String,
    #[serde(rename = "Format", default)]
    pub format: String,
    #[serde(rename = "MediaFile", default)]
    pub media_file: String,
}

#[derive(Debug, Deserialize, Default)]
pub struct ColorSpaces {}

impl Res {
    pub fn from_xml(xml: &str) -> Result<Self> {
        Ok(serde_xml_rs::from_str(xml)?)
    }
}

// ---------------------------------------------------------------------------
// Page content (Pages/Page_N/Content.xml)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct Page {
    #[serde(default)]
    pub area: Option<PageArea>,
    #[serde(default)]
    pub content: Option<Content>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct Content {
    #[serde(default, rename = "Layer")]
    pub layers: Vec<Layer>,
}

#[derive(Debug, Deserialize, Default)]
pub struct Layer {
    #[serde(rename = "ID", default)]
    pub id: Option<u64>,
    #[serde(rename = "Type", default)]
    pub r#type: Option<String>,
    #[serde(default, rename = "TextObject")]
    pub text_objects: Vec<TextObject>,
    #[serde(default, rename = "ImageObject")]
    pub image_objects: Vec<ImageObject>,
    #[serde(default, rename = "PathObject")]
    pub path_objects: Vec<PathObject>,
    #[serde(default, rename = "PageBlock")]
    pub page_blocks: Vec<PageBlock>,
}

/// A lightweight, owned enum used by consumers for uniform iteration.
#[derive(Debug, Clone)]
pub enum PageObject {
    TextObject(TextObject),
    ImageObject(ImageObject),
    PathObject(PathObject),
    PageBlock(PageBlock),
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct PageBlock {
    #[serde(rename = "ID", default)]
    pub id: Option<u64>,
    #[serde(default, rename = "TextObject")]
    pub text_objects: Vec<TextObject>,
    #[serde(default, rename = "ImageObject")]
    pub image_objects: Vec<ImageObject>,
    #[serde(default, rename = "PathObject")]
    pub path_objects: Vec<PathObject>,
    #[serde(default, rename = "PageBlock")]
    pub page_blocks: Vec<PageBlock>,
}

#[derive(Debug, Deserialize, Default, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct TextObject {
    #[serde(rename = "ID", default)]
    pub id: u64,
    #[serde(rename = "Boundary", default)]
    pub boundary: String,
    #[serde(rename = "CTM", default)]
    pub ctm: Option<String>,
    #[serde(rename = "Font", default)]
    pub font: u64,
    #[serde(rename = "Size", default)]
    pub size: f64,
    #[serde(default)]
    pub fill_color: Option<ColorNode>,
    #[serde(default, rename = "TextCode")]
    pub text_code: Vec<TextCode>,
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct ColorNode {
    #[serde(rename = "Value", default)]
    pub value: String,
    #[serde(rename = "Alpha", default)]
    pub alpha: Option<f64>,
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct TextCode {
    #[serde(rename = "X", default)]
    pub x: Option<f64>,
    #[serde(rename = "Y", default)]
    pub y: Option<f64>,
    #[serde(rename = "DeltaX", default)]
    pub delta_x: Option<String>,
    #[serde(rename = "DeltaY", default)]
    pub delta_y: Option<String>,
    #[serde(rename = "$value", default)]
    pub value: String,
}

#[derive(Debug, Deserialize, Default, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct ImageObject {
    #[serde(rename = "ID", default)]
    pub id: u64,
    #[serde(rename = "Boundary", default)]
    pub boundary: String,
    #[serde(rename = "CTM", default)]
    pub ctm: Option<String>,
    #[serde(rename = "ResourceID", default)]
    pub resource_id: u64,
}

#[derive(Debug, Deserialize, Default, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct PathObject {
    #[serde(rename = "ID", default)]
    pub id: u64,
    #[serde(rename = "Boundary", default)]
    pub boundary: String,
    #[serde(rename = "CTM", default)]
    pub ctm: Option<String>,
    #[serde(rename = "LineWidth", default)]
    pub line_width: Option<f64>,
    #[serde(rename = "Stroke", default)]
    pub stroke: Option<bool>,
    #[serde(rename = "Fill", default)]
    pub fill: Option<bool>,
    #[serde(default)]
    pub stroke_color: Option<ColorNode>,
    #[serde(default)]
    pub fill_color: Option<ColorNode>,
    #[serde(default, rename = "AbbreviatedData")]
    pub abbreviated_data: Option<String>,
}

impl Page {
    pub fn from_xml(xml: &str) -> Result<Self> {
        Ok(serde_xml_rs::from_str(xml)?)
    }

    /// Flatten all leaf objects across layers and nested PageBlocks.
    pub fn flatten_objects(&self) -> Vec<PageObject> {
        let mut out = Vec::new();
        if let Some(c) = &self.content {
            for layer in &c.layers {
                collect_layer_like(
                    &layer.text_objects,
                    &layer.image_objects,
                    &layer.path_objects,
                    &layer.page_blocks,
                    &mut out,
                );
            }
        }
        out
    }
}

fn collect_layer_like(
    texts: &[TextObject],
    images: &[ImageObject],
    paths: &[PathObject],
    blocks: &[PageBlock],
    out: &mut Vec<PageObject>,
) {
    for t in texts { out.push(PageObject::TextObject(t.clone())); }
    for i in images { out.push(PageObject::ImageObject(i.clone())); }
    for p in paths { out.push(PageObject::PathObject(p.clone())); }
    for b in blocks {
        collect_layer_like(
            &b.text_objects,
            &b.image_objects,
            &b.path_objects,
            &b.page_blocks,
            out,
        );
    }
}

// ---------------------------------------------------------------------------
// Annotations.xml
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct Annotations {
    #[serde(default, rename = "Page")]
    pub page: Vec<AnnotPageRef>,
}

#[derive(Debug, Deserialize, Default)]
pub struct AnnotPageRef {
    #[serde(rename = "PageID", default)]
    pub page_id: u64,
    #[serde(rename = "FileLoc", default)]
    pub file_loc: String,
}

impl Annotations {
    pub fn from_xml(xml: &str) -> Result<Self> {
        Ok(serde_xml_rs::from_str(xml)?)
    }
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct PageAnnot {
    #[serde(default, rename = "Annot")]
    pub annot: Vec<Annot>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct Annot {
    #[serde(rename = "ID", default)]
    pub id: u64,
    #[serde(rename = "Type", default)]
    pub r#type: String,
    #[serde(default)]
    pub appearance: Option<Appearance>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct Appearance {
    #[serde(rename = "Boundary", default)]
    pub boundary: String,
    #[serde(default, rename = "TextObject")]
    pub text_objects: Vec<TextObject>,
    #[serde(default, rename = "ImageObject")]
    pub image_objects: Vec<ImageObject>,
    #[serde(default, rename = "PathObject")]
    pub path_objects: Vec<PathObject>,
    #[serde(default, rename = "PageBlock")]
    pub page_blocks: Vec<PageBlock>,
}

impl Appearance {
    /// Flatten annotation appearance into a list of page objects.
    pub fn flatten_objects(&self) -> Vec<PageObject> {
        let mut out = Vec::new();
        collect_layer_like(
            &self.text_objects,
            &self.image_objects,
            &self.path_objects,
            &self.page_blocks,
            &mut out,
        );
        out
    }
}

impl PageAnnot {
    pub fn from_xml(xml: &str) -> Result<Self> {
        Ok(serde_xml_rs::from_str(xml)?)
    }
}
