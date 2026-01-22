//! MCP server implementation for Saorsa Canvas.
//!
//! Implements JSON-RPC 2.0 protocol for MCP tool calls and resource access.

use std::collections::HashMap;
use std::sync::Arc;

use canvas_core::{
    A2UITree, Element, ElementId, ElementKind, ImageFormat, SceneDocument, SceneStore, Transform,
};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::warn;

use crate::resources::{self, CanvasSession};
use crate::tools::{
    canvas_export, canvas_interact, ExportParams, InteractParams, RenderContent, RenderParams,
};
use crate::ToolResponse;

// ============================================================================
// Helper Functions
// ============================================================================

/// Extract session ID from JSON arguments with a default fallback.
fn extract_session_id(arguments: &serde_json::Value) -> String {
    arguments
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or("default")
        .to_string()
}

/// Extract and parse an element ID from JSON arguments.
fn extract_element_id(arguments: &serde_json::Value) -> Result<ElementId, ToolResponse> {
    let id_str = arguments
        .get("element_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolResponse::error("Missing required field: element_id"))?;

    ElementId::parse(id_str).map_err(|e| ToolResponse::error(format!("Invalid element_id: {e}")))
}

/// Parse a transform from JSON, using defaults for missing fields.
#[allow(clippy::cast_possible_truncation)]
fn parse_transform(json: Option<&serde_json::Value>) -> Transform {
    let Some(t) = json else {
        return Transform::default();
    };

    Transform {
        x: t.get("x")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0) as f32,
        y: t.get("y")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0) as f32,
        width: t
            .get("width")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(100.0) as f32,
        height: t
            .get("height")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(100.0) as f32,
        rotation: t
            .get("rotation")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0) as f32,
        z_index: t
            .get("z_index")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0) as i32,
    }
}

/// Create a new default session metadata.
fn create_session_metadata(session_id: &str, width: f32, height: f32) -> CanvasSession {
    CanvasSession {
        id: session_id.to_string(),
        name: format!("Session {session_id}"),
        created_at: chrono_now(),
        modified_at: chrono_now(),
        width,
        height,
        element_count: 0,
    }
}

/// Create an element from render content.
fn create_element_from_content(content: &RenderContent) -> Element {
    match content {
        RenderContent::Chart {
            chart_type, data, ..
        } => Element::new(ElementKind::Chart {
            chart_type: chart_type.clone(),
            data: data.clone(),
        }),
        RenderContent::Image { src, .. } => Element::new(ElementKind::Image {
            src: src.clone(),
            format: detect_image_format(src),
        }),
        RenderContent::Text { content, font_size } => Element::new(ElementKind::Text {
            content: content.clone(),
            font_size: font_size.unwrap_or(16.0),
            color: "#000000".to_string(),
        }),
        RenderContent::Model3D { src, rotation } => Element::new(ElementKind::Model3D {
            src: src.clone(),
            rotation: rotation.unwrap_or([0.0, 0.0, 0.0]),
            scale: 1.0,
        }),
    }
}

/// Update session metadata after a scene change.
fn update_session_metadata(session: &mut CanvasSession, element_count: usize) {
    session.element_count = element_count;
    session.modified_at = chrono_now();
}

/// Apply partial transform updates from JSON to an existing transform.
#[allow(clippy::cast_possible_truncation)]
fn apply_transform_updates(transform: &mut Transform, json: &serde_json::Value) {
    if let Some(x) = json.get("x").and_then(serde_json::Value::as_f64) {
        transform.x = x as f32;
    }
    if let Some(y) = json.get("y").and_then(serde_json::Value::as_f64) {
        transform.y = y as f32;
    }
    if let Some(width) = json.get("width").and_then(serde_json::Value::as_f64) {
        transform.width = width as f32;
    }
    if let Some(height) = json.get("height").and_then(serde_json::Value::as_f64) {
        transform.height = height as f32;
    }
    if let Some(rotation) = json.get("rotation").and_then(serde_json::Value::as_f64) {
        transform.rotation = rotation as f32;
    }
    if let Some(z_index) = json.get("z_index").and_then(serde_json::Value::as_i64) {
        transform.z_index = z_index as i32;
    }
}

// ============================================================================
// JSON-RPC Types
// ============================================================================

/// JSON-RPC 2.0 request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    /// JSON-RPC version (must be "2.0").
    pub jsonrpc: String,
    /// Request ID.
    pub id: serde_json::Value,
    /// Method name.
    pub method: String,
    /// Method parameters.
    #[serde(default)]
    pub params: serde_json::Value,
}

/// JSON-RPC 2.0 response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    /// JSON-RPC version (always "2.0").
    pub jsonrpc: String,
    /// Request ID (matches request).
    pub id: serde_json::Value,
    /// Result (on success).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    /// Error (on failure).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

/// JSON-RPC 2.0 error.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    /// Error code.
    pub code: i32,
    /// Error message.
    pub message: String,
    /// Additional data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl JsonRpcResponse {
    /// Create a success response.
    #[must_use]
    pub fn success(id: serde_json::Value, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    /// Create an error response.
    #[must_use]
    pub fn error(id: serde_json::Value, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
                data: None,
            }),
        }
    }
}

// ============================================================================
// MCP Tool & Resource Definitions
// ============================================================================

/// MCP tool definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    /// Tool name.
    pub name: String,
    /// Tool description.
    pub description: String,
    /// Input schema (JSON Schema).
    #[serde(rename = "inputSchema")]
    pub input_schema: serde_json::Value,
}

/// MCP resource definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resource {
    /// Resource URI.
    pub uri: String,
    /// Resource name.
    pub name: String,
    /// Resource description.
    pub description: String,
    /// MIME type.
    #[serde(rename = "mimeType")]
    pub mime_type: String,
}

// ============================================================================
// MCP Server
// ============================================================================

/// Callback type for scene change notifications.
pub type OnChangeCallback = Box<dyn Fn(&str, &canvas_core::Scene) + Send + Sync>;

