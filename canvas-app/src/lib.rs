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

use std::{cell::RefCell, collections::HashMap, rc::Rc};

use canvas_core::{
    CanvasState, Element, ElementId, ElementKind, FusionConfig, FusionResult, InputEvent,
    InputFusion, Scene, SceneDocument, TouchEvent, TouchPhase, TouchPoint, Transform, VoiceEvent,
};
use canvas_renderer::{
    BackendType, Camera, HolographicConfig, HolographicRenderer, RenderBackend, RenderResult,
    Renderer, RendererConfig, Vec3,
};

// Chart rendering is not available in WASM - always use placeholder
// The chart module uses plotters which doesn't support wasm32
// WebGPU backend is not available in WASM builds (gpu feature disabled)

use wasm_bindgen::prelude::*;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, ImageData};

/// Initialize the WASM module.
#[wasm_bindgen(start)]
pub fn init_wasm() {
    console_error_panic_hook::set_once();
    tracing::info!("Saorsa Canvas WASM initialized");
}

/// Helper to set a property on a JS object with debug logging on failure.
///
/// In debug builds, logs a warning to the browser console if the property
/// cannot be set (e.g., if the object is frozen or the property is read-only).
fn js_set_property(obj: &js_sys::Object, key: &str, value: &JsValue) {
    if let Err(e) = js_sys::Reflect::set(obj, &JsValue::from_str(key), value) {
        // Log to browser console in debug mode
        #[cfg(debug_assertions)]
        web_sys::console::warn_2(
            &JsValue::from_str(&format!("Failed to set JS property '{key}': ")),
            &e,
        );
        // In release mode, we just ignore the error silently
        #[cfg(not(debug_assertions))]
        let _ = e;
    }
}

/// Cached video frame data.
struct VideoFrame {
    /// RGBA pixel data.
    data: Vec<u8>,
    /// Width in pixels.
    width: u32,
    /// Height in pixels.
    height: u32,
    /// Frame timestamp (for staleness detection).
    timestamp: f64,
}

type RendererHandle = Rc<RefCell<DomRendererState>>;

struct DomRendererState {
    canvas: HtmlCanvasElement,
    ctx: CanvasRenderingContext2d,
    width: u32,
    height: u32,
    background_color: String,
    video_frames: HashMap<String, VideoFrame>,
}

impl DomRendererState {
    fn new(canvas: HtmlCanvasElement, ctx: CanvasRenderingContext2d) -> Self {
        let width = canvas.width();
        let height = canvas.height();
        Self {
            canvas,
            ctx,
            width,
            height,
            background_color: "#ffffff".to_string(),
            video_frames: HashMap::new(),
        }
    }

    fn resize(&mut self, width: u32, height: u32) {
        self.canvas.set_width(width);
        self.canvas.set_height(height);
        self.width = width;
        self.height = height;
    }

    fn set_background_color(&mut self, color: &str) {
        self.background_color = color.to_string();
    }

    fn clear_dynamic_content(&mut self) {
        self.video_frames.clear();
    }
}

struct DomCanvasBackend {
    state: RendererHandle,
}

impl DomRendererState {
    fn render_scene(&mut self, scene: &Scene) {
        self.ctx.set_fill_style_str(&self.background_color);
        self.ctx
            .fill_rect(0.0, 0.0, f64::from(self.width), f64::from(self.height));

        let mut elements: Vec<_> = scene.elements().cloned().collect();
        elements.sort_by_key(|e| e.transform.z_index);

        for element in &elements {
            self.render_element(element);
        }
    }

    fn render_element(&mut self, element: &Element) {
        let t = &element.transform;

        if let ElementKind::Chart { chart_type, data } = &element.kind {
            self.render_chart(element, chart_type, data);
        } else if let ElementKind::Video { stream_id, .. } = &element.kind {
            self.render_video(element, stream_id);
        } else {
            let fill_color = Self::get_element_color(element);
            self.ctx.set_fill_style_str(&fill_color);
            self.ctx.fill_rect(
                f64::from(t.x),
                f64::from(t.y),
                f64::from(t.width),
                f64::from(t.height),
            );

            self.ctx.set_fill_style_str("#333333");
            self.ctx.set_font("12px sans-serif");
            let label = Self::get_element_label(element);
            let _ = self
                .ctx
                .fill_text(&label, f64::from(t.x) + 5.0, f64::from(t.y) + 15.0);
        }

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
    }

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn render_chart(&mut self, element: &Element, chart_type: &str, _data: &serde_json::Value) {
        // Chart rendering is not available in WASM (plotters doesn't support wasm32)
        // Always use placeholder rendering
        let t = &element.transform;
        self.draw_chart_placeholder(t, chart_type);
    }

