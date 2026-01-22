//! Voice Input Integration Tests
//!
//! Tests the complete voice input flow including:
//! - Touch + voice fusion (point-and-speak)
//! - Voice-only commands
//! - Fusion configuration
//! - Timeout handling

use canvas_core::{
    ElementId, FusionConfig, FusionResult, InputEvent, InputFusion, TouchEvent, TouchPhase,
    TouchPoint, VoiceEvent,
};
use std::time::Duration;

/// Create a touch start event at the given position.
fn touch_start(x: f32, y: f32, element_id: Option<ElementId>) -> TouchEvent {
    let mut event = TouchEvent::new(
        TouchPhase::Start,
        vec![TouchPoint {
            id: 0,
            x,
            y,
            pressure: Some(1.0),
            radius: None,
        }],
        1000,
    );
    event.target_element = element_id;
    event
}

/// Create a final voice event with the given transcript.
fn voice_final(transcript: &str, confidence: f32) -> VoiceEvent {
    VoiceEvent::final_result(transcript.to_string(), confidence, 2000)
}

/// Create an interim voice event.
fn voice_interim(transcript: &str) -> VoiceEvent {
    VoiceEvent::interim(transcript.to_string(), 0.8, 1500)
}

// ============================================================================
// Point-and-Speak Workflow Tests
// ============================================================================

#[test]
fn test_point_and_speak_fuses_touch_with_voice() {
    let mut fusion = InputFusion::new();
    let element_id = ElementId::new();

    // User touches an element
    let touch = touch_start(100.0, 200.0, Some(element_id));
    let result = fusion.process_touch(&touch);
    assert!(matches!(result, FusionResult::Pending));
    assert!(fusion.has_pending_touch());

    // User speaks "make this red" while touching
    let voice = voice_final("make this red", 0.95);
    let result = fusion.process_voice(&voice);

    // Should produce a fused intent
    match result {
        FusionResult::Fused(intent) => {
            assert_eq!(intent.transcript, "make this red");
            assert_eq!(intent.element_id, Some(element_id));
            assert!((intent.location.0 - 100.0).abs() < f32::EPSILON);
            assert!((intent.location.1 - 200.0).abs() < f32::EPSILON);
            assert!((intent.confidence - 0.95).abs() < f32::EPSILON);
        }
        _ => panic!("Expected Fused result, got {:?}", result),
    }

    // Touch should be consumed
    assert!(!fusion.has_pending_touch());
}

#[test]
fn test_point_and_speak_no_target_element() {
    let mut fusion = InputFusion::new();

    // Touch empty space (no element)
    let touch = touch_start(50.0, 50.0, None);
    let _ = fusion.process_touch(&touch);

    let voice = voice_final("create a box here", 0.9);
    let result = fusion.process_voice(&voice);

    match result {
        FusionResult::Fused(intent) => {
            assert_eq!(intent.transcript, "create a box here");
            assert!(intent.element_id.is_none());
            assert!((intent.location.0 - 50.0).abs() < f32::EPSILON);
            assert!((intent.location.1 - 50.0).abs() < f32::EPSILON);
        }
        _ => panic!("Expected Fused result"),
    }
}

#[test]
fn test_multiple_point_and_speak_operations() {
    let mut fusion = InputFusion::new();
    let element1 = ElementId::new();
    let element2 = ElementId::new();

    // First operation: touch element 1, speak
    let _ = fusion.process_touch(&touch_start(100.0, 100.0, Some(element1)));
    let result = fusion.process_voice(&voice_final("delete", 0.95));

    match result {
        FusionResult::Fused(intent) => {
            assert_eq!(intent.element_id, Some(element1));
        }
        _ => panic!("Expected first fused result"),
    }

    // Second operation: touch element 2, speak
    let _ = fusion.process_touch(&touch_start(200.0, 200.0, Some(element2)));
    let result = fusion.process_voice(&voice_final("move here", 0.9));

    match result {
        FusionResult::Fused(intent) => {
            assert_eq!(intent.element_id, Some(element2));
            assert!((intent.location.0 - 200.0).abs() < f32::EPSILON);
        }
        _ => panic!("Expected second fused result"),
    }
}