/// MCP server for Saorsa Canvas.
///
/// Uses a shared [] for scene state and maintains separate metadata
/// for session tracking (element counts, timestamps, etc.).
pub struct CanvasMcpServer {
    /// Shared scene storage.
    store: SceneStore,
    /// Session metadata (`element_count`, `modified_at`, etc.).
    session_metadata: Arc<RwLock<HashMap<String, CanvasSession>>>,
    /// Change notification callback.
    on_change: Option<OnChangeCallback>,
}

impl CanvasMcpServer {
    /// Create a new MCP server with the given scene store.
    #[must_use]
    pub fn new(store: SceneStore) -> Self {
        Self {
            store,
            session_metadata: Arc::new(RwLock::new(HashMap::new())),
            on_change: None,
        }
    }

    /// Set the change notification callback.
    pub fn set_on_change<F>(&mut self, callback: F)
    where
        F: Fn(&str, &canvas_core::Scene) + Send + Sync + 'static,
    {
        self.on_change = Some(Box::new(callback));
    }

    /// Import a canonical scene document without triggering callbacks.
    pub async fn import_scene_document(&self, document: SceneDocument) {
        let session_id = document.session_id.clone();
        match document.clone().into_scene() {
            Ok(scene) => {
                let viewport = document.viewport;
                if let Err(e) = self.store.replace(&session_id, scene) {
                    warn!("Failed to import scene for {}: {}", session_id, e);
                    return;
                }

                // Update metadata
                let mut metadata = self.session_metadata.write().await;
                let entry = metadata.entry(session_id.clone()).or_insert_with(|| {
                    create_session_metadata(&session_id, viewport.width, viewport.height)
                });
                entry.width = viewport.width;
                entry.height = viewport.height;
                if let Some(scene) = self.store.get(&session_id) {
                    update_session_metadata(entry, scene.element_count());
                }
            }
            Err(err) => warn!(
                "Failed to import scene document for {}: {}",
                session_id, err
            ),
        }
    }

    /// Handle a JSON-RPC request.
    pub async fn handle_request(&self, request: JsonRpcRequest) -> JsonRpcResponse {
        tracing::debug!("MCP request: {} {:?}", request.method, request.params);

        match request.method.as_str() {
            // MCP standard methods
            "initialize" => self.handle_initialize(request.id).await,
            "tools/list" => self.handle_tools_list(request.id),
            "tools/call" => self.handle_tools_call(request.id, request.params).await,
            "resources/list" => self.handle_resources_list(request.id),
            "resources/read" => self.handle_resources_read(request.id, request.params),

            // Unknown method
            _ => JsonRpcResponse::error(
                request.id,
                -32601,
                format!("Method not found: {}", request.method),
            ),
        }
    }

    /// Handle initialize request.
    async fn handle_initialize(&self, id: serde_json::Value) -> JsonRpcResponse {
        // Ensure default session exists in store (get_or_create handles this)
        let _ = self.store.get_or_create("default");

        // Initialize metadata for default session
        let mut metadata = self.session_metadata.write().await;
        if !metadata.contains_key("default") {
            metadata.insert(
                "default".to_string(),
                CanvasSession {
                    id: "default".to_string(),
                    name: "Default Canvas".to_string(),
                    created_at: chrono_now(),
                    modified_at: chrono_now(),
                    width: 800.0,
                    height: 600.0,
                    element_count: 0,
                },
            );
        }

        JsonRpcResponse::success(
            id,
            serde_json::json!({
                "protocolVersion": "2024-11-05",
                "serverInfo": {
                    "name": "saorsa-canvas",
                    "version": env!("CARGO_PKG_VERSION")
                },
                "capabilities": {
                    "tools": {},
                    "resources": {}
                }
            }),
        )
    }

    /// Handle tools/list request.
    #[allow(clippy::unused_self)]
    fn handle_tools_list(&self, id: serde_json::Value) -> JsonRpcResponse {
        JsonRpcResponse::success(id, serde_json::json!({ "tools": get_available_tools() }))
    }

