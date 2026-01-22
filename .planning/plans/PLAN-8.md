# Phase 8: Voice Input Bridge

## Overview
Capture speech input via Web Speech API, fuse with touch events within a time window, and send fused intents to AI via MCP. This enables "point and speak" interactions where users can touch an element while speaking about it.

## Technical Decisions
- Breakdown approach: By layer (Types → Logic → WASM → Web → Tests)
- Task size: Small (1 file, ~50-100 lines)
- Testing strategy: Unit tests + Integration tests
- Dependencies: Uses canvas-core events and canvas-server WebSocket infrastructure
- Pattern: Follow existing event handling patterns in canvas-core/event.rs

## Tasks

<task type="auto" priority="p1">
  <n>Task 1: Add VoiceEvent type to canvas-core</n>
  <files>
    canvas-core/src/event.rs
  </files>
  <depends></depends>
  <action>
    Add VoiceEvent to the event system for speech recognition results:

    ```rust
    /// A voice input event from speech recognition.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct VoiceEvent {
        /// The recognized speech transcript.
        pub transcript: String,
        /// Confidence score (0.0 to 1.0).
        pub confidence: f32,
        /// Whether this is a final (committed) result.
        pub is_final: bool,
        /// Timestamp when the speech started (ms since epoch).
        pub timestamp: u64,
    }

    impl VoiceEvent {
        /// Create a new voice event.
        #[must_use]
        pub fn new(transcript: String, confidence: f32, is_final: bool, timestamp: u64) -> Self;
    }
    ```

    Also update InputEvent enum to include Voice variant:
    ```rust
    pub enum InputEvent {
        Touch(TouchEvent),
        Pointer(PointerEvent),
        Voice(VoiceEvent),  // Add this
        Gesture(GestureEvent),
    }
    ```

    Requirements:
    - NO .unwrap() or .expect() in src/
    - Add rustdoc for all public items
    - Follow patterns from existing TouchEvent
  </action>
  <verify>
    cargo fmt --all -- --check
    cargo clippy -p canvas-core -- -D warnings
    cargo test -p canvas-core
  </verify>
  <done>
    - VoiceEvent struct exists with all fields
    - InputEvent::Voice variant added
    - Compiles without warnings
    - Existing tests still pass
  </done>
</task>

<task type="auto" priority="p1">
  <n>Task 2: Create FusedIntent enum and types</n>
  <files>
    canvas-core/src/fusion.rs
  </files>
  <depends>Task 1</depends>
  <action>
    Create a new module for input fusion with intent types:

    ```rust
    //! Input fusion for combining touch and voice events.

    use crate::event::{TouchEvent, VoiceEvent};
    use crate::ElementId;
    use serde::{Deserialize, Serialize};

    /// A fused intent combining touch and voice inputs.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub enum FusedIntent {
        /// Voice command with spatial context from touch.
        SpatialVoice {
            /// The voice transcript.
            transcript: String,
            /// The (x, y) location from touch.
            location: (f32, f32),
            /// The element that was touched (if any).
            element_id: Option<ElementId>,
        },
        /// Voice command without spatial context.
        VoiceOnly {
            /// The voice transcript.
            transcript: String,
        },
        /// Touch without voice context.
        TouchOnly {
            /// The touch location.
            location: (f32, f32),
            /// The element that was touched (if any).
            element_id: Option<ElementId>,
        },
    }

    /// Configuration for input fusion behavior.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct FusionConfig {
        /// Time window in milliseconds to fuse touch + voice.
        pub fusion_window_ms: u64,
    }

    impl Default for FusionConfig {
        fn default() -> Self {
            Self {
                fusion_window_ms: 2000, // 2 seconds
            }
        }
    }
    ```

    Also add `pub mod fusion;` to lib.rs.

    Requirements:
    - NO .unwrap() or .expect() in src/
    - Add rustdoc for all public items
    - Follow Serialize/Deserialize patterns
  </action>
  <verify>
    cargo fmt --all -- --check
    cargo clippy -p canvas-core -- -D warnings
    cargo test -p canvas-core
  </verify>
  <done>
    - FusedIntent enum with SpatialVoice, VoiceOnly, TouchOnly
    - FusionConfig struct with default
    - Module exported from lib.rs
    - Compiles without warnings
  </done>
