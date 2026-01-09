//! # Saorsa Canvas MCP
//!
//! MCP (Model Context Protocol) tools and resources for the canvas.
//! Extends Communitas MCP with visual output capabilities.
//!
//! ## MCP Resources
//!
//! - `canvas://session/{id}` - A canvas session
//! - `canvas://chart/{type}` - Chart template
//! - `canvas://model/{id}` - 3D model resource
//!
//! ## MCP Tools
//!
//! - `canvas_render` - Render content to canvas
//! - `canvas_interact` - Handle touch/voice input
//! - `canvas_export` - Export canvas to image/PDF

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(clippy::all)]
#![deny(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]

pub mod resources;
pub mod server;
pub mod tools;

// Re-export key types for convenience
pub use server::{CanvasMcpServer, JsonRpcRequest, JsonRpcResponse};

use serde::{Deserialize, Serialize};

/// MCP tool response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResponse {
    /// Whether the operation succeeded.
    pub success: bool,
    /// Result data (if successful).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    /// Error message (if failed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl ToolResponse {
    /// Create a success response.
    #[must_use]
    pub fn success(data: serde_json::Value) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    /// Create an error response.
    #[must_use]
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(message.into()),
        }
    }
}

/// MCP resource content.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "content")]
pub enum ResourceContent {
    /// Text content (UTF-8).
    Text(String),
    /// Binary content (base64 encoded).
    Binary {
        /// Base64-encoded data.
        data: String,
        /// MIME type.
        mime_type: String,
    },
    /// JSON content.
    Json(serde_json::Value),
}