    /// Handle tools/call request.
    async fn handle_tools_call(
        &self,
        id: serde_json::Value,
        params: serde_json::Value,
    ) -> JsonRpcResponse {
        let name = params
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let arguments = params.get("arguments").cloned().unwrap_or_default();

        let result = match name {
            "canvas_render" => self.call_canvas_render(arguments).await,
            "canvas_render_a2ui" => self.call_canvas_render_a2ui(arguments).await,
            "canvas_interact" => self.call_canvas_interact(arguments),
            "canvas_export" => self.call_canvas_export(arguments),
            "canvas_clear" => self.call_canvas_clear(arguments).await,
            "canvas_add_element" => self.call_canvas_add_element(arguments).await,
            "canvas_remove_element" => self.call_canvas_remove_element(arguments).await,
            "canvas_update_element" => self.call_canvas_update_element(arguments).await,
            "canvas_get_scene" => self.call_canvas_get_scene(arguments),
            _ => ToolResponse::error(format!("Unknown tool: {name}")),
        };

        if result.success {
            JsonRpcResponse::success(
                id,
                serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": serde_json::to_string_pretty(&result.data).unwrap_or_default()
                    }]
                }),
            )
        } else {
            JsonRpcResponse::error(id, -32000, result.error.unwrap_or_default())
        }
    }

    /// Call `canvas_render` tool with scene mutation.
    async fn call_canvas_render(&self, arguments: serde_json::Value) -> ToolResponse {
        let params: RenderParams = match serde_json::from_value(arguments) {
            Ok(p) => p,
            Err(e) => return ToolResponse::error(format!("Invalid parameters: {e}")),
        };

        let session_id = params.session_id.clone();

        // Create element from content
        let mut element = create_element_from_content(&params.content);

        // Apply position if specified
        if let Some(pos) = &params.position {
            element = element.with_transform(Transform {
                x: pos.x,
                y: pos.y,
                width: pos.width.unwrap_or(200.0),
                height: pos.height.unwrap_or(150.0),
                rotation: 0.0,
                z_index: 0,
            });
        }

        let element_id = element.id;

        // Add element to store (creates session if needed)
        if let Err(e) = self.store.add_element(&session_id, element) {
            return ToolResponse::error(format!("Failed to add element: {e}"));
        }

        // Update metadata
        let element_count = self.store.get(&session_id).map_or(0, |s| s.element_count());

        let mut metadata = self.session_metadata.write().await;
        let session = metadata
            .entry(session_id.clone())
            .or_insert_with(|| create_session_metadata(&session_id, 800.0, 600.0));
        update_session_metadata(session, element_count);
        let final_count = session.element_count;
        drop(metadata);

        // Notify change callback
        if let Some(ref callback) = self.on_change {
            if let Some(scene) = self.store.get(&session_id) {
                callback(&session_id, &scene);
            }
        }

        ToolResponse::success(serde_json::json!({
            "session_id": session_id,
            "element_id": element_id.to_string(),
            "rendered": true,
            "element_count": final_count
        }))
    }

    /// Call `canvas_render_a2ui` tool - render an A2UI component tree.
    ///
    /// Accepts an A2UI JSON tree, converts it to canvas elements,
    /// and either replaces or merges with the existing scene.
    async fn call_canvas_render_a2ui(&self, arguments: serde_json::Value) -> ToolResponse {
        let session_id = extract_session_id(&arguments);

        // Extract the A2UI tree
        let Some(tree_json) = arguments.get("tree") else {
            return ToolResponse::error("Missing required field: tree");
        };

        // Parse the A2UI tree
        let tree: A2UITree = match serde_json::from_value(tree_json.clone()) {
            Ok(t) => t,
            Err(e) => return ToolResponse::error(format!("Invalid A2UI tree: {e}")),
        };

        // Check merge mode (default: replace)
        let merge = arguments
            .get("merge")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);

        // Extract optional position offset
        #[allow(clippy::cast_possible_truncation)]
        let offset_x = arguments
            .get("offset_x")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0) as f32;
        #[allow(clippy::cast_possible_truncation)]
        let offset_y = arguments
            .get("offset_y")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0) as f32;

        // Convert A2UI tree to elements
        let conversion_result = tree.to_elements();
        let mut elements = conversion_result.elements;

        // Apply offset to all element positions
        for element in &mut elements {
            element.transform.x += offset_x;
            element.transform.y += offset_y;
        }

        // Collect element IDs before moving
        let element_ids: Vec<String> = elements.iter().map(|e| e.id.to_string()).collect();

        // Clear existing scene if not merging
        if !merge {
            if let Err(e) = self.store.clear(&session_id) {
                // If session doesn't exist, that's fine - we'll create it
                if self.store.get(&session_id).is_some() {
                    return ToolResponse::error(format!("Failed to clear session: {e}"));
                }
            }
        }

        // Add all converted elements to the scene
        for element in elements {
            if let Err(e) = self.store.add_element(&session_id, element) {
                return ToolResponse::error(format!("Failed to add element: {e}"));
            }
        }

        // Update metadata
        let element_count = self.store.get(&session_id).map_or(0, |s| s.element_count());

        let mut metadata = self.session_metadata.write().await;
        let session = metadata
            .entry(session_id.clone())
            .or_insert_with(|| create_session_metadata(&session_id, 800.0, 600.0));
        update_session_metadata(session, element_count);
        let final_count = session.element_count;
        drop(metadata);

        // Notify change callback
        if let Some(ref callback) = self.on_change {
            if let Some(scene) = self.store.get(&session_id) {
                callback(&session_id, &scene);
            }
        }

        // Include any conversion warnings
        let warnings: Vec<String> = conversion_result.warnings;

        ToolResponse::success(serde_json::json!({
            "session_id": session_id,
            "element_ids": element_ids,
            "element_count": final_count,
            "rendered": true,
            "warnings": warnings
        }))
    }

    /// Call `canvas_interact` tool.
    #[allow(clippy::unused_self)]
    fn call_canvas_interact(&self, arguments: serde_json::Value) -> ToolResponse {
        let params: InteractParams = match serde_json::from_value(arguments) {
            Ok(p) => p,
            Err(e) => return ToolResponse::error(format!("Invalid parameters: {e}")),
        };

        canvas_interact(params)
    }

    /// Call `canvas_export` tool.
    #[allow(clippy::unused_self)]
    fn call_canvas_export(&self, arguments: serde_json::Value) -> ToolResponse {
        let params: ExportParams = match serde_json::from_value(arguments) {
            Ok(p) => p,
            Err(e) => return ToolResponse::error(format!("Invalid parameters: {e}")),
        };

        canvas_export(&params)
    }

    /// Call `canvas_clear` tool.
    async fn call_canvas_clear(&self, arguments: serde_json::Value) -> ToolResponse {
        let session_id = extract_session_id(&arguments);

        // Check if session exists
        if self.store.get(&session_id).is_none() {
            return ToolResponse::error(format!("Session not found: {session_id}"));
        }

        // Clear the scene in store
        if let Err(e) = self.store.clear(&session_id) {
            return ToolResponse::error(format!("Failed to clear session: {e}"));
        }

        // Update metadata
        let mut metadata = self.session_metadata.write().await;
        if let Some(session) = metadata.get_mut(&session_id) {
            update_session_metadata(session, 0);
        }
        drop(metadata);

        // Notify change callback
        if let Some(ref callback) = self.on_change {
            if let Some(scene) = self.store.get(&session_id) {
                callback(&session_id, &scene);
            }
        }

        ToolResponse::success(serde_json::json!({
            "session_id": session_id,
            "cleared": true
        }))
    }

    /// Call `canvas_add_element` tool - add element with full control.
    async fn call_canvas_add_element(&self, arguments: serde_json::Value) -> ToolResponse {
        let session_id = extract_session_id(&arguments);

        let Some(kind_json) = arguments.get("kind") else {
            return ToolResponse::error("Missing required field: kind");
        };

        let kind: ElementKind = match serde_json::from_value(kind_json.clone()) {
            Ok(k) => k,
            Err(e) => return ToolResponse::error(format!("Invalid element kind: {e}")),
        };

        let transform = parse_transform(arguments.get("transform"));
        let interactive = arguments
            .get("interactive")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true);

        let element = Element::new(kind)
            .with_transform(transform)
            .with_interactive(interactive);
        let element_id = element.id;

        // Add element to store (creates session if needed)
        if let Err(e) = self.store.add_element(&session_id, element) {
            return ToolResponse::error(format!("Failed to add element: {e}"));
        }

        // Update metadata
        let element_count = self.store.get(&session_id).map_or(0, |s| s.element_count());

        let mut metadata = self.session_metadata.write().await;
        let session = metadata
            .entry(session_id.clone())
            .or_insert_with(|| create_session_metadata(&session_id, 800.0, 600.0));
        update_session_metadata(session, element_count);
        let final_count = session.element_count;
        drop(metadata);

        // Notify change callback
        if let Some(ref callback) = self.on_change {
            if let Some(scene) = self.store.get(&session_id) {
                callback(&session_id, &scene);
            }
        }

        ToolResponse::success(serde_json::json!({
            "session_id": session_id,
            "element_id": element_id.to_string(),
            "element_count": final_count
        }))
    }

    /// Call `canvas_remove_element` tool - remove element by ID.
    async fn call_canvas_remove_element(&self, arguments: serde_json::Value) -> ToolResponse {
        let session_id = extract_session_id(&arguments);
        let element_id = match extract_element_id(&arguments) {
            Ok(id) => id,
            Err(response) => return response,
        };
        let element_id_str = element_id.to_string();

        // Check if session exists
        if self.store.get(&session_id).is_none() {
            return ToolResponse::error(format!("Session not found: {session_id}"));
        }

        // Remove element from store
        if let Err(e) = self.store.remove_element(&session_id, element_id) {
            return ToolResponse::error(format!("Failed to remove element: {e}"));
        }

        // Update metadata
        let element_count = self.store.get(&session_id).map_or(0, |s| s.element_count());

        let mut metadata = self.session_metadata.write().await;
        if let Some(session) = metadata.get_mut(&session_id) {
            update_session_metadata(session, element_count);
        }
        let final_count = metadata.get(&session_id).map_or(0, |s| s.element_count);
        drop(metadata);

        // Notify change callback
        if let Some(ref callback) = self.on_change {
            if let Some(scene) = self.store.get(&session_id) {
                callback(&session_id, &scene);
            }
        }

        ToolResponse::success(serde_json::json!({
            "session_id": session_id,
            "removed": true,
            "element_id": element_id_str,
            "element_count": final_count
        }))
    }

    /// Call `canvas_update_element` tool - update element properties.
    async fn call_canvas_update_element(&self, arguments: serde_json::Value) -> ToolResponse {
        let session_id = extract_session_id(&arguments);
        let element_id = match extract_element_id(&arguments) {
            Ok(id) => id,
            Err(response) => return response,
        };
        let element_id_str = element_id.to_string();

        // Check if session exists
        if self.store.get(&session_id).is_none() {
            return ToolResponse::error(format!("Session not found: {session_id}"));
        }

        // Clone arguments for the closure
        let transform_json = arguments.get("transform").cloned();
        let interactive = arguments
            .get("interactive")
            .and_then(serde_json::Value::as_bool);

        // Update element in store
        let result = self
            .store
            .update_element(&session_id, element_id, |element| {
                if let Some(ref t) = transform_json {
                    apply_transform_updates(&mut element.transform, t);
                }
                if let Some(inter) = interactive {
                    element.interactive = inter;
                }
            });

        if let Err(e) = result {
            return ToolResponse::error(format!("Failed to update element: {e}"));
        }

        // Update metadata timestamp
        let mut metadata = self.session_metadata.write().await;
        if let Some(session) = metadata.get_mut(&session_id) {
            session.modified_at = chrono_now();
        }
        drop(metadata);

        // Notify change callback
        if let Some(ref callback) = self.on_change {
            if let Some(scene) = self.store.get(&session_id) {
                callback(&session_id, &scene);
            }
        }

        ToolResponse::success(serde_json::json!({
            "session_id": session_id,
            "updated": true,
            "element_id": element_id_str
        }))
    }

    /// Call `canvas_get_scene` tool - get current scene state.
    #[allow(clippy::needless_pass_by_value)]
    fn call_canvas_get_scene(&self, arguments: serde_json::Value) -> ToolResponse {
        let session_id = extract_session_id(&arguments);

        // Check if session exists
        if self.store.get(&session_id).is_none() {
            return ToolResponse::error(format!("Session not found: {session_id}"));
        }

        let document = self.store.scene_document(&session_id);

        ToolResponse::success(serde_json::json!({
            "session_id": session_id,
            "scene": document,
        }))
    }

    /// Handle resources/list request.
    fn handle_resources_list(&self, id: serde_json::Value) -> JsonRpcResponse {
        let session_ids = self.store.session_ids();

        let mut resource_list: Vec<Resource> = session_ids
            .iter()
            .map(|session_id| Resource {
                uri: format!("canvas://session/{session_id}"),
                name: format!("Canvas Session: {session_id}"),
                description: "A canvas session with visual elements".to_string(),
                mime_type: "application/json".to_string(),
            })
            .collect();

        // Add chart templates
        for chart_type in &["bar", "line", "pie", "area", "scatter"] {
            let capitalized = capitalize(chart_type);
            resource_list.push(Resource {
                uri: format!("canvas://chart/{chart_type}"),
                name: format!("{capitalized} Chart Template"),
                description: format!("Template for creating {chart_type} charts"),
                mime_type: "application/json".to_string(),
            });
        }

        JsonRpcResponse::success(id, serde_json::json!({ "resources": resource_list }))
    }

    /// Handle resources/read request.
    #[allow(clippy::unused_self, clippy::needless_pass_by_value)]
    fn handle_resources_read(
        &self,
        id: serde_json::Value,
        params: serde_json::Value,
    ) -> JsonRpcResponse {
        let uri = params
            .get("uri")
            .and_then(|v| v.as_str())
            .unwrap_or_default();

        match resources::get_resource(uri) {
            Ok(content) => JsonRpcResponse::success(
                id,
                serde_json::json!({
                    "contents": [{
                        "uri": uri,
                        "mimeType": "application/json",
                        "text": match content {
                            crate::ResourceContent::Json(v) => serde_json::to_string_pretty(&v).unwrap_or_default(),
                            crate::ResourceContent::Text(s) => s,
                            crate::ResourceContent::Binary { data, .. } => data,
                        }
                    }]
                }),
            ),
            Err(e) => JsonRpcResponse::error(id, -32002, e),
        }
    }
}