// ============================================================================
// Voice-Only Command Tests
// ============================================================================

#[test]
fn test_voice_only_without_touch() {
    let mut fusion = InputFusion::new();

    // Speak without any touch
    let voice = voice_final("undo", 0.98);
    let result = fusion.process_voice(&voice);

    match result {
        FusionResult::VoiceOnly(intent) => {
            assert_eq!(intent.transcript, "undo");
            assert!((intent.confidence - 0.98).abs() < f32::EPSILON);
        }
        _ => panic!("Expected VoiceOnly result"),
    }
}

#[test]
fn test_voice_only_global_commands() {
    let mut fusion = InputFusion::new();

    let commands = ["save", "zoom in", "show grid", "clear all"];

    for cmd in commands {
        let result = fusion.process_voice(&voice_final(cmd, 0.95));
        match result {
            FusionResult::VoiceOnly(intent) => {
                assert_eq!(intent.transcript, cmd);
            }
            _ => panic!("Expected VoiceOnly for command: {}", cmd),
        }
    }
}

// ============================================================================
// Interim Results Tests
// ============================================================================

#[test]
fn test_interim_results_ignored() {
    let mut fusion = InputFusion::new();

    // Touch first
    let _ = fusion.process_touch(&touch_start(100.0, 100.0, None));

    // Interim result should be ignored
    let interim = voice_interim("make this");
    let result = fusion.process_voice(&interim);
    assert!(matches!(result, FusionResult::None));

    // Touch should still be pending
    assert!(fusion.has_pending_touch());

    // Final result should fuse
    let final_result = voice_final("make this red", 0.95);
    let result = fusion.process_voice(&final_result);
    assert!(matches!(result, FusionResult::Fused(_)));
}

#[test]
fn test_multiple_interim_before_final() {
    let mut fusion = InputFusion::new();
    let _ = fusion.process_touch(&touch_start(50.0, 50.0, None));

    // Multiple interim results
    assert!(matches!(
        fusion.process_voice(&voice_interim("create")),
        FusionResult::None
    ));
    assert!(matches!(
        fusion.process_voice(&voice_interim("create a")),
        FusionResult::None
    ));
    assert!(matches!(
        fusion.process_voice(&voice_interim("create a box")),
        FusionResult::None
    ));

    // Final result
    let result = fusion.process_voice(&voice_final("create a box here", 0.92));
    match result {
        FusionResult::Fused(intent) => {
            assert_eq!(intent.transcript, "create a box here");
        }
        _ => panic!("Expected Fused result"),
    }
}

// ============================================================================
// Confidence Threshold Tests
// ============================================================================

#[test]
fn test_low_confidence_rejected() {
    let config = FusionConfig {
        fusion_window: Duration::from_millis(2000),
        min_confidence: 0.7,
    };
    let mut fusion = InputFusion::with_config(config);

    // Touch first
    let _ = fusion.process_touch(&touch_start(100.0, 100.0, None));

    // Low confidence voice should be rejected
    let voice = VoiceEvent::final_result("unclear".to_string(), 0.5, 2000);
    let result = fusion.process_voice(&voice);
    assert!(matches!(result, FusionResult::None));

    // Touch should still be pending
    assert!(fusion.has_pending_touch());
}

#[test]
fn test_confidence_at_threshold() {
    let config = FusionConfig {
        fusion_window: Duration::from_millis(2000),
        min_confidence: 0.7,
    };
    let mut fusion = InputFusion::with_config(config);

    let _ = fusion.process_touch(&touch_start(100.0, 100.0, None));

    // Exactly at threshold should pass
    let voice = VoiceEvent::final_result("command".to_string(), 0.7, 2000);
    let result = fusion.process_voice(&voice);
    assert!(matches!(result, FusionResult::Fused(_)));
}

