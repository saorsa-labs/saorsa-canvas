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

use std::collections::HashMap;

use canvas_core::{
    CanvasState, Element, ElementId, ElementKind, InputEvent, Scene, TouchEvent, TouchPhase,
    TouchPoint, Transform,
};
use canvas_renderer::chart::{parse_chart_config, render_chart_to_buffer};
use wasm_bindgen::prelude::*;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, ImageData};

/// Initialize the WASM module.
#[wasm_bindgen(start)]
pub fn init_wasm() {
    console_error_panic_hook::set_once();
    tracing::info!("Saorsa Canvas WASM initialized");
}

/// Cached rendered chart data.
struct RenderedChart {
    /// RGBA pixel data.
    data: Vec<u8>,
    /// Width in pixels.
    width: u32,
    /// Height in pixels.
    height: u32,
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
    /// Cache for rendered charts (keyed by element ID).
    chart_cache: HashMap<ElementId, RenderedChart>,
    /// Cache for video frames (keyed by stream ID).
    video_frames: HashMap<String, VideoFrame>,
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
            chart_cache: HashMap::new(),
            video_frames: HashMap::new(),
        })
    }

    /// Render the current scene to the canvas.
    pub fn render(&mut self) {
        // Clear canvas
        self.ctx.set_fill_style_str(&self.background_color);
        self.ctx
            .fill_rect(0.0, 0.0, f64::from(self.width), f64::from(self.height));

        // Collect elements to render (to avoid borrow issues)
        let elements: Vec<_> = self.scene.elements().cloned().collect();

        // Render each element
        for element in &elements {
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
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn render_element(&mut self, element: &Element) {
        let t = &element.transform;

        // Handle chart rendering specially
        if let ElementKind::Chart { chart_type, data } = &element.kind {
            self.render_chart(element, chart_type, data);
        } else if let ElementKind::Video { stream_id, .. } = &element.kind {
            self.render_video(element, stream_id);
        } else {
            // Set fill color based on element type
            let fill_color = Self::get_element_color(element);
            self.ctx.set_fill_style_str(&fill_color);

            // Draw the element as a rectangle (placeholder for non-chart elements)
            self.ctx.fill_rect(
                f64::from(t.x),
                f64::from(t.y),
                f64::from(t.width),
                f64::from(t.height),
            );

            // Draw element type label for non-chart elements
            self.ctx.set_fill_style_str("#333333");
            self.ctx.set_font("12px sans-serif");
            let label = Self::get_element_label(element);
            let _ = self
                .ctx
                .fill_text(&label, f64::from(t.x) + 5.0, f64::from(t.y) + 15.0);
        }

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
    }

    /// Render a chart element using the chart rendering engine.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn render_chart(&mut self, element: &Element, chart_type: &str, data: &serde_json::Value) {
        let t = &element.transform;
        let width = t.width as u32;
        let height = t.height as u32;

        // Check if we have a cached render
        if let Some(cached) = self.chart_cache.get(&element.id) {
            // Use cached render if dimensions match
            if cached.width == width && cached.height == height {
                self.draw_rgba_buffer(&cached.data, t.x, t.y, cached.width, cached.height);
                return;
            }
        }

        // Parse chart config and render
        match parse_chart_config(chart_type, data, width, height) {
            Ok(config) => {
                match render_chart_to_buffer(&config) {
                    Ok(pixel_buffer) => {
                        // Convert RGB to RGBA
                        let canvas_buffer = Self::rgb_to_rgba(&pixel_buffer);

                        // Cache the rendered chart
                        self.chart_cache.insert(
                            element.id,
                            RenderedChart {
                                data: canvas_buffer.clone(),
                                width,
                                height,
                            },
                        );

                        // Draw to canvas
                        self.draw_rgba_buffer(&canvas_buffer, t.x, t.y, width, height);
                    }
                    Err(e) => {
                        // Fall back to placeholder on render error
                        tracing::warn!("Chart render error: {}", e);
                        self.draw_chart_placeholder(t, chart_type);
                    }
                }
            }
            Err(e) => {
                // Fall back to placeholder on parse error
                tracing::warn!("Chart config error: {}", e);
                self.draw_chart_placeholder(t, chart_type);
            }
        }
    }

    /// Convert RGB buffer to RGBA buffer.
    fn rgb_to_rgba(rgb: &[u8]) -> Vec<u8> {
        let pixel_count = rgb.len() / 3;
        let mut rgba = Vec::with_capacity(pixel_count * 4);

        for i in 0..pixel_count {
            rgba.push(rgb[i * 3]); // R
            rgba.push(rgb[i * 3 + 1]); // G
            rgba.push(rgb[i * 3 + 2]); // B
            rgba.push(255); // A (fully opaque)
        }

        rgba
    }

    /// Draw an RGBA buffer to the canvas at the specified position.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn draw_rgba_buffer(&self, data: &[u8], x: f32, y: f32, width: u32, height: u32) {
        // Create a clamped array from the RGBA data
        let clamped = wasm_bindgen::Clamped(data);

        // Create ImageData
        match ImageData::new_with_u8_clamped_array_and_sh(clamped, width, height) {
            Ok(image_data) => {
                // Draw to canvas
                if let Err(e) = self
                    .ctx
                    .put_image_data(&image_data, f64::from(x), f64::from(y))
                {
                    tracing::warn!("Failed to draw image data: {:?}", e);
                }
            }
            Err(e) => {
                tracing::warn!("Failed to create ImageData: {:?}", e);
            }
        }
    }

    /// Draw a placeholder for charts that fail to render.
    fn draw_chart_placeholder(&self, t: &Transform, chart_type: &str) {
        // Draw light blue background
        self.ctx.set_fill_style_str("#e3f2fd");
        self.ctx.fill_rect(
            f64::from(t.x),
            f64::from(t.y),
            f64::from(t.width),
            f64::from(t.height),
        );

        // Draw border
        self.ctx.set_stroke_style_str("#90caf9");
        self.ctx.set_line_width(1.0);
        self.ctx.stroke_rect(
            f64::from(t.x),
            f64::from(t.y),
            f64::from(t.width),
            f64::from(t.height),
        );

        // Draw label
        self.ctx.set_fill_style_str("#1976d2");
        self.ctx.set_font("14px sans-serif");
        let _ = self.ctx.fill_text(
            &format!("Chart: {chart_type}"),
            f64::from(t.x) + 10.0,
            f64::from(t.y) + 25.0,
        );

        // Draw icon placeholder
        self.ctx.set_fill_style_str("#bbdefb");
        let icon_x = f64::from(t.x) + f64::from(t.width) / 2.0 - 20.0;
        let icon_y = f64::from(t.y) + f64::from(t.height) / 2.0 - 10.0;
        self.ctx.fill_rect(icon_x, icon_y, 40.0, 20.0);
    }

    /// Render a video element.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn render_video(&self, element: &Element, stream_id: &str) {
        let t = &element.transform;

        // Check if we have a cached frame for this stream
        if let Some(frame) = self.video_frames.get(stream_id) {
            // Scale and draw the video frame to fit the element bounds
            self.draw_video_frame(frame, t);
        } else {
            // Draw placeholder if no frame available
            self.draw_video_placeholder(t, stream_id);
        }
    }

    /// Draw a video frame to the canvas, scaling to fit the element bounds.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn draw_video_frame(&self, frame: &VideoFrame, t: &Transform) {
        // Create ImageData from the frame
        let clamped = wasm_bindgen::Clamped(&frame.data[..]);

        match ImageData::new_with_u8_clamped_array_and_sh(clamped, frame.width, frame.height) {
            Ok(image_data) => {
                // For now, draw directly (scaling would require a temporary canvas)
                // If dimensions match, draw directly
                if frame.width == t.width as u32 && frame.height == t.height as u32 {
                    if let Err(e) =
                        self.ctx
                            .put_image_data(&image_data, f64::from(t.x), f64::from(t.y))
                    {
                        tracing::warn!("Failed to draw video frame: {:?}", e);
                    }
                } else {
                    // Create a temporary canvas for scaling
                    if let Some(window) = web_sys::window() {
                        if let Some(document) = window.document() {
                            if let Ok(temp_canvas) = document.create_element("canvas") {
                                if let Ok(temp_canvas) = temp_canvas.dyn_into::<HtmlCanvasElement>()
                                {
                                    temp_canvas.set_width(frame.width);
                                    temp_canvas.set_height(frame.height);

                                    if let Ok(Some(temp_ctx)) = temp_canvas.get_context("2d") {
                                        if let Ok(temp_ctx) =
                                            temp_ctx.dyn_into::<CanvasRenderingContext2d>()
                                        {
                                            // Draw frame to temp canvas
                                            let _ = temp_ctx.put_image_data(&image_data, 0.0, 0.0);

                                            // Draw scaled to main canvas
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
            }
            Err(e) => {
                tracing::warn!("Failed to create video ImageData: {:?}", e);
            }
        }
    }

    /// Draw a placeholder for video when no frame is available.
    fn draw_video_placeholder(&self, t: &Transform, stream_id: &str) {
        // Dark background
        self.ctx.set_fill_style_str("#212121");
        self.ctx.fill_rect(
            f64::from(t.x),
            f64::from(t.y),
            f64::from(t.width),
            f64::from(t.height),
        );

        // Center text
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

        // Reset text alignment
        self.ctx.set_text_align("start");
        self.ctx.set_text_baseline("alphabetic");
    }

    /// Update a video frame from JavaScript.
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
        self.video_frames.insert(
            stream_id.to_string(),
            VideoFrame {
                data: data.to_vec(),
                width,
                height,
                timestamp,
            },
        );
    }

    /// Remove a video stream from the cache.
    #[wasm_bindgen(js_name = removeVideoStream)]
    pub fn remove_video_stream(&mut self, stream_id: &str) {
        self.video_frames.remove(stream_id);
    }

    /// Get the list of registered video stream IDs.
    #[wasm_bindgen(js_name = getVideoStreamIds)]
    #[must_use]
    pub fn get_video_stream_ids(&self) -> Vec<String> {
        self.video_frames.keys().cloned().collect()
    }

    /// Check if a video stream has a cached frame.
    #[wasm_bindgen(js_name = hasVideoFrame)]
    #[must_use]
    pub fn has_video_frame(&self, stream_id: &str) -> bool {
        self.video_frames.contains_key(stream_id)
    }

    /// Get the timestamp of the last frame for a video stream.
    /// Returns 0.0 if the stream doesn't exist.
    #[wasm_bindgen(js_name = getVideoFrameTimestamp)]
    #[must_use]
    pub fn get_video_frame_timestamp(&self, stream_id: &str) -> f64 {
        self.video_frames
            .get(stream_id)
            .map_or(0.0, |f| f.timestamp)
    }

    /// Get the display color for an element.
    fn get_element_color(element: &Element) -> String {
        match &element.kind {
            ElementKind::Chart { .. } => "#e3f2fd".to_string(), // Light blue
            ElementKind::Image { .. } => "#f5f5f5".to_string(), // Light gray
            ElementKind::Model3D { .. } => "#e8f5e9".to_string(), // Light green
            ElementKind::Video { .. } => "#212121".to_string(), // Dark gray
            ElementKind::OverlayLayer { opacity, .. } => format!("rgba(255, 255, 255, {opacity})"),
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
            ElementKind::OverlayLayer { children, .. } => format!("Overlay ({})", children.len()),
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