// ============================================================================
// Utility Functions
// ============================================================================

/// Get current timestamp in ISO 8601 format.
fn chrono_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();
    format!("{secs}")
}

/// Detect image format from source string (URL or data URI).
fn detect_image_format(src: &str) -> ImageFormat {
    let src_lower = src.to_lowercase();
    if src_lower.contains("png") || src_lower.starts_with("data:image/png") {
        ImageFormat::Png
    } else if src_lower.contains("jpg")
        || src_lower.contains("jpeg")
        || src_lower.starts_with("data:image/jpeg")
    {
        ImageFormat::Jpeg
    } else if src_lower.contains("svg") || src_lower.starts_with("data:image/svg") {
        ImageFormat::Svg
    } else if src_lower.contains("webp") || src_lower.starts_with("data:image/webp") {
        ImageFormat::WebP
    } else {
        // Default to PNG
        ImageFormat::Png
    }
}

/// Capitalize the first letter of a string.
fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().chain(chars).collect(),
    }
}

// ============================================================================
// Tool Schemas
// ============================================================================

/// Get the list of available MCP tools.
fn get_available_tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "canvas_render".to_string(),
            description: "Render content (chart, image, text, 3D model) to the canvas".to_string(),
            input_schema: render_tool_schema(),
        },
        Tool {
            name: "canvas_render_a2ui".to_string(),
            description: "Render an A2UI component tree to the canvas. Supports Container, Text, Image, Button, Chart, and VideoFeed components with automatic layout.".to_string(),
            input_schema: render_a2ui_tool_schema(),
        },
        Tool {
            name: "canvas_interact".to_string(),
            description: "Report user interaction (touch, voice, selection) on the canvas"
                .to_string(),
            input_schema: interact_tool_schema(),
        },
        Tool {
            name: "canvas_export".to_string(),
            description: "Export the canvas to an image or PDF".to_string(),
            input_schema: export_tool_schema(),
        },
        Tool {
            name: "canvas_clear".to_string(),
            description: "Clear all elements from the canvas".to_string(),
            input_schema: clear_tool_schema(),
        },
        Tool {
            name: "canvas_add_element".to_string(),
            description: "Add an element to the canvas with full control over type, transform, and properties".to_string(),
            input_schema: add_element_tool_schema(),
        },
        Tool {
            name: "canvas_remove_element".to_string(),
            description: "Remove an element from the canvas by its ID".to_string(),
            input_schema: remove_element_tool_schema(),
        },
        Tool {
            name: "canvas_update_element".to_string(),
            description: "Update an existing element's transform or properties".to_string(),
            input_schema: update_element_tool_schema(),
        },
        Tool {
            name: "canvas_get_scene".to_string(),
            description: "Get the current scene state as a JSON document".to_string(),
            input_schema: get_scene_tool_schema(),
        },
    ]
}

