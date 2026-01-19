//! A2UI (Agent-to-User Interface) component tree parser.
//!
//! This module implements parsing for A2UI format, which is Google's specification
//! for AI agent visual output. It provides a standardized way for AI models to
//! describe UI components that get rendered to users.
//!
//! ## A2UI Component Types
//!
//! | Component   | Saorsa Element       | Description                     |
//! |-------------|----------------------|---------------------------------|
//! | Container   | Group                | Layout container for children   |
//! | Text        | Text                 | Text label or paragraph         |
//! | Image       | Image                | Static image                    |
//! | Button      | Text (interactive)   | Clickable button with action    |
//! | Chart       | Chart                | Data visualization              |
//! | `VideoFeed` | Video                | Live video stream               |
//!
//! ## Example A2UI JSON
//!
//! ```json
//! {
//!   "root": {
//!     "component": "container",
//!     "layout": "vertical",
//!     "children": [
//!       { "component": "text", "content": "Hello World" },
//!       { "component": "chart", "chart_type": "bar", "data": {"values": [1,2,3]} }
//!     ]
//!   },
//!   "data_model": {}
//! }
//! ```

use serde::{Deserialize, Serialize};

use crate::{Element, ElementKind, ImageFormat, Transform};

/// A2UI component tree from AI agent output.
///
/// This represents the full tree that an AI agent sends to describe
/// what should be rendered to the user.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct A2UITree {
    /// Root node of the component tree.
    pub root: A2UINode,
    /// Optional data model for data binding.
    #[serde(default)]
    pub data_model: serde_json::Value,
}

/// Style properties for A2UI nodes.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct A2UIStyle {
    /// Font size in pixels.
    #[serde(default)]
    pub font_size: Option<f32>,
    /// Text/foreground color as hex string.
    #[serde(default)]
    pub color: Option<String>,
    /// Background color as hex string.
    #[serde(default)]
    pub background: Option<String>,
    /// Width in pixels.
    #[serde(default)]
    pub width: Option<f32>,
    /// Height in pixels.
    #[serde(default)]
    pub height: Option<f32>,
    /// Padding in pixels.
    #[serde(default)]
    pub padding: Option<f32>,
    /// Margin in pixels.
    #[serde(default)]
    pub margin: Option<f32>,
}

/// A2UI component node.
///
/// Each variant represents a different UI component type from the A2UI spec.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "component", rename_all = "snake_case")]
pub enum A2UINode {
    /// A layout container for grouping children.
    Container {
        /// Child nodes.
        children: Vec<A2UINode>,
        /// Layout direction: "horizontal", "vertical", or "grid".
        #[serde(default = "default_layout")]
        layout: String,
        /// Optional styling.
        #[serde(default)]
        style: Option<A2UIStyle>,
    },

    /// A text label or paragraph.
    Text {
        /// Text content to display.
        content: String,
        /// Optional styling.
        #[serde(default)]
        style: Option<A2UIStyle>,
    },

    /// A static image.
    Image {
        /// Image source URL or base64 data URI.
        src: String,
        /// Alt text for accessibility.
        #[serde(default)]
        alt: Option<String>,
        /// Optional styling.
        #[serde(default)]
        style: Option<A2UIStyle>,
    },

    /// A clickable button.
    Button {
        /// Button label text.
        label: String,
        /// Action identifier sent back on click.
        action: String,
        /// Optional styling.
        #[serde(default)]
        style: Option<A2UIStyle>,
    },

    /// A data visualization chart.
    Chart {
        /// Chart type: "bar", "line", "pie", "scatter".
        chart_type: String,
        /// Chart data as JSON.
        data: serde_json::Value,
        /// Optional styling.
        #[serde(default)]
        style: Option<A2UIStyle>,
    },

    /// A live video feed (Saorsa Canvas extension).
    VideoFeed {
        /// Stream identifier.
        stream_id: String,
        /// Whether to mirror the video.
        #[serde(default)]
        mirror: bool,
        /// Optional styling.
        #[serde(default)]
        style: Option<A2UIStyle>,
    },
}

