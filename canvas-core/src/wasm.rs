//! WebAssembly bindings for canvas-core.
//!
//! This module provides JavaScript-callable functions when compiled to WASM.

use wasm_bindgen::prelude::*;

use crate::{CanvasState, Scene};

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
