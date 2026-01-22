//! # Input Fusion
//!
//! Fuses touch and voice inputs into unified intents.
//!
//! When a user touches the canvas while speaking, both inputs are combined
//! to create a spatially-aware voice command. For example:
//!
//! ```text
//! User touches element X while saying "Make this red"
//!   → FusedIntent { element: X, command: "Make this red" }
//! ```

use crate::element::ElementId;
use crate::event::{InputEvent, TouchEvent, VoiceEvent};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

/// A fused intent combining touch and voice.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FusedIntent {
    /// The voice transcript.
    pub transcript: String,
    /// Touch location (x, y).
    pub location: (f32, f32),
    /// Target element if touch hit an element.
    pub element_id: Option<ElementId>,
    /// Confidence of the voice recognition.
    pub confidence: f32,
    /// Timestamp of the fusion.
    pub timestamp_ms: u64,
}

/// A voice-only intent (no touch context).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VoiceOnlyIntent {
    /// The voice transcript.
    pub transcript: String,
    /// Confidence of the voice recognition.
    pub confidence: f32,
    /// Timestamp.
    pub timestamp_ms: u64,
}

/// Result of processing an input event.
#[derive(Debug, Clone, PartialEq)]
pub enum FusionResult {
    /// Touch and voice were fused.
    Fused(FusedIntent),
    /// Voice-only command.
    VoiceOnly(VoiceOnlyIntent),
    /// Input was stored for potential fusion.
    Pending,
    /// No action needed.
    None,
}

/// Configuration for input fusion.
#[derive(Debug, Clone)]
pub struct FusionConfig {
    /// Time window for fusion (how long touch waits for voice).
    pub fusion_window: Duration,
    /// Minimum confidence for voice recognition.
    pub min_confidence: f32,
}

impl Default for FusionConfig {
    fn default() -> Self {
        Self {
            fusion_window: Duration::from_millis(2000),
            min_confidence: 0.5,
        }
    }
}

/// Input fusion processor.
///
/// Combines touch and voice inputs that occur within a configurable time window.
#[derive(Debug)]
pub struct InputFusion {
    /// Pending touch event waiting for voice.
    pending_touch: Option<PendingTouch>,
    /// Configuration.
    config: FusionConfig,
}

#[derive(Debug, Clone)]
struct PendingTouch {
    /// Touch location.
    location: (f32, f32),
    /// Target element.
    element_id: Option<ElementId>,
    /// When the touch occurred.
    timestamp: Instant,
}

impl InputFusion {
    /// Create a new input fusion processor with default config.
    #[must_use]
    pub fn new() -> Self {
        Self::with_config(FusionConfig::default())
    }

    /// Create with custom configuration.
    #[must_use]
    pub fn with_config(config: FusionConfig) -> Self {
        Self {
            pending_touch: None,
            config,
        }
    }

    /// Get the current configuration.
    #[must_use]
    pub const fn config(&self) -> &FusionConfig {
        &self.config
    }

    /// Update the configuration.
    pub fn set_config(&mut self, config: FusionConfig) {
        self.config = config;
    }

    /// Process a touch event.
    ///
    /// Stores the touch for potential fusion with upcoming voice.
    pub fn process_touch(&mut self, touch: &TouchEvent) -> FusionResult {
        // Only process touch start events
        if touch.phase != crate::event::TouchPhase::Start {
            return FusionResult::None;
        }

        // Get primary touch point
        let Some(point) = touch.primary_touch() else {
            return FusionResult::None;
        };

        // Store touch for potential fusion
        self.pending_touch = Some(PendingTouch {
            location: (point.x, point.y),
            element_id: touch.target_element,
            timestamp: Instant::now(),
        });

        FusionResult::Pending
    }

