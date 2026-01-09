//! MCP server implementation for Saorsa Canvas.
//!
//! Implements JSON-RPC 2.0 protocol for MCP tool calls and resource access.

use std::collections::HashMap;
use std::sync::Arc;

use canvas_core::{Element, ElementKind, ImageFormat, Scene, Transform};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::resources::{self, CanvasSession};
use crate::tools::{
    canvas_export, canvas_interact, ExportParams, InteractParams, RenderContent, RenderParams,
};
use crate::ToolResponse;

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

/// Canvas session state.
#[derive(Debug)]
pub struct SessionState {
    /// Session metadata.
    pub session: CanvasSession,
    /// Scene graph.
    pub scene: Scene,
}

/// Callback type for scene change notifications.
pub type OnChangeCallback = Box<dyn Fn(&str, &Scene) + Send + Sync>;

/// MCP server for Saorsa Canvas.
pub struct CanvasMcpServer {
    /// Active sessions.
    sessions: Arc<RwLock<HashMap<String, SessionState>>>,
    /// Change notification callback.
    on_change: Option<OnChangeCallback>,
}

impl CanvasMcpServer {
    /// Create a new MCP server.
    #[must_use]
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            on_change: None,
        }
    }

    /// Set the change notification callback.
    pub fn set_on_change<F>(&mut self, callback: F)
    where
        F: Fn(&str, &Scene) + Send + Sync + 'static,
    {
        self.on_change = Some(Box::new(callback));
    }

    /// Get the sessions map for external access.
    #[must_use]
    pub fn sessions(&self) -> Arc<RwLock<HashMap<String, SessionState>>> {
        Arc::clone(&self.sessions)
    }

    /// Handle a JSON-RPC request.
    pub async fn handle_request(&self, request: JsonRpcRequest) -> JsonRpcResponse {
        tracing::debug!("MCP request: {} {:?}", request.method, request.params);

        match request.method.as_str() {
            // MCP standard methods
            "initialize" => self.handle_initialize(request.id).await,
            "tools/list" => self.handle_tools_list(request.id),
            "tools/call" => self.handle_tools_call(request.id, request.params).await,
            "resources/list" => self.handle_resources_list(request.id).await,
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
        // Create default session
        let mut sessions = self.sessions.write().await;
        if !sessions.contains_key("default") {
            sessions.insert(
                "default".to_string(),
                SessionState {
                    session: CanvasSession {
                        id: "default".to_string(),
                        name: "Default Canvas".to_string(),
                        created_at: chrono_now(),
                        modified_at: chrono_now(),
                        width: 800.0,
                        height: 600.0,
                        element_count: 0,
                    },
                    scene: Scene::new(800.0, 600.0),
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
            "canvas_interact" => self.call_canvas_interact(arguments),
            "canvas_export" => self.call_canvas_export(arguments),
            "canvas_clear" => self.call_canvas_clear(arguments).await,
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
        let mut sessions = self.sessions.write().await;

        // Get or create session
        let state = sessions
            .entry(session_id.clone())
            .or_insert_with(|| SessionState {
                session: CanvasSession {
                    id: session_id.clone(),
                    name: format!("Session {session_id}"),
                    created_at: chrono_now(),
                    modified_at: chrono_now(),
                    width: 800.0,
                    height: 600.0,
                    element_count: 0,
                },
                scene: Scene::new(800.0, 600.0),
            });

        // Create element from content
        let element = match &params.content {
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
        };

        // Apply position if specified
        let element = if let Some(pos) = &params.position {
            element.with_transform(Transform {
                x: pos.x,
                y: pos.y,
                width: pos.width.unwrap_or(200.0),
                height: pos.height.unwrap_or(150.0),
                rotation: 0.0,
                z_index: 0,
            })
        } else {
            element
        };

        // Add to scene
        let final_id = element.id;
        state.scene.add_element(element);

        state.session.element_count = state.scene.element_count();
        state.session.modified_at = chrono_now();

        // Notify change
        if let Some(ref callback) = self.on_change {
            callback(&session_id, &state.scene);
        }

        ToolResponse::success(serde_json::json!({
            "session_id": session_id,
            "element_id": final_id.to_string(),
            "rendered": true,
            "element_count": state.session.element_count
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
        let session_id = arguments
            .get("session_id")
            .and_then(|v| v.as_str())
            .unwrap_or("default")
            .to_string();

        let mut sessions = self.sessions.write().await;

        if let Some(state) = sessions.get_mut(&session_id) {
            state.scene = Scene::new(state.session.width, state.session.height);
            state.session.element_count = 0;
            state.session.modified_at = chrono_now();

            // Notify change
            drop(sessions);
            if let Some(ref callback) = self.on_change {
                let sessions = self.sessions.read().await;
                if let Some(state) = sessions.get(&session_id) {
                    callback(&session_id, &state.scene);
                }
            }

            ToolResponse::success(serde_json::json!({
                "session_id": session_id,
                "cleared": true
            }))
        } else {
            ToolResponse::error(format!("Session not found: {session_id}"))
        }
    }

    /// Handle resources/list request.
    async fn handle_resources_list(&self, id: serde_json::Value) -> JsonRpcResponse {
        let sessions = self.sessions.read().await;

        let mut resource_list: Vec<Resource> = sessions
            .keys()
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

impl Default for CanvasMcpServer {
    fn default() -> Self {
        Self::new()
    }
}

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

/// Get the list of available MCP tools.
fn get_available_tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "canvas_render".to_string(),
            description: "Render content (chart, image, text, 3D model) to the canvas".to_string(),
            input_schema: render_tool_schema(),
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
    ]
}

/// Schema for `canvas_render` tool.
fn render_tool_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "session_id": {
                "type": "string",
                "description": "Canvas session ID (default: 'default')"
            },
            "content": {
                "type": "object",
                "description": "Content to render",
                "properties": {
                    "type": { "type": "string", "enum": ["Chart", "Image", "Text", "Model3D"] },
                    "data": { "type": "object" }
                },
                "required": ["type", "data"]
            },
            "position": {
                "type": "object",
                "properties": {
                    "x": { "type": "number" },
                    "y": { "type": "number" },
                    "width": { "type": "number" },
                    "height": { "type": "number" }
                }
            }
        },
        "required": ["content"]
    })
}

/// Schema for `canvas_interact` tool.
fn interact_tool_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "session_id": { "type": "string" },
            "interaction": {
                "type": "object",
                "properties": {
                    "type": { "type": "string", "enum": ["Touch", "Voice", "Select"] },
                    "data": { "type": "object" }
                },
                "required": ["type", "data"]
            }
        },
        "required": ["interaction"]
    })
}

/// Schema for `canvas_export` tool.
fn export_tool_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "session_id": { "type": "string" },
            "format": { "type": "string", "enum": ["png", "jpeg", "svg", "pdf", "webp"] },
            "quality": { "type": "integer", "minimum": 0, "maximum": 100, "default": 90 }
        },
        "required": ["format"]
    })
}

/// Schema for `canvas_clear` tool.
fn clear_tool_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "session_id": { "type": "string" }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_initialize() {
        let server = CanvasMcpServer::new();
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
        let server = CanvasMcpServer::new();
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
        let server = CanvasMcpServer::new();

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
}