    fn draw_chart_placeholder(&self, t: &Transform, chart_type: &str) {
        self.ctx.set_fill_style_str("#e3f2fd");
        self.ctx.fill_rect(
            f64::from(t.x),
            f64::from(t.y),
            f64::from(t.width),
            f64::from(t.height),
        );

        self.ctx.set_stroke_style_str("#90caf9");
        self.ctx.set_line_width(1.0);
        self.ctx.stroke_rect(
            f64::from(t.x),
            f64::from(t.y),
            f64::from(t.width),
            f64::from(t.height),
        );

        self.ctx.set_fill_style_str("#1976d2");
        self.ctx.set_font("14px sans-serif");
        let _ = self.ctx.fill_text(
            &format!("Chart: {chart_type}"),
            f64::from(t.x) + 10.0,
            f64::from(t.y) + 25.0,
        );

        self.ctx.set_fill_style_str("#bbdefb");
        let icon_x = f64::from(t.x) + f64::from(t.width) / 2.0 - 20.0;
        let icon_y = f64::from(t.y) + f64::from(t.height) / 2.0 - 10.0;
        self.ctx.fill_rect(icon_x, icon_y, 40.0, 20.0);
    }

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn render_video(&self, element: &Element, stream_id: &str) {
        let t = &element.transform;

        if let Some(frame) = self.video_frames.get(stream_id) {
            self.draw_video_frame(frame, t);
        } else {
            self.draw_video_placeholder(t, stream_id);
        }
    }

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn draw_video_frame(&self, frame: &VideoFrame, t: &Transform) {
        let clamped = wasm_bindgen::Clamped(&frame.data[..]);

        match ImageData::new_with_u8_clamped_array_and_sh(clamped, frame.width, frame.height) {
            Ok(image_data) => {
                if frame.width == t.width as u32 && frame.height == t.height as u32 {
                    if let Err(e) =
                        self.ctx
                            .put_image_data(&image_data, f64::from(t.x), f64::from(t.y))
                    {
                        tracing::warn!("Failed to draw video frame: {:?}", e);
                    }
                } else if let Some(window) = web_sys::window() {
                    if let Some(document) = window.document() {
                        if let Ok(temp_canvas) = document.create_element("canvas") {
                            if let Ok(temp_canvas) = temp_canvas.dyn_into::<HtmlCanvasElement>() {
                                temp_canvas.set_width(frame.width);
                                temp_canvas.set_height(frame.height);

                                if let Ok(Some(temp_ctx)) = temp_canvas.get_context("2d") {
                                    if let Ok(temp_ctx) =
                                        temp_ctx.dyn_into::<CanvasRenderingContext2d>()
                                    {
                                        let _ = temp_ctx.put_image_data(&image_data, 0.0, 0.0);
                                        let _ = self
                                            .ctx
                                            .draw_image_with_html_canvas_element_and_dw_and_dh(
                                                &temp_canvas,
                                                f64::from(t.x),
                                                f64::from(t.y),
                                                f64::from(t.width),
                                                f64::from(t.height),
                                            );
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Err(e) => tracing::warn!("Failed to create video ImageData: {:?}", e),
        }
    }

    fn draw_video_placeholder(&self, t: &Transform, stream_id: &str) {
        self.ctx.set_fill_style_str("#212121");
        self.ctx.fill_rect(
            f64::from(t.x),
            f64::from(t.y),
            f64::from(t.width),
            f64::from(t.height),
        );

        self.ctx.set_fill_style_str("#757575");
        self.ctx.set_font("14px sans-serif");
        self.ctx.set_text_align("center");
        self.ctx.set_text_baseline("middle");

        let center_x = f64::from(t.x) + f64::from(t.width) / 2.0;
        let center_y = f64::from(t.y) + f64::from(t.height) / 2.0;

        let _ = self
            .ctx
            .fill_text(&format!("Video: {stream_id}"), center_x, center_y - 10.0);
        let _ = self.ctx.fill_text("No signal", center_x, center_y + 10.0);

        self.ctx.set_text_align("start");
        self.ctx.set_text_baseline("alphabetic");
    }

    fn get_element_color(element: &Element) -> String {
        match &element.kind {
            ElementKind::Chart { .. } => "#e3f2fd".to_string(),
            ElementKind::Image { .. } => "#f5f5f5".to_string(),
            ElementKind::Model3D { .. } => "#e8f5e9".to_string(),
            ElementKind::Video { .. } => "#212121".to_string(),
            ElementKind::OverlayLayer { opacity, .. } => format!("rgba(255, 255, 255, {opacity})"),
            ElementKind::Text { color, .. } => color.clone(),
            ElementKind::Group { .. } => "rgba(255, 253, 231, 0.5)".to_string(),
        }
    }

    fn get_element_label(element: &Element) -> String {
        match &element.kind {
            ElementKind::Chart { chart_type, .. } => format!("Chart: {chart_type}"),
            ElementKind::Image { .. } => "Image".to_string(),
            ElementKind::Model3D { .. } => "3D Model".to_string(),
            ElementKind::Video { stream_id, .. } => format!("Video: {stream_id}"),
            ElementKind::OverlayLayer { children, .. } => format!("Overlay ({})", children.len()),
            ElementKind::Text { content, .. } => {
                if content.len() > 20 {
                    format!("{}...", &content[..20])
                } else {
                    content.clone()
                }
            }
            ElementKind::Group { children } => format!("Group ({})", children.len()),
        }
    }
}

impl RenderBackend for DomCanvasBackend {
    fn backend_type(&self) -> BackendType {
        BackendType::Canvas2D
    }

    fn render(&mut self, scene: &Scene) -> RenderResult<()> {
        if let Ok(mut state) = self.state.try_borrow_mut() {
            state.render_scene(scene);
        }
        Ok(())
    }

    fn resize(&mut self, width: u32, height: u32) -> RenderResult<()> {
        if let Ok(mut state) = self.state.try_borrow_mut() {
            state.resize(width, height);
        }
        Ok(())
    }
}

/// The main canvas application for WASM.
#[wasm_bindgen]
pub struct CanvasApp {
    scene: Scene,
    state: CanvasState,
    frame_count: u64,
    renderer_state: RendererHandle,
    renderer: Renderer,
    /// Holographic rendering configuration (None when not in holographic mode).
    holographic_config: Option<HolographicConfig>,
    /// Holographic renderer (lazily initialized).
    holographic_renderer: Option<HolographicRenderer>,
    /// Camera for holographic rendering.
    holographic_camera: Camera,
    /// Input fusion processor for touch+voice combination.
    input_fusion: InputFusion,
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

        let renderer_state = Rc::new(RefCell::new(DomRendererState::new(canvas, ctx)));

        // Use Canvas2D backend for WASM (WebGPU not available without gpu feature)
        let backend: Box<dyn RenderBackend> = Box::new(DomCanvasBackend {
            state: Rc::clone(&renderer_state),
        });

        let preferred_backend = backend.backend_type();
        let renderer = Renderer::with_backend(
            backend,
            RendererConfig {
                preferred_backend,
                ..RendererConfig::default()
            },
        );

        Ok(Self {
            scene,
            state: CanvasState::default(),
            frame_count: 0,
            renderer_state,
            renderer,
            holographic_config: None,
            holographic_renderer: None,
            holographic_camera: Camera::default(),
            input_fusion: InputFusion::new(),
        })
    }

    /// Render the current scene to the canvas.
    pub fn render(&mut self) {
        if let Err(err) = self.renderer.render(&self.scene) {
            tracing::error!("Renderer error: {:?}", err);
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

        // Create touch event with target element for fusion
        let mut touch_event = TouchEvent::new(touch_phase, vec![touch_point], 0);
        touch_event.target_element = element_id;
        let event = InputEvent::Touch(touch_event.clone());

        // Process through fusion system (only Start events are stored for fusion)
        let _ = self.input_fusion.process_touch(&touch_event);

        // Process the event in state
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
        if let Ok(mut state) = self.renderer_state.try_borrow_mut() {
            state.clear_dynamic_content();
        }
        Ok(())
    }

    /// Apply a canonical scene document serialized as JSON.
    ///
    /// # Errors
    ///
    /// Returns an error if the JSON is invalid or the scene cannot be converted.
    #[wasm_bindgen(js_name = applySceneDocument)]
    pub fn apply_scene_document(&mut self, json: &str) -> Result<(), JsValue> {
        let document: SceneDocument = serde_json::from_str(json)
            .map_err(|e| JsValue::from_str(&format!("Scene parse error: {e}")))?;
        self.scene = document
            .into_scene()
            .map_err(|e| JsValue::from_str(&format!("Scene conversion error: {e}")))?;
        if let Ok(mut state) = self.renderer_state.try_borrow_mut() {
            state.clear_dynamic_content();
        }
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
        if let Ok(mut state) = self.renderer_state.try_borrow_mut() {
            state.resize(width, height);
        }
        if let Err(err) = self.renderer.resize(width, height) {
            tracing::warn!("Renderer resize failed: {:?}", err);
        }
        self.scene.set_viewport(width as f32, height as f32);
    }

    /// Set the background color (CSS color string).
    #[wasm_bindgen(js_name = setBackgroundColor)]
    pub fn set_background_color(&mut self, color: &str) {
        if let Ok(mut state) = self.renderer_state.try_borrow_mut() {
            state.set_background_color(color);
        }
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

    /// The data should be RGBA bytes.
    #[wasm_bindgen(js_name = updateVideoFrame)]
    pub fn update_video_frame(
        &mut self,
        stream_id: &str,
        data: &[u8],
        width: u32,
        height: u32,
        timestamp: f64,
    ) {
        if let Ok(mut state) = self.renderer_state.try_borrow_mut() {
            state.video_frames.insert(
                stream_id.to_string(),
                VideoFrame {
                    data: data.to_vec(),
                    width,
                    height,
                    timestamp,
                },
            );
        }
    }

    /// Remove a video stream from the cache.
    #[wasm_bindgen(js_name = removeVideoStream)]
    pub fn remove_video_stream(&mut self, stream_id: &str) {
        if let Ok(mut state) = self.renderer_state.try_borrow_mut() {
            state.video_frames.remove(stream_id);
        }
    }

    /// Get the list of registered video stream IDs.
    #[wasm_bindgen(js_name = getVideoStreamIds)]
    #[must_use]
    pub fn get_video_stream_ids(&self) -> Vec<String> {
        self.renderer_state
            .try_borrow()
            .map(|state| state.video_frames.keys().cloned().collect())
            .unwrap_or_default()
    }

    /// Check if a video stream has a cached frame.
    #[wasm_bindgen(js_name = hasVideoFrame)]
    #[must_use]
    pub fn has_video_frame(&self, stream_id: &str) -> bool {
        self.renderer_state
            .try_borrow()
            .map(|state| state.video_frames.contains_key(stream_id))
            .unwrap_or(false)
    }

    /// Get the timestamp of the last frame for a video stream.
    /// Returns 0.0 if the stream doesn't exist.
    #[wasm_bindgen(js_name = getVideoFrameTimestamp)]
    #[must_use]
    pub fn get_video_frame_timestamp(&self, stream_id: &str) -> f64 {
        self.renderer_state
            .try_borrow()
            .ok()
            .and_then(|state| state.video_frames.get(stream_id).map(|f| f.timestamp))
            .unwrap_or(0.0)
    }

    // ========================================================================
    // Holographic Mode Methods
    // ========================================================================

    /// Enable holographic mode with a preset configuration.
    ///
    /// Supported presets: "portrait", "4k"
    /// Pass an empty string or "off" to disable holographic mode.
    ///
    /// # Errors
    ///
    /// Returns an error if the preset is not recognized.
    #[wasm_bindgen(js_name = setHolographicConfig)]
    pub fn set_holographic_config(&mut self, preset: &str) -> Result<(), JsValue> {
        match preset.to_lowercase().as_str() {
            "portrait" => {
                let config = HolographicConfig::looking_glass_portrait();
                self.holographic_renderer = Some(HolographicRenderer::new(config.clone()));
                self.holographic_config = Some(config);
                Ok(())
            }
            "4k" => {
                let config = HolographicConfig::looking_glass_4k();
                self.holographic_renderer = Some(HolographicRenderer::new(config.clone()));
                self.holographic_config = Some(config);
                Ok(())
            }
            "" | "off" | "none" | "disabled" => {
                self.holographic_config = None;
                self.holographic_renderer = None;
                Ok(())
            }
            _ => Err(JsValue::from_str(&format!(
                "Unknown holographic preset: '{preset}'. Use 'portrait', '4k', or 'off'"
            ))),
        }
    }

    /// Check if holographic mode is currently enabled.
    #[wasm_bindgen(js_name = isHolographicMode)]
    #[must_use]
    pub fn is_holographic_mode(&self) -> bool {
        self.holographic_config.is_some()
    }

    /// Get the quilt dimensions for the current holographic configuration.
    ///
    /// Returns a JS object: { width, height, views, columns, rows }
    /// Returns null if holographic mode is not enabled.
    #[wasm_bindgen(js_name = getQuiltDimensions)]
    #[must_use]
    pub fn get_quilt_dimensions(&self) -> JsValue {
        match &self.holographic_config {
            Some(config) => {
                let obj = js_sys::Object::new();
                js_set_property(
                    &obj,
                    "width",
                    &JsValue::from_f64(f64::from(config.quilt_width())),
                );
                js_set_property(
                    &obj,
                    "height",
                    &JsValue::from_f64(f64::from(config.quilt_height())),
                );
                js_set_property(
                    &obj,
                    "views",
                    &JsValue::from_f64(f64::from(config.num_views)),
                );
                js_set_property(
                    &obj,
                    "columns",
                    &JsValue::from_f64(f64::from(config.quilt_columns)),
                );
                js_set_property(
                    &obj,
                    "rows",
                    &JsValue::from_f64(f64::from(config.quilt_rows)),
                );
                js_set_property(
                    &obj,
                    "viewWidth",
                    &JsValue::from_f64(f64::from(config.view_width)),
                );
                js_set_property(
                    &obj,
                    "viewHeight",
                    &JsValue::from_f64(f64::from(config.view_height)),
                );
                obj.into()
            }
            None => JsValue::NULL,
        }
    }

    /// Render the current scene as a holographic quilt.
    ///
    /// Returns the quilt as RGBA pixel data (Vec<u8>).
    /// The dimensions can be obtained from `getQuiltDimensions()`.
    ///
    /// # Errors
    ///
    /// Returns an error if holographic mode is not enabled.
    #[wasm_bindgen(js_name = renderQuilt)]
    pub fn render_quilt(&mut self) -> Result<Vec<u8>, JsValue> {
        let renderer = self.holographic_renderer.as_mut().ok_or_else(|| {
            JsValue::from_str("Holographic mode not enabled. Call setHolographicConfig() first.")
        })?;

        let result = renderer.render_quilt(&self.scene, &self.holographic_camera);
        Ok(result.target.pixels)
    }

    /// Set the holographic camera position and target.
    ///
    /// # Arguments
    ///
    /// * `pos_x`, `pos_y`, `pos_z` - Camera position in world space
    /// * `target_x`, `target_y`, `target_z` - Point the camera looks at
    #[wasm_bindgen(js_name = setHolographicCamera)]
    #[allow(clippy::too_many_arguments)]
    pub fn set_holographic_camera(
        &mut self,
        pos_x: f32,
        pos_y: f32,
        pos_z: f32,
        target_x: f32,
        target_y: f32,
        target_z: f32,
    ) {
        self.holographic_camera = Camera {
            position: Vec3::new(pos_x, pos_y, pos_z),
            target: Vec3::new(target_x, target_y, target_z),
            ..Camera::default()
        };
    }

    /// Get the current holographic configuration preset name.
    ///
    /// Returns "portrait", "4k", or "none" if holographic mode is disabled.
    #[wasm_bindgen(js_name = getHolographicPreset)]
    #[must_use]
    pub fn get_holographic_preset(&self) -> String {
        match &self.holographic_config {
            Some(config) => {
                // Identify preset by num_views and view dimensions
                if config.num_views == 45 && config.view_width == 420 {
                    "portrait".to_string()
                } else if config.num_views == 45 && config.view_width == 819 {
                    "4k".to_string()
                } else {
                    "custom".to_string()
                }
            }
            None => "none".to_string(),
        }
    }

    /// Get information about a specific quilt view.
    ///
    /// Returns a JS object with view offset, dimensions, and camera position,
    /// or null if the view index is out of range or holographic mode is disabled.
    #[wasm_bindgen(js_name = getQuiltViewInfo)]
    #[must_use]
    pub fn get_quilt_view_info(&self, view_index: u32) -> JsValue {
        let Some(config) = &self.holographic_config else {
            return JsValue::NULL;
        };

        if view_index >= config.num_views {
            return JsValue::NULL;
        }

        let (x_offset, y_offset) = config.view_offset(view_index);
        let (col, row) = config.view_to_grid(view_index);

        let obj = js_sys::Object::new();
        js_set_property(&obj, "index", &JsValue::from_f64(f64::from(view_index)));
        js_set_property(&obj, "xOffset", &JsValue::from_f64(f64::from(x_offset)));
        js_set_property(&obj, "yOffset", &JsValue::from_f64(f64::from(y_offset)));
        js_set_property(
            &obj,
            "width",
            &JsValue::from_f64(f64::from(config.view_width)),
        );
        js_set_property(
            &obj,
            "height",
            &JsValue::from_f64(f64::from(config.view_height)),
        );
        js_set_property(&obj, "column", &JsValue::from_f64(f64::from(col)));
        js_set_property(&obj, "row", &JsValue::from_f64(f64::from(row)));

        obj.into()
    }

    /// Get statistics from the holographic renderer.
    ///
    /// Returns a JS object with: framesRendered, avgRenderTimeMs, peakRenderTimeMs, totalViewsRendered
    /// Returns null if holographic mode is not enabled.
    #[wasm_bindgen(js_name = getHolographicStats)]
    #[must_use]
    #[allow(clippy::cast_precision_loss)] // Stats counters unlikely to exceed 2^52
    pub fn get_holographic_stats(&self) -> JsValue {
        let Some(renderer) = &self.holographic_renderer else {
            return JsValue::NULL;
        };

        let stats = renderer.stats();
        let obj = js_sys::Object::new();
        js_set_property(
            &obj,
            "framesRendered",
            &JsValue::from_f64(stats.frames_rendered as f64),
        );
        js_set_property(
            &obj,
            "avgRenderTimeMs",
            &JsValue::from_f64(stats.avg_render_time_ms),
        );
        js_set_property(
            &obj,
            "peakRenderTimeMs",
            &JsValue::from_f64(stats.peak_render_time_ms),
        );
        js_set_property(
            &obj,
            "totalViewsRendered",
            &JsValue::from_f64(stats.total_views_rendered as f64),
        );

        obj.into()
    }

    /// Reset holographic rendering statistics.
    #[wasm_bindgen(js_name = resetHolographicStats)]
    pub fn reset_holographic_stats(&mut self) {
        if let Some(renderer) = &mut self.holographic_renderer {
            renderer.reset_stats();
        }
    }

    // =========================================================================
    // Voice Input Methods
    // =========================================================================

    /// Process a voice recognition result.
    ///
    /// This method handles speech recognition results from the Web Speech API.
    /// If a touch event is pending within the fusion window, it will create
    /// a fused intent combining the voice command with the touch location.
    ///
    /// # Arguments
    ///
    /// * `transcript` - The recognized speech text
    /// * `confidence` - Confidence score (0.0 to 1.0)
    /// * `is_final` - Whether this is a final (committed) result
    /// * `timestamp` - Timestamp when the speech was recognized (ms since epoch)
    ///
    /// # Returns
    ///
    /// JSON-encoded fusion result if fusion occurs, or null if no fusion.
    #[wasm_bindgen(js_name = processVoice)]
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub fn process_voice(
        &mut self,
        transcript: String,
        confidence: f32,
        is_final: bool,
        timestamp: f64,
    ) -> JsValue {
        let voice = VoiceEvent::new(transcript, confidence, is_final, timestamp as u64);
        let result = self.input_fusion.process_voice(&voice);

        // Also process the voice event through state
        self.state.process_event(&InputEvent::Voice(voice));

        match result {
            FusionResult::Fused(intent) => serde_json::to_string(&intent)
                .map(|s| JsValue::from_str(&s))
                .unwrap_or(JsValue::NULL),
            FusionResult::VoiceOnly(intent) => serde_json::to_string(&intent)
                .map(|s| JsValue::from_str(&s))
                .unwrap_or(JsValue::NULL),
            FusionResult::Pending | FusionResult::None => JsValue::NULL,
        }
    }

    /// Check if there's a pending touch waiting for voice fusion.
    ///
    /// Returns true if a touch event is stored and still within the fusion window.
    #[wasm_bindgen(js_name = hasPendingTouch)]
    #[must_use]
    pub fn has_pending_touch(&self) -> bool {
        self.input_fusion.is_touch_valid()
    }

    /// Get the remaining time in the fusion window for a pending touch.
    ///
    /// Returns the time in milliseconds, or 0 if no pending touch or expired.
    #[wasm_bindgen(js_name = fusionTimeRemaining)]
    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub fn fusion_time_remaining(&self) -> u32 {
        self.input_fusion
            .time_remaining()
            .map_or(0, |d| d.as_millis().min(u128::from(u32::MAX)) as u32)
    }

    /// Configure the fusion time window.
    ///
    /// Sets how long a touch event waits for a voice command before expiring.
    ///
    /// # Arguments
    ///
    /// * `window_ms` - Time window in milliseconds (default: 2000)
    #[wasm_bindgen(js_name = setFusionWindow)]
    pub fn set_fusion_window(&mut self, window_ms: u32) {
        use std::time::Duration;
        self.input_fusion.set_config(FusionConfig {
            fusion_window: Duration::from_millis(u64::from(window_ms)),
            ..self.input_fusion.config().clone()
        });
    }

    /// Clear any pending touch event.
    ///
    /// Call this to cancel touch+voice fusion if the user cancels the operation.
    #[wasm_bindgen(js_name = clearPendingTouch)]
    pub fn clear_pending_touch(&mut self) {
        self.input_fusion.clear_pending();
    }

    /// Get the current fusion window configuration in milliseconds.
    #[wasm_bindgen(js_name = getFusionWindow)]
    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub fn get_fusion_window(&self) -> u32 {
        self.input_fusion
            .config()
            .fusion_window
            .as_millis()
            .min(u128::from(u32::MAX)) as u32
    }

    /// Get the minimum confidence threshold for voice recognition.
    #[wasm_bindgen(js_name = getMinVoiceConfidence)]
    #[must_use]
    pub fn get_min_voice_confidence(&self) -> f32 {
        self.input_fusion.config().min_confidence
    }

    /// Set the minimum confidence threshold for voice recognition.
    ///
    /// Voice events with confidence below this threshold will be ignored.
    ///
    /// # Arguments
    ///
    /// * `confidence` - Minimum confidence (0.0 to 1.0, default: 0.5)
    #[wasm_bindgen(js_name = setMinVoiceConfidence)]
    pub fn set_min_voice_confidence(&mut self, confidence: f32) {
        self.input_fusion.set_config(FusionConfig {
            min_confidence: confidence.clamp(0.0, 1.0),
            ..self.input_fusion.config().clone()
        });
    }
}

/// Create a chart element JSON with sample data.
#[wasm_bindgen(js_name = createChartElement)]
#[must_use]
pub fn create_chart_element(chart_type: &str, x: f32, y: f32, width: f32, height: f32) -> String {
    // Provide sample data based on chart type
    let data = match chart_type {
        "pie" | "donut" => serde_json::json!({
            "series": [
                {"label": "Category A", "value": 35},
                {"label": "Category B", "value": 25},
                {"label": "Category C", "value": 20},
                {"label": "Category D", "value": 15},
                {"label": "Other", "value": 5}
            ]
        }),
        "scatter" => serde_json::json!({
            "series": [{
                "name": "Sample Data",
                "points": [
                    {"x": 10, "y": 20},
                    {"x": 25, "y": 40},
                    {"x": 40, "y": 35},
                    {"x": 55, "y": 60},
                    {"x": 70, "y": 50},
                    {"x": 85, "y": 75}
                ]
            }]
        }),
        _ => serde_json::json!({
            "series": [{
                "name": "Series 1",
                "points": [
                    {"x": "Jan", "y": 30},
                    {"x": "Feb", "y": 45},
                    {"x": "Mar", "y": 28},
                    {"x": "Apr", "y": 60},
                    {"x": "May", "y": 55},
                    {"x": "Jun", "y": 70}
                ]
            }],
            "x_label": "Month",
            "y_label": "Value"
        }),
    };

    let element = Element::new(ElementKind::Chart {
        chart_type: chart_type.to_string(),
        data,
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

/// Create a chart element JSON with custom data.
///
/// # Errors
///
/// Returns an error if the data JSON is invalid.
#[wasm_bindgen(js_name = createChartWithData)]
pub fn create_chart_with_data(
    chart_type: &str,
    data_json: &str,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
) -> Result<String, JsValue> {
    let data: serde_json::Value =
        serde_json::from_str(data_json).map_err(|e| JsValue::from_str(&e.to_string()))?;

    let element = Element::new(ElementKind::Chart {
        chart_type: chart_type.to_string(),
        data,
    })
    .with_transform(Transform {
        x,
        y,
        width,
        height,
        rotation: 0.0,
        z_index: 0,
    });

    serde_json::to_string(&element).map_err(|e| JsValue::from_str(&e.to_string()))
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

/// Create a video element JSON.
#[wasm_bindgen(js_name = createVideoElement)]
#[must_use]
pub fn create_video_element(
    stream_id: &str,
    is_live: bool,
    mirror: bool,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
) -> String {
    let element = Element::new(ElementKind::Video {
        stream_id: stream_id.to_string(),
        is_live,
        mirror,
        crop: None,
        media_config: None,
    })
    .with_transform(Transform {
        x,
        y,
        width,
        height,
        rotation: 0.0,
        z_index: 10, // Video on top by default
    });

    serde_json::to_string(&element).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_browser);

    // ============================================================================
    // Holographic Configuration Tests
    // ============================================================================

    #[wasm_bindgen_test]
    fn test_set_holographic_config_portrait() {
        let mut app = CanvasApp::new(800.0, 600.0);

        // Initially not in holographic mode
        assert!(!app.is_holographic_mode());

        // Set holographic config
        app.set_holographic_config("portrait".to_string());

        // Now should be in holographic mode
        assert!(app.is_holographic_mode());
    }

    #[wasm_bindgen_test]
    fn test_set_holographic_config_4k() {
        let mut app = CanvasApp::new(800.0, 600.0);

        app.set_holographic_config("4k".to_string());

        assert!(app.is_holographic_mode());
    }

    #[wasm_bindgen_test]
    fn test_set_holographic_config_8k() {
        let mut app = CanvasApp::new(800.0, 600.0);

        app.set_holographic_config("8k".to_string());

        assert!(app.is_holographic_mode());
    }

    #[wasm_bindgen_test]
    fn test_set_holographic_config_go() {
        let mut app = CanvasApp::new(800.0, 600.0);

        app.set_holographic_config("go".to_string());

        assert!(app.is_holographic_mode());
    }

    #[wasm_bindgen_test]
    fn test_set_holographic_config_unknown_defaults_to_portrait() {
        let mut app = CanvasApp::new(800.0, 600.0);

        // Unknown preset should default to portrait
        app.set_holographic_config("unknown_preset".to_string());

        assert!(app.is_holographic_mode());
    }

    // ============================================================================
    // Quilt Dimensions Tests
    // ============================================================================

    #[wasm_bindgen_test]
    fn test_get_quilt_dimensions_portrait() {
        let mut app = CanvasApp::new(800.0, 600.0);
        app.set_holographic_config("portrait".to_string());

        let dims = app.get_quilt_dimensions();

        // Portrait preset: 5 cols × 420px = 2100, 9 rows × 560px = 5040
        assert_eq!(js_sys::Reflect::get(&dims, &"width".into()).unwrap(), 2100);
        assert_eq!(js_sys::Reflect::get(&dims, &"height".into()).unwrap(), 5040);
    }

    #[wasm_bindgen_test]
    fn test_get_quilt_dimensions_4k() {
        let mut app = CanvasApp::new(800.0, 600.0);
        app.set_holographic_config("4k".to_string());

        let dims = app.get_quilt_dimensions();

        // 4K preset: 5 cols × 819px = 4095, 9 rows × 455px = 4095
        assert_eq!(js_sys::Reflect::get(&dims, &"width".into()).unwrap(), 4095);
        assert_eq!(js_sys::Reflect::get(&dims, &"height".into()).unwrap(), 4095);
    }

    #[wasm_bindgen_test]
    fn test_get_quilt_dimensions_not_in_holographic_mode() {
        let app = CanvasApp::new(800.0, 600.0);

        let dims = app.get_quilt_dimensions();

        // Not in holographic mode, should return 0x0
        assert_eq!(js_sys::Reflect::get(&dims, &"width".into()).unwrap(), 0);
        assert_eq!(js_sys::Reflect::get(&dims, &"height".into()).unwrap(), 0);
    }

    // ============================================================================
    // Holographic Camera Tests
    // ============================================================================

    #[wasm_bindgen_test]
    fn test_set_holographic_camera() {
        let mut app = CanvasApp::new(800.0, 600.0);
        app.set_holographic_config("portrait".to_string());

        // Set camera position and target
        app.set_holographic_camera(
            0.0, 0.0, 5.0, // position
            0.0, 0.0, 0.0, // target
            0.0, 1.0, 0.0, // up
        );

        // Should still be in holographic mode
        assert!(app.is_holographic_mode());
    }

    #[wasm_bindgen_test]
    fn test_set_holographic_camera_not_in_holographic_mode() {
        let mut app = CanvasApp::new(800.0, 600.0);

        // Set camera without holographic mode enabled (should be a no-op)
        app.set_holographic_camera(
            0.0, 0.0, 5.0, // position
            0.0, 0.0, 0.0, // target
            0.0, 1.0, 0.0, // up
        );

        // Should still not be in holographic mode
        assert!(!app.is_holographic_mode());
    }

    // ============================================================================
    // Holographic Preset Tests
    // ============================================================================

    #[wasm_bindgen_test]
    fn test_get_holographic_preset_portrait() {
        let mut app = CanvasApp::new(800.0, 600.0);
        app.set_holographic_config("portrait".to_string());

        let preset = app.get_holographic_preset();

        // Verify preset structure
        assert_eq!(
            js_sys::Reflect::get(&preset, &"numViews".into()).unwrap(),
            45
        );
        assert_eq!(
            js_sys::Reflect::get(&preset, &"quiltColumns".into()).unwrap(),
            5
        );
        assert_eq!(
            js_sys::Reflect::get(&preset, &"quiltRows".into()).unwrap(),
            9
        );
        assert_eq!(
            js_sys::Reflect::get(&preset, &"viewWidth".into()).unwrap(),
            420
        );
        assert_eq!(
            js_sys::Reflect::get(&preset, &"viewHeight".into()).unwrap(),
            560
        );
    }

    #[wasm_bindgen_test]
    fn test_get_holographic_preset_not_in_mode() {
        let app = CanvasApp::new(800.0, 600.0);

        let preset = app.get_holographic_preset();

        // Not in holographic mode, should return empty object
        assert!(js_sys::Reflect::get(&preset, &"numViews".into())
            .unwrap()
            .is_undefined());
    }

    // ============================================================================
    // Quilt View Info Tests
    // ============================================================================

    #[wasm_bindgen_test]
    fn test_get_quilt_view_info_valid_index() {
        let mut app = CanvasApp::new(800.0, 600.0);
        app.set_holographic_config("portrait".to_string());

        // Get first view (index 0)
        let view_info = app.get_quilt_view_info(0);

        // Verify view info structure
        assert_eq!(
            js_sys::Reflect::get(&view_info, &"index".into()).unwrap(),
            0
        );
        assert_eq!(
            js_sys::Reflect::get(&view_info, &"xOffset".into()).unwrap(),
            0
        );
        assert_eq!(
            js_sys::Reflect::get(&view_info, &"yOffset".into()).unwrap(),
            0
        );
        assert_eq!(
            js_sys::Reflect::get(&view_info, &"width".into()).unwrap(),
            420
        );
        assert_eq!(
            js_sys::Reflect::get(&view_info, &"height".into()).unwrap(),
            560
        );
    }

    #[wasm_bindgen_test]
    fn test_get_quilt_view_info_last_view() {
        let mut app = CanvasApp::new(800.0, 600.0);
        app.set_holographic_config("portrait".to_string());

        // Get last view (index 44 for portrait with 45 views)
        let view_info = app.get_quilt_view_info(44);

        assert_eq!(
            js_sys::Reflect::get(&view_info, &"index".into()).unwrap(),
            44
        );
    }

    #[wasm_bindgen_test]
    fn test_get_quilt_view_info_out_of_bounds() {
        let mut app = CanvasApp::new(800.0, 600.0);
        app.set_holographic_config("portrait".to_string());

        // Get view with invalid index (> 44)
        let view_info = app.get_quilt_view_info(100);

        // Out of bounds should return empty object
        assert!(js_sys::Reflect::get(&view_info, &"index".into())
            .unwrap()
            .is_undefined());
    }

    #[wasm_bindgen_test]
    fn test_get_quilt_view_info_not_in_holographic_mode() {
        let app = CanvasApp::new(800.0, 600.0);

        let view_info = app.get_quilt_view_info(0);

        // Not in holographic mode should return empty object
        assert!(js_sys::Reflect::get(&view_info, &"index".into())
            .unwrap()
            .is_undefined());
    }

    // ============================================================================
    // Holographic Stats Tests
    // ============================================================================

    #[wasm_bindgen_test]
    fn test_get_holographic_stats_initial() {
        let mut app = CanvasApp::new(800.0, 600.0);
        app.set_holographic_config("portrait".to_string());

        let stats = app.get_holographic_stats();

        // Initial stats should be zero
        assert_eq!(
            js_sys::Reflect::get(&stats, &"framesRendered".into()).unwrap(),
            0.0
        );
        assert_eq!(
            js_sys::Reflect::get(&stats, &"avgRenderTimeMs".into()).unwrap(),
            0.0
        );
        assert_eq!(
            js_sys::Reflect::get(&stats, &"peakRenderTimeMs".into()).unwrap(),
            0.0
        );
        assert_eq!(
            js_sys::Reflect::get(&stats, &"totalViewsRendered".into()).unwrap(),
            0.0
        );
    }

    #[wasm_bindgen_test]
    fn test_get_holographic_stats_not_in_mode() {
        let app = CanvasApp::new(800.0, 600.0);

        let stats = app.get_holographic_stats();

        // Not in holographic mode should still return valid object with zeros
        assert_eq!(
            js_sys::Reflect::get(&stats, &"framesRendered".into()).unwrap(),
            0.0
        );
    }

    #[wasm_bindgen_test]
    fn test_reset_holographic_stats() {
        let mut app = CanvasApp::new(800.0, 600.0);
        app.set_holographic_config("portrait".to_string());

        // Reset stats (should not panic even with no renders)
        app.reset_holographic_stats();

        let stats = app.get_holographic_stats();
        assert_eq!(
            js_sys::Reflect::get(&stats, &"framesRendered".into()).unwrap(),
            0.0
        );
    }

    #[wasm_bindgen_test]
    fn test_reset_holographic_stats_not_in_mode() {
        let mut app = CanvasApp::new(800.0, 600.0);

        // Reset stats without holographic mode (should be a no-op)
        app.reset_holographic_stats();

        // Should not panic
        assert!(!app.is_holographic_mode());
    }

    // ============================================================================
    // Render Quilt Tests
    // ============================================================================

    #[wasm_bindgen_test]
    fn test_render_quilt_returns_pixels() {
        let mut app = CanvasApp::new(800.0, 600.0);
        app.set_holographic_config("portrait".to_string());

        let result = app.render_quilt();

        // Should return a Uint8ClampedArray with pixel data
        // Portrait: 2100 × 5040 × 4 (RGBA) = 42,336,000 bytes
        let array = js_sys::Uint8ClampedArray::from(result);
        assert!(array.length() > 0);
    }

    #[wasm_bindgen_test]
    fn test_render_quilt_not_in_holographic_mode() {
        let mut app = CanvasApp::new(800.0, 600.0);

        let result = app.render_quilt();

        // Not in holographic mode should return empty array
        let array = js_sys::Uint8ClampedArray::from(result);
        assert_eq!(array.length(), 0);
    }

    #[wasm_bindgen_test]
    fn test_render_quilt_updates_stats() {
        let mut app = CanvasApp::new(800.0, 600.0);
        app.set_holographic_config("portrait".to_string());

        // Render once
        let _ = app.render_quilt();

        let stats = app.get_holographic_stats();
        let frames: f64 = js_sys::Reflect::get(&stats, &"framesRendered".into())
            .unwrap()
            .as_f64()
            .unwrap();
        assert_eq!(frames, 1.0);

        let views: f64 = js_sys::Reflect::get(&stats, &"totalViewsRendered".into())
            .unwrap()
            .as_f64()
            .unwrap();
        assert_eq!(views, 45.0); // Portrait has 45 views
    }

    #[wasm_bindgen_test]
    fn test_render_quilt_multiple_frames() {
        let mut app = CanvasApp::new(800.0, 600.0);
        app.set_holographic_config("portrait".to_string());

        // Render multiple times
        for _ in 0..3 {
            let _ = app.render_quilt();
        }

        let stats = app.get_holographic_stats();
        let frames: f64 = js_sys::Reflect::get(&stats, &"framesRendered".into())
            .unwrap()
            .as_f64()
            .unwrap();
        assert_eq!(frames, 3.0);

        let views: f64 = js_sys::Reflect::get(&stats, &"totalViewsRendered".into())
            .unwrap()
            .as_f64()
            .unwrap();
        assert_eq!(views, 135.0); // 45 views × 3 frames
    }
}
