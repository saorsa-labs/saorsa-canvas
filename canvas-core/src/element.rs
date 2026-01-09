//! Canvas elements - the building blocks of scenes.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Unique identifier for an element.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ElementId(Uuid);

impl ElementId {
    /// Create a new unique element ID.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Create from an existing UUID.
    #[must_use]
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl Default for ElementId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for ElementId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// The type of content an element contains.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum ElementKind {
    /// A 2D chart (bar, line, pie, etc.).
    Chart {
        /// Chart type identifier.
        chart_type: String,
        /// Chart data as JSON.
        data: serde_json::Value,
    },

    /// A 2D image (PNG, JPG, SVG).
    Image {
        /// Image source URI or base64 data.
        src: String,
        /// Image format.
        format: ImageFormat,
    },

    /// A 3D model (glTF).
    Model3D {
        /// glTF source URI.
        src: String,
        /// Initial rotation (euler angles in radians).
        rotation: [f32; 3],
        /// Initial scale.
        scale: f32,
    },

    /// A video stream or WebRTC feed.
    Video {
        /// Stream identifier.
        stream_id: String,
        /// Whether this is a live WebRTC stream.
        is_live: bool,
    },

    /// A text label or annotation.
    Text {
        /// Text content.
        content: String,
        /// Font size in pixels.
        font_size: f32,
        /// Text color as hex.
        color: String,
    },

    /// A container group for other elements.
    Group {
        /// Child element IDs.
        children: Vec<ElementId>,
    },
}

/// Supported image formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImageFormat {
    /// PNG image.
    Png,
    /// JPEG image.
    Jpeg,
    /// SVG vector image.
    Svg,
    /// WebP image.
    WebP,
}

/// Transform for positioning and sizing elements.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Transform {
    /// X position (pixels from left).
    pub x: f32,
    /// Y position (pixels from top).
    pub y: f32,
    /// Width in pixels.
    pub width: f32,
    /// Height in pixels.
    pub height: f32,
    /// Rotation in radians.
    pub rotation: f32,
    /// Z-index for layering.
    pub z_index: i32,
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 100.0,
            rotation: 0.0,
            z_index: 0,
        }
    }
}

/// A canvas element with content and transform.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Element {
    /// Unique identifier.
    pub id: ElementId,
    /// Element content type.
    pub kind: ElementKind,
    /// Position and size.
    pub transform: Transform,
    /// Whether this element is selected.
    pub selected: bool,
    /// Whether this element can be interacted with.
    pub interactive: bool,
    /// Optional parent element ID (for grouped elements).
    pub parent: Option<ElementId>,
}

impl Element {
    /// Create a new element with the given kind.
    #[must_use]
    pub fn new(kind: ElementKind) -> Self {
        Self {
            id: ElementId::new(),
            kind,
            transform: Transform::default(),
            selected: false,
            interactive: true,
            parent: None,
        }
    }

    /// Set the transform.
    #[must_use]
    pub fn with_transform(mut self, transform: Transform) -> Self {
        self.transform = transform;
        self
    }

    /// Set whether the element is interactive.
    #[must_use]
    pub fn with_interactive(mut self, interactive: bool) -> Self {
        self.interactive = interactive;
        self
    }

    /// Check if a point (in canvas coordinates) is within this element.
    #[must_use]
    pub fn contains_point(&self, x: f32, y: f32) -> bool {
        let t = &self.transform;
        x >= t.x && x <= t.x + t.width && y >= t.y && y <= t.y + t.height
    }
}
