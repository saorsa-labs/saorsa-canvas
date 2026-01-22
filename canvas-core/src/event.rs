//! Input events for canvas interaction.

use serde::{Deserialize, Serialize};

use crate::ElementId;

/// Phase of a touch event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TouchPhase {
    /// Touch started (finger down).
    Start,
    /// Touch moved (finger dragging).
    Move,
    /// Touch ended (finger up).
    End,
    /// Touch cancelled (e.g., palm rejection).
    Cancel,
}

/// A single touch point.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TouchPoint {
    /// Touch identifier (for multi-touch).
    pub id: u32,
    /// X position in canvas coordinates.
    pub x: f32,
    /// Y position in canvas coordinates.
    pub y: f32,
    /// Pressure (0.0 to 1.0, if available).
    pub pressure: Option<f32>,
    /// Touch radius in pixels (if available).
    pub radius: Option<f32>,
}

/// A touch event with one or more touch points.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TouchEvent {
    /// Phase of this touch event.
    pub phase: TouchPhase,
    /// All current touch points.
    pub touches: Vec<TouchPoint>,
    /// Timestamp in milliseconds since canvas start.
    pub timestamp_ms: u64,
    /// Element ID that was touched (if any).
    pub target_element: Option<ElementId>,
}

impl TouchEvent {
    /// Create a new touch event.
    #[must_use]
    pub fn new(phase: TouchPhase, touches: Vec<TouchPoint>, timestamp_ms: u64) -> Self {
        Self {
            phase,
            touches,
            timestamp_ms,
            target_element: None,
        }
    }

    /// Get the primary (first) touch point.
    #[must_use]
    pub fn primary_touch(&self) -> Option<&TouchPoint> {
        self.touches.first()
    }

    /// Check if this is a multi-touch event.
    #[must_use]
    pub fn is_multi_touch(&self) -> bool {
        self.touches.len() > 1
    }
}

/// A voice input event from speech recognition.
///
/// Represents transcribed speech from Web Speech API or similar
/// speech recognition systems.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VoiceEvent {
    /// The recognized speech transcript.
    pub transcript: String,
    /// Confidence score (0.0 to 1.0).
    pub confidence: f32,
    /// Whether this is a final (committed) result.
    ///
    /// Interim results may change as speech recognition continues.
    /// Final results are stable and ready for processing.
    pub is_final: bool,
    /// Timestamp when the speech was recognized (ms since epoch).
    pub timestamp_ms: u64,
}

impl VoiceEvent {
    /// Create a new voice event.
    #[must_use]
    pub fn new(transcript: String, confidence: f32, is_final: bool, timestamp_ms: u64) -> Self {
        Self {
            transcript,
            confidence,
            is_final,
            timestamp_ms,
        }
    }

    /// Create an interim (non-final) voice event.
    #[must_use]
    pub fn interim(transcript: String, confidence: f32, timestamp_ms: u64) -> Self {
        Self::new(transcript, confidence, false, timestamp_ms)
    }

    /// Create a final voice event.
    #[must_use]
    pub fn final_result(transcript: String, confidence: f32, timestamp_ms: u64) -> Self {
        Self::new(transcript, confidence, true, timestamp_ms)
    }
}

/// Recognized gestures from touch input.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "gesture", content = "data")]
#[allow(missing_docs)] // Enum variant fields documented at variant level
pub enum Gesture {
    /// Single tap at a point (x, y coordinates).
    Tap {
        /// X coordinate.
        x: f32,
        /// Y coordinate.
        y: f32,
    },

    /// Double tap at a point (x, y coordinates).
    DoubleTap {
        /// X coordinate.
        x: f32,
        /// Y coordinate.
        y: f32,
    },

    /// Long press at a point with duration.
    LongPress {
        /// X coordinate.
        x: f32,
        /// Y coordinate.
        y: f32,
        /// Duration in milliseconds.
        duration_ms: u64,
    },

    /// Drag from one point to another.
    Drag {
        /// Starting X coordinate.
        start_x: f32,
        /// Starting Y coordinate.
        start_y: f32,
        /// Current X coordinate.
        current_x: f32,
        /// Current Y coordinate.
        current_y: f32,
        /// Delta X from last position.
        delta_x: f32,
        /// Delta Y from last position.
        delta_y: f32,
    },

    /// Pinch to zoom gesture.
    Pinch {
        /// Center X coordinate.
        center_x: f32,
        /// Center Y coordinate.
        center_y: f32,
        /// Scale factor (1.0 = no change).
        scale: f32,
    },

    /// Two-finger rotate gesture.
    Rotate {
        /// Center X coordinate.
        center_x: f32,
        /// Center Y coordinate.
        center_y: f32,
        /// Rotation angle in radians.
        angle_radians: f32,
    },
}

/// All input events the canvas can receive.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum InputEvent {
    /// Raw touch event.
    Touch(TouchEvent),

    /// Recognized gesture.
    Gesture(Gesture),

    /// Pointer (mouse) event.
    Pointer {
        /// X coordinate.
        x: f32,
        /// Y coordinate.
        y: f32,
        /// Mouse button (0 = left, 1 = middle, 2 = right).
        button: u8,
        /// Whether the button is pressed.
        pressed: bool,
    },

    /// Keyboard event.
    Key {
        /// Key name or code.
        key: String,
        /// Whether the key is pressed.
        pressed: bool,
        /// Active modifier keys.
        modifiers: KeyModifiers,
    },

    /// Voice input from speech recognition.
    Voice(VoiceEvent),
}

/// Keyboard modifiers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[allow(clippy::struct_excessive_bools)]
pub struct KeyModifiers {
    /// Shift key pressed.
    pub shift: bool,
    /// Control key pressed.
    pub ctrl: bool,
    /// Alt/Option key pressed.
    pub alt: bool,
    /// Meta/Command key pressed.
    pub meta: bool,
}
