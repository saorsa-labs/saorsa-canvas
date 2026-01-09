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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
        /// Stream identifier (peer ID, "local", or media URL).
        stream_id: String,
        /// Whether this is a live WebRTC stream.
        is_live: bool,
        /// Whether to mirror the video (useful for local camera).
        mirror: bool,
        /// Optional crop region within the video frame.
        crop: Option<CropRect>,
    },

    /// A transparent overlay layer for annotations on top of video.
    OverlayLayer {
        /// Child element IDs drawn on this layer.
        children: Vec<ElementId>,
        /// Background opacity (0.0 = fully transparent).
        opacity: f32,
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

/// A crop rectangle for video frames.
/// Values are normalized (0.0 to 1.0) relative to the video dimensions.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CropRect {
    /// Left edge (0.0 = leftmost).
    pub x: f32,
    /// Top edge (0.0 = topmost).
    pub y: f32,
    /// Width (1.0 = full width).
    pub width: f32,
    /// Height (1.0 = full height).
    pub height: f32,
}

impl Default for CropRect {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            width: 1.0,
            height: 1.0,
        }
    }
}

impl CropRect {
    /// Create a crop rect that shows the full frame.
    #[must_use]
    pub fn full() -> Self {
        Self::default()
    }

    /// Create a centered square crop (useful for profile pictures).
    #[must_use]
    pub fn center_square() -> Self {
        Self {
            x: 0.25,
            y: 0.0,
            width: 0.5,
            height: 1.0,
        }
    }
}

/// Transform for positioning and sizing elements.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
