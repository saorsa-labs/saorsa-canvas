//! MCP tools for canvas operations.

use serde::{Deserialize, Serialize};

use crate::ToolResponse;

/// Parameters for the `canvas_render` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderParams {
    /// Session ID for the canvas.
    pub session_id: String,
    /// Content to render.
    pub content: RenderContent,
    /// Position (optional, defaults to auto-layout).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<Position>,
}

/// Content that can be rendered to the canvas.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum RenderContent {
    /// A chart.
    Chart {
        /// Chart type (bar, line, pie, etc.).
        chart_type: String,
        /// Chart data.
        data: serde_json::Value,
        /// Chart title.
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<String>,
    },
    /// An image.
    Image {
        /// Image source (URL or base64).
        src: String,
        /// Alt text.
        #[serde(skip_serializing_if = "Option::is_none")]
        alt: Option<String>,
    },
    /// A 3D model.
    Model3D {
        /// glTF source URL.
        src: String,
        /// Initial rotation.
        #[serde(skip_serializing_if = "Option::is_none")]
        rotation: Option<[f32; 3]>,
    },
    /// Text/annotation.
    Text {
        /// Text content.
        content: String,
        /// Font size.
        #[serde(skip_serializing_if = "Option::is_none")]
        font_size: Option<f32>,
    },
}

/// Position specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    /// X coordinate.
    pub x: f32,
    /// Y coordinate.
    pub y: f32,
    /// Width.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<f32>,
    /// Height.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<f32>,
}

/// Execute the `canvas_render` tool.
///
/// # Errors
///
/// Returns an error if rendering fails.
pub fn canvas_render(params: &RenderParams) -> ToolResponse {
    tracing::info!(
        "Rendering to session {}: {:?}",
        params.session_id,
        params.content
    );

    // TODO: Actually render to the canvas session

    ToolResponse::success(serde_json::json!({
        "session_id": &params.session_id,
        "element_id": uuid::Uuid::new_v4().to_string(),
        "rendered": true
    }))
}

/// Parameters for the `canvas_interact` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractParams {
    /// Session ID.
    pub session_id: String,
    /// Interaction type.
    pub interaction: Interaction,
}

/// Types of interactions.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum Interaction {
    /// Touch at a point.
    Touch {
        /// X coordinate.
        x: f32,
        /// Y coordinate.
        y: f32,
        /// Element ID that was touched.
        #[serde(skip_serializing_if = "Option::is_none")]
        element_id: Option<String>,
    },
    /// Voice command.
    Voice {
        /// Transcribed text.
        transcript: String,
        /// Element context (if any).
        #[serde(skip_serializing_if = "Option::is_none")]
        context_element: Option<String>,
    },
    /// Selection change.
    Select {
        /// Selected element IDs.
        element_ids: Vec<String>,
    },
}

/// Execute the `canvas_interact` tool.
pub fn canvas_interact(params: InteractParams) -> ToolResponse {
    tracing::info!(
        "Interaction on session {}: {:?}",
        params.session_id,
        params.interaction
    );

    // TODO: Process the interaction and return AI-friendly response

    ToolResponse::success(serde_json::json!({
        "session_id": params.session_id,
        "acknowledged": true,
        "interpretation": match params.interaction {
            Interaction::Touch { x, y, element_id } => {
                serde_json::json!({
                    "type": "touch",
                    "location": {"x": x, "y": y},
                    "element": element_id
                })
            },
            Interaction::Voice { transcript, context_element } => {
                serde_json::json!({
                    "type": "voice",
                    "transcript": transcript,
                    "context": context_element
                })
            },
            Interaction::Select { element_ids } => {
                serde_json::json!({
                    "type": "selection",
                    "elements": element_ids
                })
            }
        }
    }))
}

/// Parameters for the `canvas_export` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportParams {
    /// Session ID.
    pub session_id: String,
    /// Export format.
    pub format: ExportFormat,
    /// Quality (0-100, for lossy formats).
    #[serde(default = "default_quality")]
    pub quality: u8,
}

fn default_quality() -> u8 {
    90
}

/// Export formats.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    /// PNG image.
    Png,
    /// JPEG image.
    Jpeg,
    /// SVG vector.
    Svg,
    /// PDF document.
    Pdf,
    /// WebP image.
    WebP,
}

/// Execute the `canvas_export` tool.
pub fn canvas_export(params: &ExportParams) -> ToolResponse {
    tracing::info!(
        "Exporting session {} as {:?}",
        params.session_id,
        params.format
    );

    // TODO: Actually export the canvas

    ToolResponse::success(serde_json::json!({
        "session_id": &params.session_id,
        "format": params.format,
        "data": "base64_encoded_data_here",
        "mime_type": match params.format {
            ExportFormat::Png => "image/png",
            ExportFormat::Jpeg => "image/jpeg",
            ExportFormat::Svg => "image/svg+xml",
            ExportFormat::Pdf => "application/pdf",
            ExportFormat::WebP => "image/webp",
        }
    }))
}