    /// Process a voice event.
    ///
    /// If a touch is pending within the fusion window, creates a fused intent.
    pub fn process_voice(&mut self, voice: &VoiceEvent) -> FusionResult {
        // Only process final transcriptions
        if !voice.is_final {
            return FusionResult::None;
        }

        // Check confidence threshold
        if voice.confidence < self.config.min_confidence {
            return FusionResult::None;
        }

        // Check for pending touch
        if let Some(pending) = self.pending_touch.take() {
            // Check if within fusion window
            if pending.timestamp.elapsed() <= self.config.fusion_window {
                return FusionResult::Fused(FusedIntent {
                    transcript: voice.transcript.clone(),
                    location: pending.location,
                    element_id: pending.element_id,
                    confidence: voice.confidence,
                    timestamp_ms: voice.timestamp_ms,
                });
            }
        }

        // Voice-only intent
        FusionResult::VoiceOnly(VoiceOnlyIntent {
            transcript: voice.transcript.clone(),
            confidence: voice.confidence,
            timestamp_ms: voice.timestamp_ms,
        })
    }

    /// Process any input event.
    pub fn process(&mut self, event: &InputEvent) -> FusionResult {
        match event {
            InputEvent::Touch(touch) => self.process_touch(touch),
            InputEvent::Voice(voice) => self.process_voice(voice),
            _ => FusionResult::None,
        }
    }

    /// Check if there's a pending touch.
    #[must_use]
    pub fn has_pending_touch(&self) -> bool {
        self.pending_touch.is_some()
    }

    /// Check if pending touch is still within fusion window.
    #[must_use]
    pub fn is_touch_valid(&self) -> bool {
        self.pending_touch
            .as_ref()
            .is_some_and(|p| p.timestamp.elapsed() <= self.config.fusion_window)
    }

    /// Clear any pending touch.
    pub fn clear_pending(&mut self) {
        self.pending_touch = None;
    }

    /// Get time remaining in fusion window for pending touch.
    #[must_use]
    pub fn time_remaining(&self) -> Option<Duration> {
        self.pending_touch.as_ref().and_then(|p| {
            let elapsed = p.timestamp.elapsed();
            self.config.fusion_window.checked_sub(elapsed)
        })
    }
}

impl Default for InputFusion {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{TouchPhase, TouchPoint};

    fn create_touch_event(x: f32, y: f32, element: Option<ElementId>) -> TouchEvent {
        TouchEvent {
            phase: TouchPhase::Start,
            touches: vec![TouchPoint {
                id: 0,
                x,
                y,
                pressure: Some(1.0),
                radius: None,
            }],
            timestamp_ms: 1000,
            target_element: element,
        }
    }

    fn create_voice_event(transcript: &str, is_final: bool) -> VoiceEvent {
        VoiceEvent {
            transcript: transcript.to_string(),
            confidence: 0.95,
            is_final,
            timestamp_ms: 2000,
        }
    }

    #[test]
    fn test_fusion_new() {
        let fusion = InputFusion::new();
        assert!(!fusion.has_pending_touch());
        assert!(!fusion.is_touch_valid());
    }

    #[test]
    fn test_fusion_config() {
        let config = FusionConfig {
            fusion_window: Duration::from_millis(3000),
            min_confidence: 0.7,
        };
        let fusion = InputFusion::with_config(config);
        assert_eq!(fusion.config().fusion_window.as_millis(), 3000);
    }

    #[test]
    fn test_process_touch_stores_pending() {
        let mut fusion = InputFusion::new();
        let touch = create_touch_event(100.0, 200.0, None);

        let result = fusion.process_touch(&touch);

        assert!(matches!(result, FusionResult::Pending));
        assert!(fusion.has_pending_touch());
        assert!(fusion.is_touch_valid());
    }

    #[test]
    fn test_process_touch_only_start_phase() {
        let mut fusion = InputFusion::new();
        let mut touch = create_touch_event(100.0, 200.0, None);
        touch.phase = TouchPhase::Move;

        let result = fusion.process_touch(&touch);

        assert!(matches!(result, FusionResult::None));
        assert!(!fusion.has_pending_touch());
    }