#[test]
fn test_adjustable_confidence_threshold() {
    let mut fusion = InputFusion::new();

    // Update config with higher threshold
    fusion.set_config(FusionConfig {
        fusion_window: Duration::from_millis(2000),
        min_confidence: 0.9,
    });

    let _ = fusion.process_touch(&touch_start(100.0, 100.0, None));

    // 0.85 confidence below 0.9 threshold
    let voice = VoiceEvent::final_result("command".to_string(), 0.85, 2000);
    let result = fusion.process_voice(&voice);
    assert!(matches!(result, FusionResult::None));

    // 0.95 above threshold should work
    let voice = VoiceEvent::final_result("command".to_string(), 0.95, 2000);
    let result = fusion.process_voice(&voice);
    // Touch was consumed by time check, so this would be voice only
    match result {
        FusionResult::VoiceOnly(_) | FusionResult::Fused(_) => {}
        _ => panic!("Expected VoiceOnly or Fused"),
    }
}

// ============================================================================
// Touch Phase Tests
// ============================================================================

#[test]
fn test_only_touch_start_triggers_pending() {
    let mut fusion = InputFusion::new();

    // Move phase should not store pending touch
    let mut move_event = touch_start(100.0, 100.0, None);
    move_event.phase = TouchPhase::Move;
    let result = fusion.process_touch(&move_event);
    assert!(matches!(result, FusionResult::None));
    assert!(!fusion.has_pending_touch());

    // End phase should not store pending touch
    let mut end_event = touch_start(100.0, 100.0, None);
    end_event.phase = TouchPhase::End;
    let result = fusion.process_touch(&end_event);
    assert!(matches!(result, FusionResult::None));
    assert!(!fusion.has_pending_touch());

    // Start phase should store pending touch
    let start_event = touch_start(100.0, 100.0, None);
    let result = fusion.process_touch(&start_event);
    assert!(matches!(result, FusionResult::Pending));
    assert!(fusion.has_pending_touch());
}

#[test]
fn test_new_touch_replaces_pending() {
    let mut fusion = InputFusion::new();
    let element1 = ElementId::new();
    let element2 = ElementId::new();

    // First touch
    let _ = fusion.process_touch(&touch_start(100.0, 100.0, Some(element1)));
    assert!(fusion.has_pending_touch());

    // Second touch replaces first
    let _ = fusion.process_touch(&touch_start(200.0, 200.0, Some(element2)));
    assert!(fusion.has_pending_touch());

    // Voice should fuse with second touch
    let result = fusion.process_voice(&voice_final("select", 0.95));
    match result {
        FusionResult::Fused(intent) => {
            assert_eq!(intent.element_id, Some(element2));
            assert!((intent.location.0 - 200.0).abs() < f32::EPSILON);
        }
        _ => panic!("Expected Fused with second touch"),
    }
}

// ============================================================================
// InputEvent Processing Tests
// ============================================================================

#[test]
fn test_process_generic_input_event() {
    let mut fusion = InputFusion::new();
    let element_id = ElementId::new();

    // Process touch via InputEvent
    let touch = touch_start(100.0, 100.0, Some(element_id));
    let result = fusion.process(&InputEvent::Touch(touch));
    assert!(matches!(result, FusionResult::Pending));

    // Process voice via InputEvent
    let voice = voice_final("test", 0.9);
    let result = fusion.process(&InputEvent::Voice(voice));
    assert!(matches!(result, FusionResult::Fused(_)));
}

#[test]
fn test_process_ignores_other_events() {
    let mut fusion = InputFusion::new();

    // Key event should return None
    let key_event = InputEvent::Key {
        key: "a".to_string(),
        pressed: true,
        modifiers: canvas_core::event::KeyModifiers::default(),
    };
    let result = fusion.process(&key_event);
    assert!(matches!(result, FusionResult::None));

    // Pointer event should return None
    let pointer_event = InputEvent::Pointer {
        x: 100.0,
        y: 100.0,
        button: 0,
        pressed: true,
    };
    let result = fusion.process(&pointer_event);
    assert!(matches!(result, FusionResult::None));
}

// ============================================================================
// Utility Method Tests
// ============================================================================

#[test]
fn test_clear_pending_touch() {
    let mut fusion = InputFusion::new();

    let _ = fusion.process_touch(&touch_start(100.0, 100.0, None));
    assert!(fusion.has_pending_touch());

    fusion.clear_pending();
    assert!(!fusion.has_pending_touch());

    // Voice should now be voice-only
    let result = fusion.process_voice(&voice_final("test", 0.9));
    assert!(matches!(result, FusionResult::VoiceOnly(_)));
}

