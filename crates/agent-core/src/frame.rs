use serde::{Deserialize, Serialize};

/// 基础 Frame 结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaseFrame {
    pub render: FrameContent,
    #[serde(default)]
    pub children: Vec<BaseFrame>,
}

impl BaseFrame {
    pub fn to_dict(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or_default()
    }
}

/// Frame 内容类型
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FrameContent {
    Single(String),
    Multiple(Vec<String>),
}

impl Default for FrameContent {
    fn default() -> Self {
        FrameContent::Single(String::new())
    }
}

/// 页面 Frame
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageFrame {
    #[serde(flatten)]
    pub base: BaseFrame,
}

impl PageFrame {
    pub fn new(render: impl Into<FrameContent>) -> Self {
        Self {
            base: BaseFrame {
                render: render.into(),
                children: Vec::new(),
            },
        }
    }
}

impl From<String> for FrameContent {
    fn from(s: String) -> Self {
        FrameContent::Single(s)
    }
}

impl From<&str> for FrameContent {
    fn from(s: &str) -> Self {
        FrameContent::Single(s.to_string())
    }
}

impl From<Vec<String>> for FrameContent {
    fn from(v: Vec<String>) -> Self {
        FrameContent::Multiple(v)
    }
}

impl From<Vec<&str>> for FrameContent {
    fn from(v: Vec<&str>) -> Self {
        FrameContent::Multiple(v.iter().map(|s| s.to_string()).collect())
    }
}

/// 章节 Frame
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SectionFrame {
    #[serde(flatten)]
    pub base: BaseFrame,
}

impl SectionFrame {
    pub fn new(render: impl Into<FrameContent>) -> Self {
        Self {
            base: BaseFrame {
                render: render.into(),
                children: Vec::new(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base_frame() {
        let frame = BaseFrame {
            render: FrameContent::Single("Hello".to_string()),
            children: vec![],
        };
        let dict = frame.to_dict();
        assert!(dict.get("render").is_some());
    }

    #[test]
    fn test_page_frame() {
        let frame = PageFrame::new("Page content");
        assert!(matches!(frame.base.render, FrameContent::Single(_)));
    }

    #[test]
    fn test_section_frame() {
        let frame = SectionFrame::new(vec!["Section 1", "Section 2"]);
        assert!(matches!(frame.base.render, FrameContent::Multiple(_)));
    }
}
