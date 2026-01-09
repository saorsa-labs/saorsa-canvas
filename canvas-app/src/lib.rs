//! # Saorsa Canvas WASM Application
//!
//! This crate provides the WASM bindings for the Saorsa Canvas,
//! enabling the canvas to run in web browsers.
//!
//! ## Usage
//!
//! Build for WASM:
//! ```bash
//! wasm-pack build --target web canvas-app
//! ```
//!
//! Then import in JavaScript:
//! ```javascript
//! import init, { CanvasApp } from './pkg/canvas_app.js';
//!
//! await init();
//! const app = new CanvasApp('main-canvas');
//!
//! function render() {
//!     app.render();
//!     requestAnimationFrame(render);
//! }
//! render();
//! ```

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(clippy::all)]
#![deny(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]

use canvas_core::{
    CanvasState, Element, ElementId, ElementKind, InputEvent, Scene, TouchEvent, TouchPhase,
    TouchPoint, Transform,
};
use wasm_bindgen::prelude::*;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement};

/// Initialize the WASM module.
#[wasm_bindgen(start)]
pub fn init_wasm() {
    console_error_panic_hook::set_once();
    tracing::info!("Saorsa Canvas WASM initialized");
}

/// The main canvas application for WASM.
#[wasm_bindgen]
pub struct CanvasApp {
    canvas: HtmlCanvasElement,
    ctx: CanvasRenderingContext2d,
    scene: Scene,
    state: CanvasState,
    width: u32,
    height: u32,
    background_color: String,
    frame_count: u64,
}

#[wasm_bindgen]
impl CanvasApp {
    /// Create a new canvas application attached to the given canvas element ID.
    ///
    /// # Errors
    ///
    /// Returns an error if the canvas element is not found or 2D context fails.
    #[wasm_bindgen(constructor)]
    pub fn new(canvas_id: &str) -> Result<CanvasApp, JsValue> {
        let window = web_sys::window().ok_or_else(|| JsValue::from_str("No window object"))?;
        let document = window
            .document()
            .ok_or_else(|| JsValue::from_str("No document object"))?;

        let canvas = document
            .get_element_by_id(canvas_id)
            .ok_or_else(|| JsValue::from_str(&format!("Canvas element '{canvas_id}' not found")))?
            .dyn_into::<HtmlCanvasElement>()
            .map_err(|_| JsValue::from_str("Element is not a canvas"))?;

        let ctx = canvas
            .get_context("2d")
            .map_err(|_| JsValue::from_str("Failed to get 2D context"))?
            .ok_or_else(|| JsValue::from_str("2D context not available"))?
            .dyn_into::<CanvasRenderingContext2d>()
            .map_err(|_| JsValue::from_str("Failed to cast to 2D context"))?;

        let width = canvas.width();
        let height = canvas.height();

        #[allow(clippy::cast_precision_loss)]
        let scene = Scene::new(width as f32, height as f32);

        Ok(Self {
            canvas,
            ctx,
            scene,
            state: CanvasState::default(),
            width,
            height,
            background_color: "#ffffff".to_string(),
            frame_count: 0,
        })
    }

    /// Render the current scene to the canvas.
    pub fn render(&mut self) {
        // Clear canvas
        self.ctx.set_fill_style_str(&self.background_color);
        self.ctx
            .fill_rect(0.0, 0.0, f64::from(self.width), f64::from(self.height));

        // Render each element
        for element in self.scene.elements() {
            self.render_element(element);
        }

        self.frame_count += 1;
    }