</task>

<task type="auto" priority="p1">
  <n>Task 3: Implement InputFusion processor</n>
  <files>
    canvas-core/src/fusion.rs
  </files>
  <depends>Task 2</depends>
  <action>
    Add the InputFusion struct that processes events and produces fused intents:

    ```rust
    use std::time::Instant;

    /// Processor that fuses touch and voice events within a time window.
    #[derive(Debug)]
    pub struct InputFusion {
        /// Pending touch event waiting for voice.
        pending_touch: Option<(TouchEvent, Instant)>,
        /// Pending voice event waiting for touch.
        pending_voice: Option<(VoiceEvent, Instant)>,
        /// Configuration for fusion behavior.
        config: FusionConfig,
    }

    impl InputFusion {
        /// Create a new input fusion processor with default config.
        #[must_use]
        pub fn new() -> Self;

        /// Create with custom configuration.
        #[must_use]
        pub fn with_config(config: FusionConfig) -> Self;

        /// Process a touch event, potentially fusing with pending voice.
        pub fn process_touch(&mut self, touch: TouchEvent) -> Option<FusedIntent>;

        /// Process a voice event, potentially fusing with pending touch.
        pub fn process_voice(&mut self, voice: VoiceEvent) -> Option<FusedIntent>;

        /// Check for expired pending events and emit them as single-mode intents.
        pub fn flush_expired(&mut self) -> Vec<FusedIntent>;

        /// Clear all pending events.
        pub fn clear(&mut self);
    }

    impl Default for InputFusion {
        fn default() -> Self {
            Self::new()
        }
    }
    ```

    Fusion logic:
    - When touch arrives: check if pending voice within window → fuse or store touch
    - When voice (final) arrives: check if pending touch within window → fuse or store voice
    - flush_expired() returns any pending events past the window

    Requirements:
    - NO .unwrap() or .expect() in src/
    - Handle edge cases (no touch target, interim voice results)
    - Time comparison using Instant::elapsed()
  </action>
  <verify>
    cargo fmt --all -- --check
    cargo clippy -p canvas-core -- -D warnings
    cargo test -p canvas-core
  </verify>
  <done>
    - InputFusion struct with all methods
    - Touch + voice fusion works within time window
    - Expired events flush correctly
    - Compiles without warnings
  </done>
</task>

<task type="auto" priority="p1">
  <n>Task 4: Add WASM bindings for voice input</n>
  <files>
    canvas-app/src/lib.rs
  </files>
  <depends>Task 3</depends>
  <action>
    Add WASM-accessible methods for voice input processing:

    ```rust
    #[wasm_bindgen]
    impl CanvasApp {
        /// Process a voice recognition result.
        /// Returns JSON-encoded FusedIntent if fusion occurs, or null.
        #[wasm_bindgen(js_name = processVoice)]
        pub fn process_voice(
            &mut self,
            transcript: String,
            confidence: f32,
            is_final: bool,
            timestamp: f64,
        ) -> JsValue;

        /// Flush any expired pending inputs.
        /// Returns array of JSON-encoded FusedIntents.
        #[wasm_bindgen(js_name = flushPendingInputs)]
        pub fn flush_pending_inputs(&mut self) -> JsValue;

        /// Configure fusion time window in milliseconds.
        #[wasm_bindgen(js_name = setFusionWindow)]
        pub fn set_fusion_window(&mut self, window_ms: u32);
    }
    ```

    Also add InputFusion to CanvasApp struct:
    ```rust
    struct CanvasApp {
        // ... existing fields
        input_fusion: InputFusion,
    }
    ```

    Requirements:
    - Return JsValue::NULL when no fusion occurs
    - Use serde_wasm_bindgen for JSON conversion
    - Handle timestamp as f64 (JS number) → u64 conversion
  </action>
  <verify>
    cargo fmt --all -- --check
    cargo clippy -p canvas-app -- -D warnings
    cargo build -p canvas-app --target wasm32-unknown-unknown
  </verify>
  <done>
    - processVoice method exposed to JS
    - flushPendingInputs method exposed to JS
    - setFusionWindow method exposed to JS
    - WASM build succeeds
  </done>
