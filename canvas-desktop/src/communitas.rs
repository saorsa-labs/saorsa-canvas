//! Simplified Communitas MCP client for the desktop application.
//!
//! This module provides a minimal client to connect to a Communitas MCP server
//! and fetch scene documents. It is designed for startup initialization and
//! does not include the full retry/bridge infrastructure from canvas-server.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use canvas_core::SceneDocument;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use thiserror::Error;
use url::Url;

const JSONRPC_VERSION: &str = "2.0";

/// Errors that can occur when talking to the Communitas MCP server.
#[derive(Debug, Error)]
pub enum DesktopCommunitasError {
    /// The MCP base URL provided is invalid.
    #[error("invalid Communitas MCP URL: {0}")]
    InvalidUrl(String),
    /// HTTP layer failed (connection, timeout, etc.).
    #[error("Communitas MCP HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    /// JSON parsing failed.
    #[error("failed to parse Communitas MCP payload: {0}")]
    Json(#[from] serde_json::Error),
    /// The server returned an RPC error.
    #[error("Communitas MCP RPC error {code}: {message}")]
    Rpc {
        /// Error code defined by MCP.
        code: i32,
        /// Human readable error message.
        message: String,
    },
    /// The RPC response did not match the expected structure.
    #[error("unexpected Communitas MCP response: {0}")]
    UnexpectedResponse(String),
    /// Scene conversion failed.
    #[error("scene conversion failed: {0}")]
    SceneConversion(String),
}

/// Minimal Communitas MCP client for desktop scene fetching.
#[derive(Clone)]
pub struct DesktopMcpClient {
    inner: Arc<InnerClient>,
}

struct InnerClient {
    http: Client,
    endpoint: Url,
    request_id: AtomicU64,
}

impl DesktopMcpClient {
    /// Create a new Communitas MCP client.
    ///
    /// `base_url` may be either the MCP endpoint itself (`https://host:3040/mcp`)
    /// or just the host (in which case `/mcp` is appended automatically).
    ///
    /// # Errors
    ///
    /// Returns [`DesktopCommunitasError::InvalidUrl`] if the URL is malformed.
    /// Returns [`DesktopCommunitasError::Http`] if the HTTP client fails to build.
    pub fn new(base_url: &str) -> Result<Self, DesktopCommunitasError> {
        let mut url =
            Url::parse(base_url).map_err(|e| DesktopCommunitasError::InvalidUrl(e.to_string()))?;

        if url.path().is_empty() || url.path() == "/" {
            url.set_path("/mcp");
        }

        let http = Client::builder()
            .user_agent("canvas-desktop (saorsa-canvas)")
            // Disable proxy detection to avoid macOS system-configuration panic
            .no_proxy()
            .build()?;

        Ok(Self {
            inner: Arc::new(InnerClient {
                http,
                endpoint: url,
                request_id: AtomicU64::new(1),
            }),
        })
    }