    /// Handle a touch event at the given coordinates.
    #[wasm_bindgen(js_name = handleTouch)]
    pub fn handle_touch(&mut self, x: f32, y: f32, phase: &str) -> Option<String> {
        // Find element at touch location
        let element_id = self.scene.element_at(x, y);

        // Parse touch phase (default to Start for unknown phases)
        let touch_phase = match phase {
            "move" | "moved" => TouchPhase::Move,
            "end" | "ended" => TouchPhase::End,
            "cancel" | "cancelled" => TouchPhase::Cancel,
            _ => TouchPhase::Start,
        };

        // Create touch point
        let touch_point = TouchPoint {
            id: 0,
            x,
            y,
            pressure: None,
            radius: None,
        };

        // Create touch event
        let touch_event = TouchEvent::new(touch_phase, vec![touch_point], 0);
        let event = InputEvent::Touch(touch_event);

        // Process the event
        self.state.process_event(&event);

        // If an element was touched, select it
        if let Some(id) = element_id {
            self.select_element(&id);
            Some(id.to_string())
        } else {
            self.clear_selection();
            None
        }
    }

    /// Handle a mouse click at the given coordinates.
    #[wasm_bindgen(js_name = handleClick)]
    pub fn handle_click(&mut self, x: f32, y: f32) -> Option<String> {
        self.handle_touch(x, y, "start")
    }

    /// Add an element to the scene from JSON.
    ///
    /// # Errors
    ///
    /// Returns an error if JSON parsing fails.
    #[wasm_bindgen(js_name = addElement)]
    pub fn add_element(&mut self, json: &str) -> Result<String, JsValue> {
        let element: Element =
            serde_json::from_str(json).map_err(|e| JsValue::from_str(&e.to_string()))?;
        let id = element.id;
        self.scene.add_element(element);
        Ok(id.to_string())
    }

    /// Remove an element from the scene.
    ///
    /// # Errors
    ///
    /// Returns an error if the element is not found.
    #[wasm_bindgen(js_name = removeElement)]
    pub fn remove_element(&mut self, id: &str) -> Result<(), JsValue> {
        let uuid = uuid::Uuid::parse_str(id).map_err(|e| JsValue::from_str(&e.to_string()))?;
        let element_id = ElementId::from_uuid(uuid);
        self.scene
            .remove_element(&element_id)
            .map(|_| ())
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Get the current scene as JSON.
    #[wasm_bindgen(js_name = getSceneJson)]
    #[must_use]
    pub fn get_scene_json(&self) -> String {
        serde_json::to_string(&self.scene).unwrap_or_default()
    }

    /// Update the entire scene from JSON.
    ///
    /// # Errors
    ///
    /// Returns an error if JSON parsing fails.
    #[wasm_bindgen(js_name = setSceneJson)]
    pub fn set_scene_json(&mut self, json: &str) -> Result<(), JsValue> {
        self.scene = serde_json::from_str(json).map_err(|e| JsValue::from_str(&e.to_string()))?;
        Ok(())
    }

    /// Get the number of elements in the scene.
    #[wasm_bindgen(js_name = elementCount)]
    #[must_use]
    pub fn element_count(&self) -> usize {
        self.scene.element_count()
    }

    /// Get the current frame count.
    #[wasm_bindgen(js_name = frameCount)]
    #[must_use]
    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }

    /// Resize the canvas.
    #[allow(clippy::cast_precision_loss)]
    pub fn resize(&mut self, width: u32, height: u32) {
        self.canvas.set_width(width);
        self.canvas.set_height(height);
        self.width = width;
        self.height = height;
        self.scene.set_viewport(width as f32, height as f32);
    }

    /// Set the background color (CSS color string).
    #[wasm_bindgen(js_name = setBackgroundColor)]
    pub fn set_background_color(&mut self, color: &str) {
        self.background_color = color.to_string();
    }

    /// Check if connected to AI backend.
    #[wasm_bindgen(js_name = isConnected)]
    #[must_use]
    pub fn is_connected(&self) -> bool {
        self.state.is_connected()
    }

    /// Select an element by ID.
    fn select_element(&mut self, id: &ElementId) {
        // Clear previous selection
        for element in self.scene.elements_mut() {
            element.selected = element.id == *id;
        }
    }

    /// Clear all selections.
    fn clear_selection(&mut self) {
        for element in self.scene.elements_mut() {
            element.selected = false;
        }
    }

