//! WebAssembly bindings for canvas-core.
//!
//! This module provides JavaScript-callable functions when compiled to WASM.

use wasm_bindgen::prelude::*;

use crate::{CanvasState, Scene, SceneDocument};

/// Initialize the canvas WASM module.
#[wasm_bindgen(start)]
pub fn init() {
    // Set up panic hook for better error messages
    #[cfg(feature = "wasm")]
    console_error_panic_hook::set_once();
}

/// Canvas instance for WASM.
#[wasm_bindgen]
pub struct WasmCanvas {
    state: CanvasState,
    scene: Scene,
}

#[wasm_bindgen]
impl WasmCanvas {
    /// Create a new canvas instance.
    #[wasm_bindgen(constructor)]
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: CanvasState::default(),
            scene: Scene::default(),
        }
    }

    /// Get the current scene as JSON.
    #[wasm_bindgen(js_name = getSceneJson)]
    #[must_use]
    pub fn get_scene_json(&self) -> String {
        serde_json::to_string(&self.scene).unwrap_or_default()
    }

    /// Update the scene from JSON.
    ///
    /// # Errors
    ///
    /// Returns an error string if JSON parsing fails.
    #[wasm_bindgen(js_name = updateSceneFromJson)]
    pub fn update_scene_from_json(&mut self, json: &str) -> Result<(), String> {
        self.scene = serde_json::from_str(json).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Apply a canonical scene document serialized as JSON.
    ///
    /// # Errors
    ///
    /// Returns an error string if parsing or conversion fails.
    #[wasm_bindgen(js_name = applySceneDocument)]
    pub fn apply_scene_document(&mut self, json: &str) -> Result<(), String> {
        let document: SceneDocument = serde_json::from_str(json).map_err(|e| e.to_string())?;
        self.scene = document.into_scene()?;
        Ok(())
    }

    /// Check if the canvas is connected to an AI backend.
    #[wasm_bindgen(js_name = isConnected)]
    #[must_use]
    pub fn is_connected(&self) -> bool {
        self.state.is_connected()
    }

    /// Get the current connection status.
    #[wasm_bindgen(js_name = getConnectionStatus)]
    #[must_use]
    pub fn get_connection_status(&self) -> String {
        format!("{:?}", self.state.connection_status())
    }
}

impl Default for WasmCanvas {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wasm_canvas_new_creates_default_instance() {
        let canvas = WasmCanvas::new();
        assert!(!canvas.is_connected());
    }

    #[test]
    fn wasm_canvas_default_trait_works() {
        let canvas = WasmCanvas::default();
        assert!(!canvas.is_connected());
    }

    #[test]
    fn get_scene_json_returns_valid_json() {
        let canvas = WasmCanvas::new();
        let json = canvas.get_scene_json();
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(&json);
        assert!(parsed.is_ok(), "Scene JSON should be valid");
    }

    #[test]
    fn update_scene_from_json_accepts_valid_scene() {
        let mut canvas = WasmCanvas::new();
        let scene_json = r#"{"elements":{},"root_elements":[],"selected":[],"viewport_width":1024.0,"viewport_height":768.0,"zoom":1.5,"pan_x":10.0,"pan_y":20.0}"#;
        let result = canvas.update_scene_from_json(scene_json);
        assert!(result.is_ok());
        let updated_json = canvas.get_scene_json();
        assert!(updated_json.contains("1024"));
    }

    #[test]
    fn update_scene_from_json_rejects_invalid_json() {
        let mut canvas = WasmCanvas::new();
        let result = canvas.update_scene_from_json("{ not valid json }");
        assert!(result.is_err());
    }

    #[test]
    fn update_scene_from_json_rejects_wrong_structure() {
        let mut canvas = WasmCanvas::new();
        let result = canvas.update_scene_from_json(r#"{"foo": "bar"}"#);
        assert!(result.is_err());
    }

    #[test]
    fn apply_scene_document_accepts_valid_document() {
        let mut canvas = WasmCanvas::new();
        let doc_json = r#"{"session_id":"test","viewport":{"width":1920.0,"height":1080.0,"zoom":2.0,"pan_x":0.0,"pan_y":0.0},"elements":[],"timestamp":123}"#;
        let result = canvas.apply_scene_document(doc_json);
        assert!(result.is_ok());
        let scene_json = canvas.get_scene_json();
        assert!(scene_json.contains("1920"));
    }

    #[test]
    fn apply_scene_document_rejects_invalid_json() {
        let mut canvas = WasmCanvas::new();
        let result = canvas.apply_scene_document("not json");
        assert!(result.is_err());
    }

    #[test]
    fn apply_scene_document_rejects_invalid_element_id() {
        let mut canvas = WasmCanvas::new();
        let doc_json = r#"{"session_id":"test","viewport":{"width":800.0,"height":600.0},"elements":[{"id":"invalid-uuid","kind":{"type":"rectangle","width":100.0,"height":50.0}}],"timestamp":123}"#;
        let result = canvas.apply_scene_document(doc_json);
        assert!(result.is_err());
    }

    #[test]
    fn is_connected_returns_false_initially() {
        let canvas = WasmCanvas::new();
        assert!(!canvas.is_connected());
    }

    #[test]
    fn get_connection_status_returns_string() {
        let canvas = WasmCanvas::new();
        let status = canvas.get_connection_status();
        assert!(!status.is_empty());
        assert!(status.contains("Connecting"));
    }

    #[test]
    fn scene_json_roundtrip() {
        let canvas1 = WasmCanvas::new();
        let json1 = canvas1.get_scene_json();
        let mut canvas2 = WasmCanvas::new();
        let result = canvas2.update_scene_from_json(&json1);
        assert!(result.is_ok());
        let json2 = canvas2.get_scene_json();
        assert_eq!(json1, json2);
    }
}
