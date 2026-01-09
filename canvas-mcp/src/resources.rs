//! MCP resources for canvas sessions and content.

use serde::{Deserialize, Serialize};

use crate::ResourceContent;

/// A canvas session resource.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasSession {
    /// Unique session ID.
    pub id: String,
    /// Session name/title.
    pub name: String,
    /// Creation timestamp (ISO 8601).
    pub created_at: String,
    /// Last modified timestamp.
    pub modified_at: String,
    /// Viewport width.
    pub width: f32,
    /// Viewport height.
    pub height: f32,
    /// Number of elements in the session.
    pub element_count: usize,
}

/// Resource URI schemes supported by the canvas.
pub mod uri {
    /// Parse a canvas URI.
    ///
    /// Supported formats:
    /// - `canvas://session/{id}` - A canvas session
    /// - `canvas://chart/{type}` - Chart template
    /// - `canvas://model/{id}` - 3D model
    #[must_use]
    pub fn parse(uri: &str) -> Option<CanvasUri> {
        if !uri.starts_with("canvas://") {
            return None;
        }

        let path = &uri[9..]; // Skip "canvas://"
        let parts: Vec<&str> = path.split('/').collect();

        match parts.as_slice() {
            ["session", id] => Some(CanvasUri::Session((*id).to_string())),
            ["chart", chart_type] => Some(CanvasUri::ChartTemplate((*chart_type).to_string())),
            ["model", id] => Some(CanvasUri::Model((*id).to_string())),
            _ => None,
        }
    }

    /// Parsed canvas URI.
    #[derive(Debug, Clone)]
    pub enum CanvasUri {
        /// A canvas session.
        Session(String),
        /// A chart template.
        ChartTemplate(String),
        /// A 3D model.
        Model(String),
    }
}

/// Get a resource by URI.
///
/// # Errors
///
/// Returns an error if the resource is not found.
pub fn get_resource(uri: &str) -> Result<ResourceContent, String> {
    let parsed = uri::parse(uri).ok_or_else(|| format!("Invalid canvas URI: {uri}"))?;

    match parsed {
        uri::CanvasUri::Session(id) => {
            // TODO: Look up actual session
            Ok(ResourceContent::Json(serde_json::json!({
                "id": id,
                "name": "New Session",
                "elements": []
            })))
        }
        uri::CanvasUri::ChartTemplate(chart_type) => Ok(ResourceContent::Json(serde_json::json!({
            "type": chart_type,
            "template": {
                "title": "",
                "x_label": "",
                "y_label": "",
                "data": []
            }
        }))),
        uri::CanvasUri::Model(id) => {
            // TODO: Return actual model data
            Err(format!("Model {id} not found"))
        }
    }
}

/// List available resources.
#[must_use]
pub fn list_resources() -> Vec<String> {
    vec![
        "canvas://session/default".to_string(),
        "canvas://chart/bar".to_string(),
        "canvas://chart/line".to_string(),
        "canvas://chart/pie".to_string(),
    ]
}
