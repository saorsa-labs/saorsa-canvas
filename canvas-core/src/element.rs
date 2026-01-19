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

    /// Parse an element ID from a string.
    ///
    /// # Errors
    ///
    /// Returns an error if the string is not a valid UUID.
    pub fn parse(s: &str) -> Result<Self, uuid::Error> {
        Uuid::parse_str(s).map(Self)
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
        /// Optional media quality configuration.
        media_config: Option<MediaConfig>,
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

/// Video resolution presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Resolution {
    /// 426x240 (very low bandwidth).
    R240p,
    /// 640x360.
    R360p,
    /// 854x480.
    R480p,
    /// 1280x720 (default).
    #[default]
    R720p,
    /// 1920x1080.
    R1080p,
}

impl Resolution {
    /// Get width x height dimensions for this resolution.
    #[must_use]
    pub const fn dimensions(&self) -> (u32, u32) {
        match self {
            Self::R240p => (426, 240),
            Self::R360p => (640, 360),
            Self::R480p => (854, 480),
            Self::R720p => (1280, 720),
            Self::R1080p => (1920, 1080),
        }
    }

    /// Get suggested bitrate in kbps for this resolution.
    #[must_use]
    pub const fn suggested_bitrate_kbps(&self) -> u32 {
        match self {
            Self::R240p => 400,
            Self::R360p => 800,
            Self::R480p => 1200,
            Self::R720p => 2500,
            Self::R1080p => 5000,
        }
    }
}

/// Quality presets for automatic video configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QualityPreset {
    /// Automatic adaptation based on network conditions.
    #[default]
    Auto,
    /// Low bandwidth mode (240p-360p, ~400kbps).
    Low,
    /// Medium quality (480p, ~1200kbps).
    Medium,
    /// High quality (720p, ~2500kbps).
    High,
    /// Maximum quality (1080p, ~5000kbps).
    Ultra,
}

impl QualityPreset {
    /// Get the target resolution for this preset.
    #[must_use]
    pub const fn resolution(&self) -> Resolution {
        match self {
            Self::Auto | Self::High => Resolution::R720p,
            Self::Low => Resolution::R360p,
            Self::Medium => Resolution::R480p,
            Self::Ultra => Resolution::R1080p,
        }
    }

    /// Get the target bitrate in kbps for this preset.
    #[must_use]
    pub const fn bitrate_kbps(&self) -> u32 {
        match self {
            Self::Low => 400,
            Self::Medium => 1200,
            Self::Auto | Self::High => 2500,
            Self::Ultra => 5000,
        }
    }

    /// Get the target framerate for this preset.
    #[must_use]
    pub const fn framerate(&self) -> u8 {
        match self {
            Self::Low => 15,
            Self::Medium => 24,
            Self::Auto | Self::High | Self::Ultra => 30,
        }
    }
}

/// Configuration for video stream quality.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MediaConfig {
    /// Target bitrate in kbps (e.g., 1500 for 720p).
    pub bitrate_kbps: Option<u32>,
    /// Maximum resolution constraint.
    pub max_resolution: Option<Resolution>,
    /// Quality preset (overrides specific settings when not Auto).
    pub quality_preset: QualityPreset,
    /// Target framerate (default 30).
    pub target_fps: Option<u8>,
    /// Whether audio track is enabled.
    pub audio_enabled: bool,
}

impl Default for MediaConfig {
    fn default() -> Self {
        Self {
            bitrate_kbps: None,
            max_resolution: None,
            quality_preset: QualityPreset::Auto,
            target_fps: None,
            audio_enabled: false,
        }
    }
}

impl MediaConfig {
    /// Create a config from a quality preset.
    #[must_use]
    pub fn from_preset(preset: QualityPreset) -> Self {
        Self {
            bitrate_kbps: Some(preset.bitrate_kbps()),
            max_resolution: Some(preset.resolution()),
            quality_preset: preset,
            target_fps: Some(preset.framerate()),
            audio_enabled: false,
        }
    }

    /// Get the effective bitrate, considering preset.
    #[must_use]
    pub fn effective_bitrate_kbps(&self) -> u32 {
        self.bitrate_kbps
            .unwrap_or_else(|| self.quality_preset.bitrate_kbps())
    }

    /// Get the effective resolution, considering preset.
    #[must_use]
    pub fn effective_resolution(&self) -> Resolution {
        self.max_resolution
            .unwrap_or_else(|| self.quality_preset.resolution())
    }

    /// Get the effective framerate, considering preset.
    #[must_use]
    pub fn effective_fps(&self) -> u8 {
        self.target_fps
            .unwrap_or_else(|| self.quality_preset.framerate())
    }
}

/// Real-time media statistics from WebRTC.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct MediaStats {
    /// Round-trip time in milliseconds.
    pub rtt_ms: Option<f64>,
    /// Jitter in milliseconds.
    pub jitter_ms: Option<f64>,
    /// Packet loss percentage (0.0 - 100.0).
    pub packet_loss_percent: Option<f64>,
    /// Current framerate.
    pub fps: Option<f64>,
    /// Current bitrate in kbps.
    pub bitrate_kbps: Option<f64>,
    /// Timestamp of last update (unix milliseconds).
    pub timestamp_ms: u64,
}

impl MediaStats {
    /// Check if the connection quality is good based on stats.
    #[must_use]
    pub fn is_quality_good(&self) -> bool {
        let loss_ok = self.packet_loss_percent.is_none_or(|l| l < 2.0);
        let rtt_ok = self.rtt_ms.is_none_or(|r| r < 150.0);
        loss_ok && rtt_ok
    }

    /// Check if adaptive quality should downgrade.
    #[must_use]
    pub fn should_downgrade(&self) -> bool {
        let high_loss = self.packet_loss_percent.is_some_and(|l| l > 5.0);
        let high_rtt = self.rtt_ms.is_some_and(|r| r > 300.0);
        high_loss || high_rtt
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