    #[test]
    fn test_voice_only_without_touch() {
        let mut fusion = InputFusion::new();
        let voice = create_voice_event("Make it red", true);

        let result = fusion.process_voice(&voice);

        match result {
            FusionResult::VoiceOnly(intent) => {
                assert_eq!(intent.transcript, "Make it red");
                assert!((intent.confidence - 0.95).abs() < f32::EPSILON);
            }
            _ => panic!("Expected VoiceOnly result"),
        }
    }

    #[test]
    fn test_voice_ignores_interim() {
        let mut fusion = InputFusion::new();
        let voice = create_voice_event("Make it", false);

        let result = fusion.process_voice(&voice);

        assert!(matches!(result, FusionResult::None));
    }

    #[test]
    fn test_voice_ignores_low_confidence() {
        let mut fusion = InputFusion::new();
        let mut voice = create_voice_event("Make it red", true);
        voice.confidence = 0.3;

        let result = fusion.process_voice(&voice);

        assert!(matches!(result, FusionResult::None));
    }

    #[test]
    fn test_fusion_touch_then_voice() {
        let mut fusion = InputFusion::new();
        let element_id = ElementId::new();
        let touch = create_touch_event(100.0, 200.0, Some(element_id));
        let voice = create_voice_event("Make this red", true);

        // Process touch first
        let _ = fusion.process_touch(&touch);
        assert!(fusion.has_pending_touch());

        // Then voice
        let result = fusion.process_voice(&voice);

        match result {
            FusionResult::Fused(intent) => {
                assert_eq!(intent.transcript, "Make this red");
                assert_eq!(intent.location, (100.0, 200.0));
                assert_eq!(intent.element_id, Some(element_id));
            }
            _ => panic!("Expected Fused result"),
        }

        // Touch should be consumed
        assert!(!fusion.has_pending_touch());
    }

    #[test]
    fn test_fusion_clears_pending() {
        let mut fusion = InputFusion::new();
        let touch = create_touch_event(100.0, 200.0, None);

        let _ = fusion.process_touch(&touch);
        assert!(fusion.has_pending_touch());

        fusion.clear_pending();
        assert!(!fusion.has_pending_touch());
    }

    #[test]
    fn test_time_remaining() {
        let mut fusion = InputFusion::new();
        let touch = create_touch_event(100.0, 200.0, None);

        let _ = fusion.process_touch(&touch);

        let remaining = fusion.time_remaining();
        assert!(remaining.is_some());
        assert!(remaining.unwrap() > Duration::from_millis(1900)); // Should be close to 2s
    }

    #[test]
    fn test_time_remaining_none_without_pending() {
        let fusion = InputFusion::new();
        assert!(fusion.time_remaining().is_none());
    }

    #[test]
    fn test_default_impl() {
        let fusion = InputFusion::default();
        assert!(!fusion.has_pending_touch());
    }

    #[test]
    fn test_set_config() {
        let mut fusion = InputFusion::new();
        fusion.set_config(FusionConfig {
            fusion_window: Duration::from_millis(5000),
            min_confidence: 0.8,
        });
        assert_eq!(fusion.config().fusion_window.as_millis(), 5000);
    }

    #[test]
    fn test_fused_intent_fields() {
        let intent = FusedIntent {
            transcript: "test".to_string(),
            location: (10.0, 20.0),
            element_id: None,
            confidence: 0.9,
            timestamp_ms: 1234,
        };
        assert_eq!(intent.transcript, "test");
        assert!((intent.location.0 - 10.0).abs() < f32::EPSILON);
        assert!((intent.confidence - 0.9).abs() < f32::EPSILON);
    }

    #[test]
    fn test_voice_only_intent_fields() {
        let intent = VoiceOnlyIntent {
            transcript: "undo".to_string(),
            confidence: 0.85,
            timestamp_ms: 5678,
        };
        assert_eq!(intent.transcript, "undo");
        assert_eq!(intent.timestamp_ms, 5678);
    }

    #[test]
    fn test_voice_event_fields() {
        let voice = VoiceEvent {
            transcript: "hello".to_string(),
            confidence: 0.99,
            is_final: true,
            timestamp_ms: 9999,
        };
        assert!(voice.is_final);
        assert_eq!(voice.transcript, "hello");
    }
}