#[test]
fn test_time_remaining() {
    let config = FusionConfig {
        fusion_window: Duration::from_millis(2000),
        min_confidence: 0.5,
    };
    let mut fusion = InputFusion::with_config(config);

    // No pending touch
    assert!(fusion.time_remaining().is_none());

    // With pending touch
    let _ = fusion.process_touch(&touch_start(100.0, 100.0, None));
    let remaining = fusion.time_remaining();
    assert!(remaining.is_some());

    // Should be close to 2 seconds (within 100ms for test timing)
    let remaining_ms = remaining.unwrap().as_millis();
    assert!(remaining_ms > 1800);
    assert!(remaining_ms <= 2000);
}

#[test]
fn test_is_touch_valid() {
    let config = FusionConfig {
        fusion_window: Duration::from_millis(100), // Short window for test
        min_confidence: 0.5,
    };
    let mut fusion = InputFusion::with_config(config);

    let _ = fusion.process_touch(&touch_start(100.0, 100.0, None));
    assert!(fusion.is_touch_valid());

    // Wait for window to expire
    std::thread::sleep(Duration::from_millis(150));
    assert!(!fusion.is_touch_valid());

    // Voice after expiry should be voice-only
    let result = fusion.process_voice(&voice_final("test", 0.9));
    assert!(matches!(result, FusionResult::VoiceOnly(_)));
}

// ============================================================================
// Config Accessors Tests
// ============================================================================

#[test]
fn test_config_accessors() {
    let config = FusionConfig {
        fusion_window: Duration::from_millis(3000),
        min_confidence: 0.8,
    };
    let fusion = InputFusion::with_config(config);

    assert_eq!(fusion.config().fusion_window.as_millis(), 3000);
    assert!((fusion.config().min_confidence - 0.8).abs() < f32::EPSILON);
}

#[test]
fn test_default_config() {
    let fusion = InputFusion::default();

    // Default: 2 second window, 0.5 confidence
    assert_eq!(fusion.config().fusion_window.as_millis(), 2000);
    assert!((fusion.config().min_confidence - 0.5).abs() < f32::EPSILON);
}

// ============================================================================
// Edge Cases
// ============================================================================

#[test]
fn test_empty_transcript() {
    let mut fusion = InputFusion::new();

    // Empty transcript with high confidence
    let voice = VoiceEvent::final_result(String::new(), 0.95, 2000);
    let result = fusion.process_voice(&voice);

    // Should still work (application logic handles empty transcripts)
    match result {
        FusionResult::VoiceOnly(intent) => {
            assert!(intent.transcript.is_empty());
        }
        _ => panic!("Expected VoiceOnly"),
    }
}

#[test]
fn test_touch_at_origin() {
    let mut fusion = InputFusion::new();

    let _ = fusion.process_touch(&touch_start(0.0, 0.0, None));
    let result = fusion.process_voice(&voice_final("test", 0.9));

    match result {
        FusionResult::Fused(intent) => {
            assert!(intent.location.0.abs() < f32::EPSILON);
            assert!(intent.location.1.abs() < f32::EPSILON);
        }
        _ => panic!("Expected Fused"),
    }
}

#[test]
fn test_negative_coordinates() {
    let mut fusion = InputFusion::new();

    // Canvas coordinates can be negative in some scroll scenarios
    let _ = fusion.process_touch(&touch_start(-50.0, -100.0, None));
    let result = fusion.process_voice(&voice_final("test", 0.9));

    match result {
        FusionResult::Fused(intent) => {
            assert!((intent.location.0 - (-50.0)).abs() < f32::EPSILON);
            assert!((intent.location.1 - (-100.0)).abs() < f32::EPSILON);
        }
        _ => panic!("Expected Fused"),
    }
}

#[test]
fn test_very_long_transcript() {
    let mut fusion = InputFusion::new();

    let long_text = "a".repeat(1000);
    let voice = VoiceEvent::final_result(long_text.clone(), 0.9, 2000);
    let result = fusion.process_voice(&voice);

    match result {
        FusionResult::VoiceOnly(intent) => {
            assert_eq!(intent.transcript.len(), 1000);
        }
        _ => panic!("Expected VoiceOnly"),
    }
}