fn default_layout() -> String {
    "vertical".to_string()
}

/// Result of converting an A2UI tree to canvas elements.
#[derive(Debug, Clone)]
pub struct ConversionResult {
    /// The converted elements.
    pub elements: Vec<Element>,
    /// Any warnings during conversion.
    pub warnings: Vec<String>,
}

impl A2UITree {
    /// Parse an A2UI tree from JSON string.
    ///
    /// # Errors
    ///
    /// Returns an error if the JSON is invalid or doesn't match the A2UI schema.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Convert this A2UI tree to Saorsa Canvas elements.
    ///
    /// The conversion applies automatic layout based on container layout types
    /// and respects style properties where applicable.
    #[must_use]
    pub fn to_elements(&self) -> ConversionResult {
        let mut converter = A2UIConverter::new();
        let elements = converter.convert_node(&self.root, 0.0, 0.0);
        ConversionResult {
            elements,
            warnings: converter.warnings,
        }
    }
}

/// Internal converter state.
struct A2UIConverter {
    /// Accumulated warnings.
    warnings: Vec<String>,
    /// Current z-index counter for layering.
    z_index: i32,
}

impl A2UIConverter {
    fn new() -> Self {
        Self {
            warnings: Vec::new(),
            z_index: 0,
        }
    }

    fn next_z_index(&mut self) -> i32 {
        let z = self.z_index;
        self.z_index += 1;
        z
    }

    fn convert_node(&mut self, node: &A2UINode, x: f32, y: f32) -> Vec<Element> {
        match node {
            A2UINode::Container {
                children,
                layout,
                style,
            } => self.convert_container(children, layout, style.as_ref(), x, y),

            A2UINode::Text { content, style } => {
                vec![self.convert_text(content, style.as_ref(), x, y)]
            }

            A2UINode::Image { src, style, .. } => {
                vec![self.convert_image(src, style.as_ref(), x, y)]
            }

            A2UINode::Button { label, style, .. } => {
                // Buttons are rendered as interactive text
                let mut element = self.convert_text(label, style.as_ref(), x, y);
                element.interactive = true;
                vec![element]
            }

            A2UINode::Chart {
                chart_type,
                data,
                style,
            } => {
                vec![self.convert_chart(chart_type, data, style.as_ref(), x, y)]
            }

            A2UINode::VideoFeed {
                stream_id,
                mirror,
                style,
            } => {
                vec![self.convert_video(stream_id, *mirror, style.as_ref(), x, y)]
            }
        }
    }

    fn convert_container(
        &mut self,
        children: &[A2UINode],
        layout: &str,
        style: Option<&A2UIStyle>,
        start_x: f32,
        start_y: f32,
    ) -> Vec<Element> {
        let mut elements = Vec::new();
        let mut current_x = start_x;
        let mut current_y = start_y;

        let padding = style.and_then(|s| s.padding).unwrap_or(10.0);
        let spacing = 10.0; // Default spacing between children

        current_x += padding;
        current_y += padding;

        for child in children {
            let child_elements = self.convert_node(child, current_x, current_y);

            // Calculate the bounding box of child elements for layout
            let (child_width, child_height) = Self::calculate_bounds(&child_elements);

            elements.extend(child_elements);

            match layout {
                "horizontal" => {
                    current_x += child_width + spacing;
                }
                _ => {
                    // Default to vertical layout
                    current_y += child_height + spacing;
                }
            }
        }

        elements
    }

    fn detect_image_format(src: &str) -> ImageFormat {
        use std::path::Path;
        let ext = Path::new(src)
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_lowercase);