</task>

<task type="auto" priority="p1">
  <n>Task 5: Create web/voice.js for Web Speech API</n>
  <files>
    web/voice.js
  </files>
  <depends>Task 4</depends>
  <action>
    Create JavaScript module for speech recognition:

    ```javascript
    /**
     * Voice input handler using Web Speech API.
     */
    export class VoiceInput {
        /**
         * @param {Object} canvasApp - The WASM CanvasApp instance
         * @param {Object} options - Configuration options
         */
        constructor(canvasApp, options = {}) {
            this.canvasApp = canvasApp;
            this.options = {
                continuous: true,
                interimResults: true,
                language: 'en-US',
                ...options
            };
            this.recognition = null;
            this.isListening = false;
        }

        /**
         * Check if speech recognition is supported.
         * @returns {boolean}
         */
        static isSupported() {
            return 'SpeechRecognition' in window ||
                   'webkitSpeechRecognition' in window;
        }

        /**
         * Initialize speech recognition.
         * @throws {Error} If not supported
         */
        init() {
            if (!VoiceInput.isSupported()) {
                throw new Error('Speech recognition not supported');
            }

            const SpeechRecognition = window.SpeechRecognition ||
                                      window.webkitSpeechRecognition;
            this.recognition = new SpeechRecognition();
            this.recognition.continuous = this.options.continuous;
            this.recognition.interimResults = this.options.interimResults;
            this.recognition.lang = this.options.language;

            this.recognition.onresult = (event) => this._handleResult(event);
            this.recognition.onerror = (event) => this._handleError(event);
            this.recognition.onend = () => this._handleEnd();
        }

        /**
         * Start listening for voice input.
         */
        start() {
            if (!this.recognition) this.init();
            this.recognition.start();
            this.isListening = true;
        }

        /**
         * Stop listening.
         */
        stop() {
            if (this.recognition) {
                this.recognition.stop();
            }
            this.isListening = false;
        }

        /**
         * Toggle listening state.
         * @returns {boolean} New listening state
         */
        toggle() {
            if (this.isListening) {
                this.stop();
            } else {
                this.start();
            }
            return this.isListening;
        }

        _handleResult(event) {
            const result = event.results[event.results.length - 1];
            const transcript = result[0].transcript;
            const confidence = result[0].confidence;
            const isFinal = result.isFinal;
            const timestamp = Date.now();

            const fusedIntent = this.canvasApp.processVoice(
                transcript,
                confidence,
                isFinal,
                timestamp
            );

            if (fusedIntent) {
                this._emitIntent(fusedIntent);
            }
        }

        _handleError(event) {
            console.error('Speech recognition error:', event.error);
            if (this.options.onError) {
                this.options.onError(event.error);
            }
        }

        _handleEnd() {
            if (this.isListening && this.options.continuous) {
                // Restart if we should still be listening
                this.recognition.start();
            }
        }

        _emitIntent(intent) {
            if (this.options.onIntent) {
                this.options.onIntent(intent);
            }
            // Dispatch custom event for other listeners
            window.dispatchEvent(new CustomEvent('fusedIntent', { detail: intent }));
        }
    }
    ```

    Requirements:
    - Handle browser prefix (webkit)
    - Auto-restart on end if continuous
    - Emit events for intents
    - Error handling
  </action>
  <verify>
    # Manual verification in browser
    # Check syntax: node --check web/voice.js (if using ESM compatible node)
  </verify>
  <done>
    - VoiceInput class created
    - isSupported() checks browser compatibility
    - start/stop/toggle methods work
    - Results passed to WASM processVoice
    - Events emitted on fusion
  </done>