    /// Perform MCP initialize handshake.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP request fails or the server returns an error.
    pub async fn initialize(&self) -> Result<InitializeResult, DesktopCommunitasError> {
        let params = InitializeParams {
            protocol_version: "2024-11-05".to_string(),
            capabilities: ClientCapabilities {},
            client_info: ClientInfo {
                name: "canvas-desktop".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
        };

        self.send_rpc("initialize", Some(serde_json::to_value(params)?))
            .await
    }

    /// Authenticate using a delegate token issued by Communitas.
    ///
    /// # Errors
    ///
    /// Returns an error if authentication fails.
    pub async fn authenticate(&self, token: &str) -> Result<(), DesktopCommunitasError> {
        self.call_tool("authenticate_token", Some(json!({ "token": token })))
            .await?;
        Ok(())
    }

    /// Fetch the scene document for a session.
    ///
    /// # Errors
    ///
    /// Returns an error if the fetch fails or the response cannot be parsed.
    pub async fn get_scene(
        &self,
        session: Option<&str>,
    ) -> Result<SceneDocument, DesktopCommunitasError> {
        let session_id = session.unwrap_or("default");
        let response = self
            .call_tool(
                "canvas_get_scene",
                Some(json!({ "session_id": session_id })),
            )
            .await?;

        Self::deserialize_scene(&response)
    }

    /// Call an MCP tool with optional arguments.
    async fn call_tool(
        &self,
        name: &str,
        arguments: Option<Value>,
    ) -> Result<Value, DesktopCommunitasError> {
        let params = json!({
            "name": name,
            "arguments": arguments.unwrap_or_else(|| json!({}))
        });
        self.send_rpc::<Value>("tools/call", Some(params)).await
    }

    fn deserialize_scene(value: &Value) -> Result<SceneDocument, DesktopCommunitasError> {
        // Try direct scene field
        if let Some(scene) = value.get("scene") {
            return Ok(serde_json::from_value(scene.clone())?);
        }

        // Try MCP content array format
        if let Some(content) = value.get("content").and_then(Value::as_array) {
            if let Some(first) = content.first() {
                if let Some(text) = first.get("text").and_then(Value::as_str) {
                    return serde_json::from_str(text).map_err(DesktopCommunitasError::from);
                }
            }
        }

        Err(DesktopCommunitasError::UnexpectedResponse(
            "response did not contain a scene document".to_string(),
        ))
    }

    async fn send_rpc<T>(
        &self,
        method: &str,
        params: Option<Value>,
    ) -> Result<T, DesktopCommunitasError>
    where
        for<'de> T: Deserialize<'de>,
    {
        let id = self.inner.request_id.fetch_add(1, Ordering::Relaxed);
        let request = JsonRpcRequest {
            jsonrpc: JSONRPC_VERSION,
            id,
            method,
            params,
        };

        let response = self
            .inner
            .http
            .post(self.inner.endpoint.clone())
            .json(&request)
            .send()
            .await?;

        let rpc: JsonRpcResponse = response.json().await?;

        if let Some(error) = rpc.error {
            return Err(DesktopCommunitasError::Rpc {
                code: error.code,
                message: error.message,
            });
        }

        let result = rpc
            .result
            .ok_or_else(|| DesktopCommunitasError::UnexpectedResponse("missing result".into()))?;

        Ok(serde_json::from_value(result)?)
    }
}

#[derive(Debug, Clone, Serialize)]
struct JsonRpcRequest<'a> {
    jsonrpc: &'a str,
    id: u64,
    method: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct JsonRpcResponse {
    #[allow(dead_code)]
    jsonrpc: String,
    #[serde(default)]
    result: Option<Value>,
    #[serde(default)]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Deserialize)]
struct JsonRpcError {
    code: i32,
    message: String,
}

#[derive(Debug, Clone, Serialize)]
struct InitializeParams {
    protocol_version: String,
    capabilities: ClientCapabilities,
    client_info: ClientInfo,
}

#[derive(Debug, Clone, Serialize)]
struct ClientCapabilities {}

#[derive(Debug, Clone, Serialize)]
struct ClientInfo {
    name: String,
    version: String,
}

/// Initialize result returned by Communitas MCP.
#[derive(Debug, Clone, Deserialize)]
pub struct InitializeResult {
    /// Protocol version negotiated.
    pub protocol_version: String,
    /// Server capabilities.
    pub capabilities: ServerCapabilities,
    /// Server info.
    pub server_info: ServerInfo,
}

/// Server capabilities from initialization.
#[derive(Debug, Clone, Deserialize)]
pub struct ServerCapabilities {
    /// Tools capability.
    #[serde(default)]
    pub tools: Option<ToolsCapability>,
}

/// Tools capability descriptor.
#[derive(Debug, Clone, Deserialize)]
pub struct ToolsCapability {
    /// Whether tool list changes are reported.
    #[serde(default)]
    pub list_changed: bool,
}

/// Server information.
#[derive(Debug, Clone, Deserialize)]
pub struct ServerInfo {
    /// Server name.
    pub name: String,
    /// Server version.
    pub version: String,
}