/// Common `session_id` property schema.
fn session_id_property() -> serde_json::Value {
    serde_json::json!({
        "type": "string",
        "description": "Canvas session ID (defaults to 'default')",
        "default": "default"
    })
}

/// Common `element_id` property schema.
fn element_id_property() -> serde_json::Value {
    serde_json::json!({
        "type": "string",
        "description": "Unique element identifier (UUID format)"
    })
}

/// Common transform property schema.
fn transform_property() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "description": "Element transform (position, size, rotation)",
        "properties": {
            "x": { "type": "number", "description": "X position in pixels" },
            "y": { "type": "number", "description": "Y position in pixels" },
            "width": { "type": "number", "description": "Width in pixels" },
            "height": { "type": "number", "description": "Height in pixels" },
            "rotation": { "type": "number", "description": "Rotation in degrees" },
            "z_index": { "type": "integer", "description": "Stack order (higher = front)" }
        }
    })
}

/// Schema for `canvas_render` tool.
fn render_tool_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "session_id": session_id_property(),
            "content": {
                "type": "object",
                "description": "Content to render",
                "oneOf": [
                    {
                        "type": "object",
                        "properties": {
                            "type": { "const": "Chart" },
                            "data": {
                                "type": "object",
                                "properties": {
                                    "chart_type": { "type": "string", "enum": ["bar", "line", "pie", "area", "scatter"] },
                                    "data": { "type": "object" }
                                },
                                "required": ["chart_type", "data"]
                            }
                        },
                        "required": ["type", "data"]
                    },
                    {
                        "type": "object",
                        "properties": {
                            "type": { "const": "Image" },
                            "data": {
                                "type": "object",
                                "properties": {
                                    "src": { "type": "string" }
                                },
                                "required": ["src"]
                            }
                        },
                        "required": ["type", "data"]
                    },
                    {
                        "type": "object",
                        "properties": {
                            "type": { "const": "Text" },
                            "data": {
                                "type": "object",
                                "properties": {
                                    "content": { "type": "string" },
                                    "font_size": { "type": "number" }
                                },
                                "required": ["content"]
                            }
                        },
                        "required": ["type", "data"]
                    }
                ]
            },
            "position": {
                "type": "object",
                "properties": {
                    "x": { "type": "number" },
                    "y": { "type": "number" },
                    "width": { "type": "number" },
                    "height": { "type": "number" }
                },
                "required": ["x", "y"]
            }
        },
        "required": ["content"]
    })
}