        match ext.as_deref() {
            Some("jpg" | "jpeg") => ImageFormat::Jpeg,
            Some("svg") => ImageFormat::Svg,
            Some("webp") => ImageFormat::WebP,
            _ => ImageFormat::Png,
        }
    }

    fn calculate_bounds(elements: &[Element]) -> (f32, f32) {
        elements.iter().fold((0.0, 0.0), |(w, h), el| {
            (w.max(el.transform.width), h.max(el.transform.height))
        })
    }

    fn convert_text(
        &mut self,
        content: &str,
        style: Option<&A2UIStyle>,
        x: f32,
        y: f32,
    ) -> Element {
        let font_size = style.and_then(|s| s.font_size).unwrap_or(16.0);
        let color = style
            .and_then(|s| s.color.clone())
            .unwrap_or_else(|| "#000000".to_string());
        let width = style.and_then(|s| s.width).unwrap_or(200.0);
        let height = style.and_then(|s| s.height).unwrap_or(font_size * 1.5);

        Element::new(ElementKind::Text {
            content: content.to_string(),
            font_size,
            color,
        })
        .with_transform(Transform {
            x,
            y,
            width,
            height,
            rotation: 0.0,
            z_index: self.next_z_index(),
        })
    }

    fn convert_image(&mut self, src: &str, style: Option<&A2UIStyle>, x: f32, y: f32) -> Element {
        let width = style.and_then(|s| s.width).unwrap_or(200.0);
        let height = style.and_then(|s| s.height).unwrap_or(200.0);

        // Detect format from extension or default to PNG (case-insensitive)
        let format = Self::detect_image_format(src);

        Element::new(ElementKind::Image {
            src: src.to_string(),
            format,
        })
        .with_transform(Transform {
            x,
            y,
            width,
            height,
            rotation: 0.0,
            z_index: self.next_z_index(),
        })
    }

    fn convert_chart(
        &mut self,
        chart_type: &str,
        data: &serde_json::Value,
        style: Option<&A2UIStyle>,
        x: f32,
        y: f32,
    ) -> Element {
        let width = style.and_then(|s| s.width).unwrap_or(400.0);
        let height = style.and_then(|s| s.height).unwrap_or(300.0);

        Element::new(ElementKind::Chart {
            chart_type: chart_type.to_string(),
            data: data.clone(),
        })
        .with_transform(Transform {
            x,
            y,
            width,
            height,
            rotation: 0.0,
            z_index: self.next_z_index(),
        })
    }

    fn convert_video(
        &mut self,
        stream_id: &str,
        mirror: bool,
        style: Option<&A2UIStyle>,
        x: f32,
        y: f32,
    ) -> Element {
        let width = style.and_then(|s| s.width).unwrap_or(640.0);
        let height = style.and_then(|s| s.height).unwrap_or(480.0);

        Element::new(ElementKind::Video {
            stream_id: stream_id.to_string(),
            is_live: true,
            mirror,
            crop: None,
            media_config: None,
        })
        .with_transform(Transform {
            x,
            y,
            width,
            height,
            rotation: 0.0,
            z_index: self.next_z_index(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===========================================
    // TDD: Parsing Tests
    // ===========================================

    #[test]
    fn test_parse_simple_text_node() {
        let json = r#"{
            "root": {
                "component": "text",
                "content": "Hello World"
            }
        }"#;

        let tree = A2UITree::from_json(json).expect("should parse");

        match &tree.root {
            A2UINode::Text { content, style } => {
                assert_eq!(content, "Hello World");
                assert!(style.is_none());
            }
            _ => panic!("Expected Text node"),
        }
    }

    #[test]
    fn test_parse_text_with_style() {
        let json = r##"{
            "root": {
                "component": "text",
                "content": "Styled Text",
                "style": {
                    "font_size": 24.0,
                    "color": "#FF0000"
                }
            }
        }"##;

        let tree = A2UITree::from_json(json).expect("should parse");

        match &tree.root {
            A2UINode::Text { content, style } => {
                assert_eq!(content, "Styled Text");
                let s = style.as_ref().expect("should have style");
                assert_eq!(s.font_size, Some(24.0));
                assert_eq!(s.color.as_deref(), Some("#FF0000"));
            }
            _ => panic!("Expected Text node"),
        }
    }

    #[test]
    fn test_parse_container_with_children() {
        let json = r#"{
            "root": {
                "component": "container",
                "layout": "vertical",
                "children": [
                    { "component": "text", "content": "First" },
                    { "component": "text", "content": "Second" }
                ]
            }
        }"#;

        let tree = A2UITree::from_json(json).expect("should parse");

        match &tree.root {
            A2UINode::Container {
                children, layout, ..
            } => {
                assert_eq!(layout, "vertical");
                assert_eq!(children.len(), 2);
            }
            _ => panic!("Expected Container node"),
        }
    }

    #[test]
    fn test_parse_horizontal_container() {
        let json = r#"{
            "root": {
                "component": "container",
                "layout": "horizontal",
                "children": []
            }
        }"#;

        let tree = A2UITree::from_json(json).expect("should parse");

        match &tree.root {
            A2UINode::Container { layout, .. } => {
                assert_eq!(layout, "horizontal");
            }
            _ => panic!("Expected Container node"),
        }
    }

    #[test]
    fn test_parse_image() {
        let json = r#"{
            "root": {
                "component": "image",
                "src": "https://example.com/image.png",
                "alt": "Example image"
            }
        }"#;

        let tree = A2UITree::from_json(json).expect("should parse");

        match &tree.root {
            A2UINode::Image { src, alt, .. } => {
                assert_eq!(src, "https://example.com/image.png");
                assert_eq!(alt.as_deref(), Some("Example image"));
            }
            _ => panic!("Expected Image node"),
        }
    }

    #[test]
    fn test_parse_button() {
        let json = r#"{
            "root": {
                "component": "button",
                "label": "Click Me",
                "action": "submit_form"
            }
        }"#;

        let tree = A2UITree::from_json(json).expect("should parse");

        match &tree.root {
            A2UINode::Button { label, action, .. } => {
                assert_eq!(label, "Click Me");
                assert_eq!(action, "submit_form");
            }
            _ => panic!("Expected Button node"),
        }
    }

    #[test]
    fn test_parse_chart() {
        let json = r#"{
            "root": {
                "component": "chart",
                "chart_type": "bar",
                "data": {
                    "labels": ["A", "B", "C"],
                    "values": [10, 20, 15]
                }
            }
        }"#;

        let tree = A2UITree::from_json(json).expect("should parse");

        match &tree.root {
            A2UINode::Chart {
                chart_type, data, ..
            } => {
                assert_eq!(chart_type, "bar");
                assert!(data.get("labels").is_some());
                assert!(data.get("values").is_some());
            }
            _ => panic!("Expected Chart node"),
        }
    }

    #[test]
    fn test_parse_video_feed() {
        let json = r#"{
            "root": {
                "component": "video_feed",
                "stream_id": "local",
                "mirror": true
            }
        }"#;

        let tree = A2UITree::from_json(json).expect("should parse");

        match &tree.root {
            A2UINode::VideoFeed {
                stream_id, mirror, ..
            } => {
                assert_eq!(stream_id, "local");
                assert!(*mirror);
            }
            _ => panic!("Expected VideoFeed node"),
        }
    }

    #[test]
    fn test_parse_with_data_model() {
        let json = r#"{
            "root": {
                "component": "text",
                "content": "Data bound"
            },
            "data_model": {
                "user": "Alice",
                "count": 42
            }
        }"#;

        let tree = A2UITree::from_json(json).expect("should parse");

        assert_eq!(tree.data_model["user"], "Alice");
        assert_eq!(tree.data_model["count"], 42);
    }

    #[test]
    fn test_parse_invalid_json() {
        let json = "{ invalid json }";
        let result = A2UITree::from_json(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_missing_component_tag() {
        let json = r#"{
            "root": {
                "content": "No component tag"
            }
        }"#;

        let result = A2UITree::from_json(json);
        assert!(result.is_err());
    }

    // ===========================================
    // TDD: Conversion Tests
    // ===========================================

    #[test]
    fn test_convert_text_to_element() {
        let json = r#"{
            "root": {
                "component": "text",
                "content": "Hello World"
            }
        }"#;

        let tree = A2UITree::from_json(json).expect("should parse");
        let result = tree.to_elements();

        assert_eq!(result.elements.len(), 1);
        assert!(result.warnings.is_empty());

        match &result.elements[0].kind {
            ElementKind::Text {
                content, font_size, ..
            } => {
                assert_eq!(content, "Hello World");
                assert!((font_size - 16.0).abs() < f32::EPSILON); // Default font size
            }
            _ => panic!("Expected Text element"),
        }
    }

    #[test]
    fn test_convert_text_with_style() {
        let json = r##"{
            "root": {
                "component": "text",
                "content": "Styled",
                "style": {
                    "font_size": 32.0,
                    "color": "#00FF00"
                }
            }
        }"##;

        let tree = A2UITree::from_json(json).expect("should parse");
        let result = tree.to_elements();

        match &result.elements[0].kind {
            ElementKind::Text {
                font_size, color, ..
            } => {
                assert!((font_size - 32.0).abs() < f32::EPSILON);
                assert_eq!(color, "#00FF00");
            }
            _ => panic!("Expected Text element"),
        }
    }

    #[test]
    fn test_convert_container_vertical_layout() {
        let json = r#"{
            "root": {
                "component": "container",
                "layout": "vertical",
                "children": [
                    { "component": "text", "content": "First" },
                    { "component": "text", "content": "Second" }
                ]
            }
        }"#;

        let tree = A2UITree::from_json(json).expect("should parse");
        let result = tree.to_elements();

        assert_eq!(result.elements.len(), 2);

        // In vertical layout, Y positions should increase
        let y1 = result.elements[0].transform.y;
        let y2 = result.elements[1].transform.y;
        assert!(y2 > y1, "Second element should be below first");
    }

    #[test]
    fn test_convert_container_horizontal_layout() {
        let json = r#"{
            "root": {
                "component": "container",
                "layout": "horizontal",
                "children": [
                    { "component": "text", "content": "Left" },
                    { "component": "text", "content": "Right" }
                ]
            }
        }"#;

        let tree = A2UITree::from_json(json).expect("should parse");
        let result = tree.to_elements();

        assert_eq!(result.elements.len(), 2);

        // In horizontal layout, X positions should increase
        let x1 = result.elements[0].transform.x;
        let x2 = result.elements[1].transform.x;
        assert!(x2 > x1, "Second element should be right of first");
    }

    #[test]
    fn test_convert_image() {
        let json = r#"{
            "root": {
                "component": "image",
                "src": "test.png"
            }
        }"#;

        let tree = A2UITree::from_json(json).expect("should parse");
        let result = tree.to_elements();

        assert_eq!(result.elements.len(), 1);

        match &result.elements[0].kind {
            ElementKind::Image { src, format } => {
                assert_eq!(src, "test.png");
                assert_eq!(*format, ImageFormat::Png);
            }
            _ => panic!("Expected Image element"),
        }
    }

    #[test]
    fn test_convert_image_jpeg() {
        let json = r#"{
            "root": {
                "component": "image",
                "src": "photo.jpg"
            }
        }"#;

        let tree = A2UITree::from_json(json).expect("should parse");
        let result = tree.to_elements();

        match &result.elements[0].kind {
            ElementKind::Image { format, .. } => {
                assert_eq!(*format, ImageFormat::Jpeg);
            }
            _ => panic!("Expected Image element"),
        }
    }

    #[test]
    fn test_convert_button_is_interactive() {
        let json = r#"{
            "root": {
                "component": "button",
                "label": "Click Me",
                "action": "do_something"
            }
        }"#;

        let tree = A2UITree::from_json(json).expect("should parse");
        let result = tree.to_elements();

        assert_eq!(result.elements.len(), 1);
        assert!(
            result.elements[0].interactive,
            "Button should be interactive"
        );
    }

    #[test]
    fn test_convert_chart() {
        let json = r#"{
            "root": {
                "component": "chart",
                "chart_type": "pie",
                "data": { "values": [25, 50, 25] }
            }
        }"#;

        let tree = A2UITree::from_json(json).expect("should parse");
        let result = tree.to_elements();

        assert_eq!(result.elements.len(), 1);

        match &result.elements[0].kind {
            ElementKind::Chart {
                chart_type, data, ..
            } => {
                assert_eq!(chart_type, "pie");
                assert!(data.get("values").is_some());
            }
            _ => panic!("Expected Chart element"),
        }
    }

    #[test]
    fn test_convert_video_feed() {
        let json = r#"{
            "root": {
                "component": "video_feed",
                "stream_id": "camera_1",
                "mirror": true
            }
        }"#;

        let tree = A2UITree::from_json(json).expect("should parse");
        let result = tree.to_elements();

        assert_eq!(result.elements.len(), 1);

        match &result.elements[0].kind {
            ElementKind::Video {
                stream_id,
                is_live,
                mirror,
                ..
            } => {
                assert_eq!(stream_id, "camera_1");
                assert!(*is_live);
                assert!(*mirror);
            }
            _ => panic!("Expected Video element"),
        }
    }

    #[test]
    fn test_convert_nested_containers() {
        let json = r#"{
            "root": {
                "component": "container",
                "layout": "vertical",
                "children": [
                    {
                        "component": "container",
                        "layout": "horizontal",
                        "children": [
                            { "component": "text", "content": "A" },
                            { "component": "text", "content": "B" }
                        ]
                    },
                    { "component": "text", "content": "C" }
                ]
            }
        }"#;

        let tree = A2UITree::from_json(json).expect("should parse");
        let result = tree.to_elements();

        // Should have 3 text elements: A, B, C
        assert_eq!(result.elements.len(), 3);
    }

    #[test]
    fn test_convert_z_index_ordering() {
        let json = r#"{
            "root": {
                "component": "container",
                "layout": "vertical",
                "children": [
                    { "component": "text", "content": "First" },
                    { "component": "text", "content": "Second" },
                    { "component": "text", "content": "Third" }
                ]
            }
        }"#;

        let tree = A2UITree::from_json(json).expect("should parse");
        let result = tree.to_elements();

        // Z-indices should be in order
        let z0 = result.elements[0].transform.z_index;
        let z1 = result.elements[1].transform.z_index;
        let z2 = result.elements[2].transform.z_index;

        assert!(z1 > z0, "Second should be above first");
        assert!(z2 > z1, "Third should be above second");
    }

    #[test]
    fn test_convert_style_dimensions() {
        let json = r#"{
            "root": {
                "component": "image",
                "src": "test.png",
                "style": {
                    "width": 300.0,
                    "height": 150.0
                }
            }
        }"#;

        let tree = A2UITree::from_json(json).expect("should parse");
        let result = tree.to_elements();

        let transform = &result.elements[0].transform;
        assert!((transform.width - 300.0).abs() < f32::EPSILON);
        assert!((transform.height - 150.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_roundtrip_serialize_deserialize() {
        let original = A2UITree {
            root: A2UINode::Container {
                children: vec![
                    A2UINode::Text {
                        content: "Hello".to_string(),
                        style: None,
                    },
                    A2UINode::Button {
                        label: "Click".to_string(),
                        action: "submit".to_string(),
                        style: None,
                    },
                ],
                layout: "vertical".to_string(),
                style: None,
            },
            data_model: serde_json::json!({"key": "value"}),
        };

        let json = serde_json::to_string(&original).expect("should serialize");
        let parsed: A2UITree = serde_json::from_str(&json).expect("should deserialize");

        assert_eq!(original, parsed);
    }
}