    /// Render a single element to the canvas.
    fn render_element(&self, element: &Element) {
        let t = &element.transform;

        // Set fill color based on element type
        let fill_color = Self::get_element_color(element);
        self.ctx.set_fill_style_str(&fill_color);

        // Draw the element as a rectangle (placeholder)
        self.ctx.fill_rect(
            f64::from(t.x),
            f64::from(t.y),
            f64::from(t.width),
            f64::from(t.height),
        );

        // Draw selection highlight if selected
        if element.selected {
            self.ctx.set_stroke_style_str("#0066ff");
            self.ctx.set_line_width(2.0);
            self.ctx.stroke_rect(
                f64::from(t.x),
                f64::from(t.y),
                f64::from(t.width),
                f64::from(t.height),
            );
        }

        // Draw element type label
        self.ctx.set_fill_style_str("#333333");
        self.ctx.set_font("12px sans-serif");
        let label = Self::get_element_label(element);
        let _ = self
            .ctx
            .fill_text(&label, f64::from(t.x) + 5.0, f64::from(t.y) + 15.0);
    }

    /// Get the display color for an element.
    fn get_element_color(element: &Element) -> String {
        match &element.kind {
            ElementKind::Chart { .. } => "#e3f2fd".to_string(), // Light blue
            ElementKind::Image { .. } => "#f5f5f5".to_string(), // Light gray
            ElementKind::Model3D { .. } => "#e8f5e9".to_string(), // Light green
            ElementKind::Video { .. } => "#212121".to_string(), // Dark gray
            ElementKind::Text { color, .. } => color.clone(),
            ElementKind::Group { .. } => "rgba(255, 253, 231, 0.5)".to_string(), // Transparent yellow
        }
    }

    /// Get a label for the element type.
    fn get_element_label(element: &Element) -> String {
        match &element.kind {
            ElementKind::Chart { chart_type, .. } => format!("Chart: {chart_type}"),
            ElementKind::Image { .. } => "Image".to_string(),
            ElementKind::Model3D { .. } => "3D Model".to_string(),
            ElementKind::Video { stream_id, .. } => format!("Video: {stream_id}"),
            ElementKind::Text { content, .. } => {
                let preview = if content.len() > 20 {
                    format!("{}...", &content[..20])
                } else {
                    content.clone()
                };
                preview
            }
            ElementKind::Group { children } => format!("Group ({})", children.len()),
        }
    }
}

/// Create a chart element JSON.
#[wasm_bindgen(js_name = createChartElement)]
#[must_use]
pub fn create_chart_element(chart_type: &str, x: f32, y: f32, width: f32, height: f32) -> String {
    let element = Element::new(ElementKind::Chart {
        chart_type: chart_type.to_string(),
        data: serde_json::json!({}),
    })
    .with_transform(Transform {
        x,
        y,
        width,
        height,
        rotation: 0.0,
        z_index: 0,
    });

    serde_json::to_string(&element).unwrap_or_default()
}

/// Create a text element JSON.
#[wasm_bindgen(js_name = createTextElement)]
#[must_use]
pub fn create_text_element(content: &str, x: f32, y: f32, font_size: f32, color: &str) -> String {
    let element = Element::new(ElementKind::Text {
        content: content.to_string(),
        font_size,
        color: color.to_string(),
    })
    .with_transform(Transform {
        x,
        y,
        width: 200.0, // Default width
        height: font_size * 1.5,
        rotation: 0.0,
        z_index: 0,
    });

    serde_json::to_string(&element).unwrap_or_default()
}

/// Create an image element JSON.
#[wasm_bindgen(js_name = createImageElement)]
#[must_use]
pub fn create_image_element(src: &str, x: f32, y: f32, width: f32, height: f32) -> String {
    let element = Element::new(ElementKind::Image {
        src: src.to_string(),
        format: canvas_core::ImageFormat::Png,
    })
    .with_transform(Transform {
        x,
        y,
        width,
        height,
        rotation: 0.0,
        z_index: 0,
    });

    serde_json::to_string(&element).unwrap_or_default()
}