/// Schema for `canvas_render_a2ui` tool.
fn render_a2ui_tool_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "session_id": session_id_property(),
            "tree": {
                "type": "object",
                "description": "A2UI component tree to render",
                "properties": {
                    "root": {
                        "type": "object",
                        "description": "Root A2UI node (Container, Text, Image, Button, Chart, or VideoFeed)"
                    },
                    "data_model": {
                        "type": "object",
                        "description": "Optional data bindings for dynamic content"
                    }
                },
                "required": ["root"]
            },
            "merge": {
                "type": "boolean",
                "description": "If true, merge with existing scene. If false (default), replace scene.",
                "default": false
            },
            "offset_x": {
                "type": "number",
                "description": "X offset to apply to all elements (default: 0)",
                "default": 0
            },
            "offset_y": {
                "type": "number",
                "description": "Y offset to apply to all elements (default: 0)",
                "default": 0
            }
        },
        "required": ["tree"]
    })
}

/// Schema for `canvas_interact` tool.
fn interact_tool_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "session_id": session_id_property(),
            "interaction_type": {
                "type": "string",
                "enum": ["touch", "voice", "selection"],
                "description": "Type of interaction"
            },
            "data": {
                "type": "object",
                "description": "Interaction-specific data"
            }
        },
        "required": ["interaction_type"]
    })
}

/// Schema for `canvas_export` tool.
fn export_tool_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "session_id": session_id_property(),
            "format": {
                "type": "string",
                "enum": ["png", "jpeg", "pdf", "svg"],
                "description": "Export format"
            },
            "quality": {
                "type": "number",
                "minimum": 0,
                "maximum": 100,
                "description": "Export quality (for lossy formats)"
            }
        },
        "required": ["format"]
    })
}

/// Schema for `canvas_clear` tool.
fn clear_tool_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "session_id": session_id_property()
        }
    })
}

/// Schema for `canvas_add_element` tool.
fn add_element_tool_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "session_id": session_id_property(),
            "kind": {
                "type": "object",
                "description": "Element type and data",
                "oneOf": [
                    {
                        "type": "object",
                        "properties": {
                            "type": { "const": "Text" },
                            "data": {
                                "type": "object",
                                "properties": {
                                    "content": { "type": "string" },
                                    "font_size": { "type": "number" },
                                    "color": { "type": "string" }
                                },
                                "required": ["content", "font_size", "color"]
                            }
                        },
                        "required": ["type", "data"]
                    },
                    {
                        "type": "object",
                        "properties": {
                            "type": { "const": "Chart" },
                            "data": {
                                "type": "object",
                                "properties": {
                                    "chart_type": { "type": "string" },
                                    "data": { "type": "object" }
                                },
                                "required": ["chart_type", "data"]
                            }
                        },
                        "required": ["type", "data"]
                    },
                    {
                        "type": "object",
                        "properties": {
                            "type": { "const": "Image" },
                            "data": {
                                "type": "object",
                                "properties": {
                                    "src": { "type": "string" },
                                    "format": { "type": "string" }
                                },
                                "required": ["src", "format"]
                            }
                        },
                        "required": ["type", "data"]
                    }
                ]
            },
            "transform": transform_property(),
            "interactive": {
                "type": "boolean",
                "description": "Whether the element responds to interactions",
                "default": true
            }
        },
        "required": ["kind"]
    })
}

/// Schema for `canvas_remove_element` tool.
fn remove_element_tool_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "session_id": session_id_property(),
            "element_id": element_id_property()
        },
        "required": ["element_id"]
    })
}

/// Schema for `canvas_update_element` tool.
fn update_element_tool_schema() -> serde_json::Value {
    let mut transform = transform_property();
    transform["description"] = serde_json::json!("New transform values (partial update supported)");

    serde_json::json!({
        "type": "object",
        "properties": {
            "session_id": session_id_property(),
            "element_id": element_id_property(),
            "transform": transform,
            "interactive": {
                "type": "boolean",
                "description": "Whether the element responds to interactions"
            }
        },
        "required": ["element_id"]
    })
}

