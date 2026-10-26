use serde::{Deserialize, Serialize};

/// AuPage document root
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuPageDocument {
    pub version: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<AuPageMeta>,
    pub elements: Vec<Element>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuPageMeta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generated_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub margin: Option<AuPageMargin>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_family: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_size: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuPageMargin {
    #[serde(default = "default_margin")]
    pub top: u32,
    #[serde(default = "default_margin")]
    pub right: u32,
    #[serde(default = "default_margin")]
    pub bottom: u32,
    #[serde(default = "default_margin")]
    pub left: u32,
}

fn default_margin() -> u32 { 72 }

/// All element types share common fields via this wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Element {
    Section(ElementSection),
    Row(ElementRow),
    Column(ElementColumn),
    Heading(ElementHeading),
    Paragraph(ElementParagraph),
    Markdown(ElementMarkdown),
    SummaryCards(ElementSummaryCards),
    Chart(ElementChart),
    Table(ElementTable),
    List(ElementList),
    Alert(ElementAlert),
    Divider(ElementDivider),
    Image(ElementImage),
}

impl Element {
    pub fn common(&self) -> &ElementCommon {
        match self {
            Element::Section(e) => &e.common,
            Element::Row(e) => &e.common,
            Element::Column(e) => &e.common,
            Element::Heading(e) => &e.common,
            Element::Paragraph(e) => &e.common,
            Element::Markdown(e) => &e.common,
            Element::SummaryCards(e) => &e.common,
            Element::Chart(e) => &e.common,
            Element::Table(e) => &e.common,
            Element::List(e) => &e.common,
            Element::Alert(e) => &e.common,
            Element::Divider(e) => &e.common,
            Element::Image(e) => &e.common,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ElementCommon {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collapsed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flex: Option<FlexValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grow: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub align: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub justify: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gap: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FlexValue {
    Number(f64),
    String(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElementSection {
    #[serde(flatten)]
    pub common: ElementCommon,
    pub elements: Vec<Element>,
    #[serde(default)]
    pub page_break: PageBreak,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PageBreak {
    #[default]
    Auto,
    Always,
    Avoid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElementRow {
    #[serde(flatten)]
    pub common: ElementCommon,
    pub elements: Vec<Element>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wrap: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElementColumn {
    #[serde(flatten)]
    pub common: ElementCommon,
    pub elements: Vec<Element>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElementHeading {
    #[serde(flatten)]
    pub common: ElementCommon,
    pub level: u32,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElementParagraph {
    #[serde(flatten)]
    pub common: ElementCommon,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElementMarkdown {
    #[serde(flatten)]
    pub common: ElementCommon,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElementSummaryCards {
    #[serde(flatten)]
    pub common: ElementCommon,
    pub cards: Vec<Card>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Card {
    pub label: String,
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trend: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trend_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElementChart {
    #[serde(flatten)]
    pub common: ElementCommon,
    pub chart_type: ChartType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub data: ChartData,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<ChartConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChartType {
    Bar,
    Column,
    Line,
    Area,
    Pie,
    Ring,
    Scatter,
    Radar,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChartData {
    pub categories: Vec<String>,
    pub series: Vec<ChartSeries>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChartSeries {
    pub name: String,
    pub values: Vec<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chart_type: Option<ChartType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub y_axis_index: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub y_axis_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub smooth: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChartConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x_axis_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub y_axis_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_label: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_legend: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElementTable {
    #[serde(flatten)]
    pub common: ElementCommon,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub columns: Vec<TableColumn>,
    pub rows: Vec<serde_json::Map<String, serde_json::Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableColumn {
    pub key: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub align: Option<Align>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Align {
    Left,
    Right,
    Center,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElementList {
    #[serde(flatten)]
    pub common: ElementCommon,
    #[serde(default)]
    pub list_type: ListType,
    pub items: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ListType {
    #[default]
    Unordered,
    Ordered,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElementAlert {
    #[serde(flatten)]
    pub common: ElementCommon,
    pub alert_type: AlertType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertType {
    Error,
    Warning,
    Info,
    Success,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElementDivider {
    #[serde(flatten)]
    pub common: ElementCommon,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElementImage {
    #[serde(flatten)]
    pub common: ElementCommon,
    pub src: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
}

/// Export format
#[derive(Debug, Clone, clap::ValueEnum)]
pub enum ExportFormat {
    Pdf,
    Docx,
    Html,
}

/// Rendering detail level
#[derive(Debug, Clone, Default, clap::ValueEnum)]
#[clap(rename_all = "snake_case")]
pub enum ShowMode {
    /// Compact: hide section titles
    #[default]
    Compact,
    /// Full: show all elements including section titles
    Full,
}

/// Embedded image for export (base64 PNG)
#[derive(Debug, Clone)]
pub struct EmbeddedImage {
    /// Key to match chart references (e.g. "chart-0", "chart-1")
    pub key: String,
    /// PNG bytes
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
}