</task>

<task type="auto" priority="p1">
  <n>Task 6: Unit tests for InputFusion</n>
  <files>
    canvas-core/src/fusion.rs
  </files>
  <depends>Task 3</depends>
  <action>
    Add comprehensive unit tests for fusion logic:

    ```rust
    #[cfg(test)]
    mod tests {
        use super::*;

        // Basic fusion tests
        #[test]
        fn test_voice_then_touch_fuses();
        #[test]
        fn test_touch_then_voice_fuses();
        #[test]
        fn test_voice_only_when_no_touch();
        #[test]
        fn test_touch_only_when_no_voice();

        // Time window tests
        #[test]
        fn test_fusion_within_window();
        #[test]
        fn test_no_fusion_outside_window();

        // Interim vs final voice
        #[test]
        fn test_interim_voice_does_not_fuse();
        #[test]
        fn test_final_voice_fuses();

        // Flush expired tests
        #[test]
        fn test_flush_expired_touch();
        #[test]
        fn test_flush_expired_voice();

        // Edge cases
        #[test]
        fn test_clear_pending();
        #[test]
        fn test_multiple_touches_only_latest();
        #[test]
        fn test_custom_fusion_window();
    }
    ```

    Requirements:
    - Cover all public methods
    - Test edge cases and timing
    - Use deterministic time simulation where possible
  </action>
  <verify>
    cargo fmt --all -- --check
    cargo clippy -p canvas-core -- -D warnings
    cargo test -p canvas-core -- fusion --nocapture
  </verify>
  <done>
    - At least 12 unit tests
    - All fusion scenarios covered
    - All tests pass
  </done>
</task>

<task type="auto" priority="p1">
  <n>Task 7: Integration test for voice flow</n>
  <files>
    canvas-core/tests/voice_integration.rs
  </files>
  <depends>Task 6</depends>
  <action>
    Create integration test verifying full voice input flow:

    ```rust
    //! Integration tests for voice input and fusion.

    use canvas_core::event::{TouchEvent, VoiceEvent, TouchPhase, Touch};
    use canvas_core::fusion::{InputFusion, FusedIntent, FusionConfig};

    #[test]
    fn test_point_and_speak_workflow() {
        // 1. User touches element
        // 2. User speaks "make this red"
        // 3. Fusion produces SpatialVoice with element_id
    }

    #[test]
    fn test_speak_then_point_workflow() {
        // 1. User says "delete"
        // 2. User touches element
        // 3. Fusion produces SpatialVoice with element_id
    }

    #[test]
    fn test_voice_command_without_touch() {
        // 1. User says "add a chart"
        // 2. No touch within window
        // 3. flush_expired produces VoiceOnly
    }

    #[test]
    fn test_rapid_fire_inputs() {
        // Simulate many rapid inputs
        // Verify each produces correct intent
    }

    #[test]
    fn test_fusion_config_respected() {
        // Use short window (500ms)
        // Verify fusion fails outside window
    }
    ```

    Requirements:
    - Test realistic user workflows
    - Verify correct FusedIntent variants produced
    - Test timing edge cases
  </action>
  <verify>
    cargo fmt --all -- --check
    cargo clippy -p canvas-core -- -D warnings
    cargo test -p canvas-core --test voice_integration
  </verify>
  <done>
    - At least 5 integration tests
    - Point-and-speak workflow tested
    - All tests pass
  </done>
</task>

## Exit Criteria
- [ ] All 7 tasks complete
- [ ] All tests passing
- [ ] Zero clippy warnings
- [ ] Code reviewed via /review
- [ ] Voice button in web UI toggles speech recognition
- [ ] Touch + voice fusion works within 2-second window