/// Schema for `canvas_get_scene` tool.
fn get_scene_tool_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "session_id": session_id_property()
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_initialize() {
        let server = CanvasMcpServer::new(SceneStore::new());
        let response = server
            .handle_request(JsonRpcRequest {
                jsonrpc: "2.0".to_string(),
                id: serde_json::json!(1),
                method: "initialize".to_string(),
                params: serde_json::json!({}),
            })
            .await;

        assert!(response.result.is_some());
        assert!(response.error.is_none());
    }

    #[tokio::test]
    async fn test_tools_list() {
        let server = CanvasMcpServer::new(SceneStore::new());
        let response = server
            .handle_request(JsonRpcRequest {
                jsonrpc: "2.0".to_string(),
                id: serde_json::json!(1),
                method: "tools/list".to_string(),
                params: serde_json::json!({}),
            })
            .await;

        assert!(response.result.is_some());
        let result = response.result.unwrap();
        let tools = result["tools"].as_array().unwrap();
        assert!(!tools.is_empty());
    }

    #[tokio::test]
    async fn test_canvas_render() {
        let server = CanvasMcpServer::new(SceneStore::new());

        // Initialize first
        server
            .handle_request(JsonRpcRequest {
                jsonrpc: "2.0".to_string(),
                id: serde_json::json!(0),
                method: "initialize".to_string(),
                params: serde_json::json!({}),
            })
            .await;

        // Render a chart
        let response = server
            .handle_request(JsonRpcRequest {
                jsonrpc: "2.0".to_string(),
                id: serde_json::json!(1),
                method: "tools/call".to_string(),
                params: serde_json::json!({
                    "name": "canvas_render",
                    "arguments": {
                        "session_id": "default",
                        "content": {
                            "type": "Chart",
                            "data": {
                                "chart_type": "bar",
                                "data": {
                                    "labels": ["A", "B", "C"],
                                    "values": [10, 20, 30]
                                }
                            }
                        }
                    }
                }),
            })
            .await;

        assert!(response.result.is_some());
        assert!(response.error.is_none());
    }

    #[tokio::test]
    async fn test_canvas_add_element() {
        let server = CanvasMcpServer::new(SceneStore::new());

        // Initialize first
        server
            .handle_request(JsonRpcRequest {
                jsonrpc: "2.0".to_string(),
                id: serde_json::json!(0),
                method: "initialize".to_string(),
                params: serde_json::json!({}),
            })
            .await;

        // Add a text element
        let response = server
            .handle_request(JsonRpcRequest {
                jsonrpc: "2.0".to_string(),
                id: serde_json::json!(1),
                method: "tools/call".to_string(),
                params: serde_json::json!({
                    "name": "canvas_add_element",
                    "arguments": {
                        "session_id": "default",
                        "kind": {
                            "type": "Text",
                            "data": {
                                "content": "Hello World",
                                "font_size": 24.0,
                                "color": "#ff0000"
                            }
                        },
                        "transform": {
                            "x": 100.0,
                            "y": 200.0,
                            "width": 300.0,
                            "height": 50.0
                        }
                    }
                }),
            })
            .await;

        assert!(response.result.is_some());
        assert!(response.error.is_none());

        // Verify content has element_id
        let result = response.result.unwrap();
        let content = result["content"].as_array().unwrap();
        let text = content[0]["text"].as_str().unwrap();
        assert!(text.contains("element_id"));
    }

    #[tokio::test]
    async fn test_canvas_get_scene() {
        let server = CanvasMcpServer::new(SceneStore::new());

        // Initialize first
        server
            .handle_request(JsonRpcRequest {
                jsonrpc: "2.0".to_string(),
                id: serde_json::json!(0),
                method: "initialize".to_string(),
                params: serde_json::json!({}),
            })
            .await;

        // Get scene
        let response = server
            .handle_request(JsonRpcRequest {
                jsonrpc: "2.0".to_string(),
                id: serde_json::json!(1),
                method: "tools/call".to_string(),
                params: serde_json::json!({
                    "name": "canvas_get_scene",
                    "arguments": {
                        "session_id": "default"
                    }
                }),
            })
            .await;

        assert!(response.result.is_some());
        assert!(response.error.is_none());

        // Verify response contains scene document
        let result = response.result.unwrap();
        let content = result["content"].as_array().unwrap();
        let text = content[0]["text"].as_str().unwrap();
        let scene_payload: serde_json::Value =
            serde_json::from_str(text).expect("scene payload should parse");
        assert!(scene_payload.get("scene").is_some());
    }

    #[tokio::test]
    async fn test_canvas_remove_element() {
        let server = CanvasMcpServer::new(SceneStore::new());

        // Initialize first
        server
            .handle_request(JsonRpcRequest {
                jsonrpc: "2.0".to_string(),
                id: serde_json::json!(0),
                method: "initialize".to_string(),
                params: serde_json::json!({}),
            })
            .await;

        // Add an element first
        let add_response = server
            .handle_request(JsonRpcRequest {
                jsonrpc: "2.0".to_string(),
                id: serde_json::json!(1),
                method: "tools/call".to_string(),
                params: serde_json::json!({
                    "name": "canvas_add_element",
                    "arguments": {
                        "session_id": "default",
                        "kind": {
                            "type": "Text",
                            "data": {
                                "content": "To Remove",
                                "font_size": 16.0,
                                "color": "#000000"
                            }
                        }
                    }
                }),
            })
            .await;

        assert!(add_response.result.is_some());
        let add_result = add_response.result.unwrap();
        let content = add_result["content"].as_array().unwrap();
        let text = content[0]["text"].as_str().unwrap();
        let data: serde_json::Value = serde_json::from_str(text).unwrap();
        let element_id = data["element_id"].as_str().unwrap();

        // Remove the element
        let remove_response = server
            .handle_request(JsonRpcRequest {
                jsonrpc: "2.0".to_string(),
                id: serde_json::json!(2),
                method: "tools/call".to_string(),
                params: serde_json::json!({
                    "name": "canvas_remove_element",
                    "arguments": {
                        "session_id": "default",
                        "element_id": element_id
                    }
                }),
            })
            .await;

        assert!(remove_response.result.is_some());
        assert!(remove_response.error.is_none());
    }

    #[tokio::test]
    async fn test_canvas_update_element() {
        let server = CanvasMcpServer::new(SceneStore::new());

        // Initialize first
        server
            .handle_request(JsonRpcRequest {
                jsonrpc: "2.0".to_string(),
                id: serde_json::json!(0),
                method: "initialize".to_string(),
                params: serde_json::json!({}),
            })
            .await;

        // Add an element first
        let add_response = server
            .handle_request(JsonRpcRequest {
                jsonrpc: "2.0".to_string(),
                id: serde_json::json!(1),
                method: "tools/call".to_string(),
                params: serde_json::json!({
                    "name": "canvas_add_element",
                    "arguments": {
                        "session_id": "default",
                        "kind": {
                            "type": "Text",
                            "data": {
                                "content": "To Update",
                                "font_size": 16.0,
                                "color": "#000000"
                            }
                        },
                        "transform": {
                            "x": 0.0,
                            "y": 0.0,
                            "width": 100.0,
                            "height": 50.0
                        }
                    }
                }),
            })
            .await;

        assert!(add_response.result.is_some());
        let add_result = add_response.result.unwrap();
        let content = add_result["content"].as_array().unwrap();
        let text = content[0]["text"].as_str().unwrap();
        let data: serde_json::Value = serde_json::from_str(text).unwrap();
        let element_id = data["element_id"].as_str().unwrap();

        // Update the element position
        let update_response = server
            .handle_request(JsonRpcRequest {
                jsonrpc: "2.0".to_string(),
                id: serde_json::json!(2),
                method: "tools/call".to_string(),
                params: serde_json::json!({
                    "name": "canvas_update_element",
                    "arguments": {
                        "session_id": "default",
                        "element_id": element_id,
                        "transform": {
                            "x": 100.0,
                            "y": 200.0
                        }
                    }
                }),
            })
            .await;

        assert!(update_response.result.is_some());
        assert!(update_response.error.is_none());

        // Verify the update via get_scene
        let get_response = server
            .handle_request(JsonRpcRequest {
                jsonrpc: "2.0".to_string(),
                id: serde_json::json!(3),
                method: "tools/call".to_string(),
                params: serde_json::json!({
                    "name": "canvas_get_scene",
                    "arguments": {
                        "session_id": "default"
                    }
                }),
            })
            .await;

        assert!(get_response.result.is_some());
        let get_result = get_response.result.unwrap();
        let content = get_result["content"].as_array().unwrap();
        let text = content[0]["text"].as_str().unwrap();
        // Verify position was updated
        assert!(text.contains("100"));
        assert!(text.contains("200"));
    }

    #[tokio::test]
    async fn test_tools_list_includes_new_tools() {
        let server = CanvasMcpServer::new(SceneStore::new());
        let response = server
            .handle_request(JsonRpcRequest {
                jsonrpc: "2.0".to_string(),
                id: serde_json::json!(1),
                method: "tools/list".to_string(),
                params: serde_json::json!({}),
            })
            .await;

        assert!(response.result.is_some());
        let result = response.result.unwrap();
        let tools = result["tools"].as_array().unwrap();

        // Should have 9 tools total
        assert_eq!(tools.len(), 9);

        // Verify all tool names are present
        let tool_names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
        assert!(tool_names.contains(&"canvas_render"));
        assert!(tool_names.contains(&"canvas_render_a2ui"));
        assert!(tool_names.contains(&"canvas_interact"));
        assert!(tool_names.contains(&"canvas_export"));
        assert!(tool_names.contains(&"canvas_clear"));
        assert!(tool_names.contains(&"canvas_add_element"));
        assert!(tool_names.contains(&"canvas_remove_element"));
        assert!(tool_names.contains(&"canvas_update_element"));
        assert!(tool_names.contains(&"canvas_get_scene"));
    }

    #[tokio::test]
    async fn test_canvas_render_a2ui() {
        let server = CanvasMcpServer::new(SceneStore::new());

        // Initialize first
        server
            .handle_request(JsonRpcRequest {
                jsonrpc: "2.0".to_string(),
                id: serde_json::json!(0),
                method: "initialize".to_string(),
                params: serde_json::json!({}),
            })
            .await;

        // Render an A2UI tree with a container and text
        // Note: A2UI uses "component" tag with snake_case values
        let response = server
            .handle_request(JsonRpcRequest {
                jsonrpc: "2.0".to_string(),
                id: serde_json::json!(1),
                method: "tools/call".to_string(),
                params: serde_json::json!({
                    "name": "canvas_render_a2ui",
                    "arguments": {
                        "session_id": "default",
                        "tree": {
                            "root": {
                                "component": "container",
                                "children": [
                                    {
                                        "component": "text",
                                        "content": "Hello A2UI"
                                    },
                                    {
                                        "component": "button",
                                        "label": "Click Me",
                                        "action": "submit"
                                    }
                                ],
                                "layout": "column"
                            }
                        }
                    }
                }),
            })
            .await;

        assert!(response.result.is_some());
        assert!(response.error.is_none());

        // Verify the response contains element_ids
        let result = response.result.unwrap();
        let content = result["content"].as_array().unwrap();
        let text = content[0]["text"].as_str().unwrap();
        assert!(text.contains("element_ids"));
        assert!(text.contains("rendered"));
    }

    #[tokio::test]
    async fn test_canvas_render_a2ui_with_merge() {
        let server = CanvasMcpServer::new(SceneStore::new());

        // Initialize first
        server
            .handle_request(JsonRpcRequest {
                jsonrpc: "2.0".to_string(),
                id: serde_json::json!(0),
                method: "initialize".to_string(),
                params: serde_json::json!({}),
            })
            .await;

        // First render (A2UI uses "component" tag with snake_case)
        server
            .handle_request(JsonRpcRequest {
                jsonrpc: "2.0".to_string(),
                id: serde_json::json!(1),
                method: "tools/call".to_string(),
                params: serde_json::json!({
                    "name": "canvas_render_a2ui",
                    "arguments": {
                        "session_id": "default",
                        "tree": {
                            "root": {
                                "component": "text",
                                "content": "First"
                            }
                        }
                    }
                }),
            })
            .await;

        // Second render with merge=true
        let response = server
            .handle_request(JsonRpcRequest {
                jsonrpc: "2.0".to_string(),
                id: serde_json::json!(2),
                method: "tools/call".to_string(),
                params: serde_json::json!({
                    "name": "canvas_render_a2ui",
                    "arguments": {
                        "session_id": "default",
                        "tree": {
                            "root": {
                                "component": "text",
                                "content": "Second"
                            }
                        },
                        "merge": true,
                        "offset_x": 100.0,
                        "offset_y": 50.0
                    }
                }),
            })
            .await;

        assert!(response.result.is_some());

        // Get scene and verify both elements exist
        let get_response = server
            .handle_request(JsonRpcRequest {
                jsonrpc: "2.0".to_string(),
                id: serde_json::json!(3),
                method: "tools/call".to_string(),
                params: serde_json::json!({
                    "name": "canvas_get_scene",
                    "arguments": {
                        "session_id": "default"
                    }
                }),
            })
            .await;

        let get_result = get_response.result.unwrap();
        let content = get_result["content"].as_array().unwrap();
        let text = content[0]["text"].as_str().unwrap();

        // Both "First" and "Second" should be in the scene
        assert!(text.contains("First"));
        assert!(text.contains("Second"));
    }
}
