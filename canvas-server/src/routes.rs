//! API route handlers for scene management.

use axum::{
    extract::{Path, State},
    http::{header, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};

use canvas_core::{ElementDocument, SceneDocument};
use canvas_renderer::export::{ExportConfig, ExportFormat, SceneExporter};

use crate::metrics::record_validation_failure;
use crate::sync::{current_timestamp, SyncOrigin};
use crate::validation::validate_session_id;
use crate::AppState;

/// Response for scene endpoints.
#[derive(Debug, Serialize)]
pub struct SceneResponse {
    /// Whether the operation succeeded.
    pub success: bool,
    /// Canonical scene document.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scene: Option<SceneDocument>,
    /// Error message if failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Request to update the scene.
#[derive(Debug, Deserialize)]
pub struct UpdateSceneRequest {
    /// Session ID to update (defaults to "default").
    #[serde(default = "default_session")]
    pub session_id: String,
    /// Elements to add.
    #[serde(default)]
    pub add: Vec<ElementDocument>,
    /// Element IDs to remove.
    #[serde(default)]
    pub remove: Vec<String>,
    /// Whether to clear all existing elements first.
    #[serde(default)]
    pub clear: bool,
}

fn default_session() -> String {
    "default".to_string()
}

/// Get the current scene for the default session.
pub async fn get_scene_handler(State(state): State<AppState>) -> impl IntoResponse {
    get_scene_for_session(&state, "default").await
}

/// Get the scene for a specific session.
pub async fn get_session_scene(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    if let Err(e) = validate_session_id(&session_id) {
        record_validation_failure("session_id");
        return (
            StatusCode::BAD_REQUEST,
            Json(SceneResponse {
                success: false,
                scene: None,
                error: Some(e.to_string()),
            }),
        )
            .into_response();
    }
    state.sync().record_access(&session_id);
    get_scene_for_session(&state, &session_id)
        .await
        .into_response()
}

/// Internal function to get scene for a session.
async fn get_scene_for_session(state: &AppState, session_id: &str) -> Json<SceneResponse> {
    if let Some(communitas) = state.communitas() {
        match communitas.fetch_scene(session_id).await {
            Ok(document) => {
                if let Ok(scene) = document.clone().into_scene() {
                    if let Err(err) =
                        state
                            .sync()
                            .replace_scene(session_id, scene, SyncOrigin::Remote)
                    {
                        tracing::warn!("Failed to cache Communitas scene: {}", err);
                    }
                }
                return Json(SceneResponse {
                    success: true,
                    scene: Some(document),
                    error: None,
                });
            }
            Err(err) => tracing::warn!("Communitas scene fetch failed: {}", err),
        }
    }

    let scene = state.sync().get_or_create_scene(session_id);
    let document = SceneDocument::from_scene(session_id, &scene, current_timestamp());

    Json(SceneResponse {
        success: true,
        scene: Some(document),
        error: None,
    })
}

/// Update the scene.
pub async fn update_scene_handler(
    State(state): State<AppState>,
    Json(request): Json<UpdateSceneRequest>,
) -> impl IntoResponse {
    tracing::debug!("Scene update request: {:?}", request);

    // Validate session_id
    if let Err(e) = validate_session_id(&request.session_id) {
        record_validation_failure("session_id");
        return Json(SceneResponse {
            success: false,
            scene: None,
            error: Some(e.to_string()),
        });
    }

    let sync = state.sync();
    let session_id = &request.session_id;

    // Clear if requested
    if request.clear {
        if let Err(e) = sync.update_scene(session_id, |scene| scene.clear()) {
            return Json(SceneResponse {
                success: false,
                scene: None,
                error: Some(e.to_string()),
            });
        }
    }

    // Remove elements
    for id in &request.remove {
        if let Err(e) = sync.remove_element(session_id, id) {
            tracing::warn!("Failed to remove element {}: {}", id, e);
        }
    }

    // Add elements
    for element in &request.add {
        if let Err(e) = sync.add_element(session_id, element) {
            tracing::warn!("Failed to add element: {}", e);
        }
    }

    if let Some(client) = state.communitas() {
        let document = sync.scene_document(session_id);
        if let Err(err) = client.push_scene(&document).await {
            tracing::warn!("Failed to push scene to Communitas: {}", err);
        }
    }

    // Return updated scene
    get_scene_for_session(&state, session_id).await
}

/// Request body for the export endpoint.
#[derive(Debug, Deserialize)]
pub struct ExportRequest {
    /// Session ID to export.
    pub session_id: String,
    /// Output format: "png", "jpeg", "svg", "pdf".
    pub format: String,
    /// Optional width override (pixels).
    pub width: Option<u32>,
    /// Optional height override (pixels).
    pub height: Option<u32>,
    /// DPI for print export (default 96).
    pub dpi: Option<f32>,
    /// JPEG quality 1-100 (default 85).
    pub quality: Option<u8>,
    /// Scale factor (default 1.0).
    pub scale: Option<f32>,
}

/// Export a session's scene to an image/document format.
pub async fn export_scene_handler(
    State(state): State<AppState>,
    Json(request): Json<ExportRequest>,
) -> impl IntoResponse {
    // Validate session ID
    if let Err(e) = validate_session_id(&request.session_id) {
        record_validation_failure("session_id");
        return (
            StatusCode::BAD_REQUEST,
            [(header::CONTENT_TYPE, "application/json")],
            serde_json::json!({"success": false, "error": e.to_string()})
                .to_string()
                .into_bytes(),
        )
            .into_response();
    }

    // Parse format
    let format = match request.format.as_str() {
        "png" => ExportFormat::Png,
        "jpeg" | "jpg" => ExportFormat::Jpeg,
        "svg" => ExportFormat::Svg,
        "pdf" => ExportFormat::Pdf,
        other => {
            return (
                StatusCode::BAD_REQUEST,
                [(header::CONTENT_TYPE, "application/json")],
                serde_json::json!({"success": false, "error": format!("Unsupported format: {other}")})
                    .to_string()
                    .into_bytes(),
            )
                .into_response();
        }
    };

    // Get the scene
    let sync = state.sync();
    let scene = match sync.store().get(&request.session_id) {
        Some(scene) => scene,
        None => {
            return (
                StatusCode::NOT_FOUND,
                [(header::CONTENT_TYPE, "application/json")],
                serde_json::json!({"success": false, "error": "Session not found"})
                    .to_string()
                    .into_bytes(),
            )
                .into_response();
        }
    };

    // Configure exporter
    let config = ExportConfig {
        width: request.width,
        height: request.height,
        dpi: request.dpi.unwrap_or(96.0),
        jpeg_quality: request.quality.unwrap_or(85),
        scale: request.scale.unwrap_or(1.0),
        ..Default::default()
    };

    let exporter = SceneExporter::new(config);

    // Export
    match exporter.export(&scene, format) {
        Ok(data) => {
            let content_type = match format {
                ExportFormat::Png => "image/png",
                ExportFormat::Jpeg => "image/jpeg",
                ExportFormat::Svg => "image/svg+xml",
                ExportFormat::Pdf => "application/pdf",
            };

            (StatusCode::OK, [(header::CONTENT_TYPE, content_type)], data).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            [(header::CONTENT_TYPE, "application/json")],
            serde_json::json!({"success": false, "error": format!("Export failed: {e}")})
                .to_string()
                .into_bytes(),
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::communitas::{ClientDescriptor, CommunitasMcpClient};
    use crate::sync::SyncState;
    use canvas_core::ViewportDocument;
    use canvas_mcp::CanvasMcpServer;
    use std::sync::Arc;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn test_state_with_communitas(client: CommunitasMcpClient) -> AppState {
        let sync = SyncState::new();
        AppState {
            mcp: Arc::new(CanvasMcpServer::new(sync.store())),
            sync,
            communitas: Some(client),
        }
    }

    #[test]
    fn test_update_scene_request_defaults() {
        let json = r#"{"add": []}"#;
        let request: UpdateSceneRequest = serde_json::from_str(json).expect("should parse");
        assert_eq!(request.session_id, "default");
        assert!(!request.clear);
        assert!(request.remove.is_empty());
    }

    #[test]
    fn test_update_scene_request_with_session() {
        let json = r#"{"session_id": "custom", "clear": true}"#;
        let request: UpdateSceneRequest = serde_json::from_str(json).expect("should parse");
        assert_eq!(request.session_id, "custom");
        assert!(request.clear);
    }

    #[test]
    fn test_scene_response_serialization() {
        let response = SceneResponse {
            success: true,
            scene: Some(SceneDocument {
                session_id: "default".to_string(),
                viewport: ViewportDocument {
                    width: 800.0,
                    height: 600.0,
                    zoom: 1.0,
                    pan_x: 0.0,
                    pan_y: 0.0,
                },
                elements: vec![],
                timestamp: 0,
            }),
            error: None,
        };

        let json = serde_json::to_string(&response).expect("should serialize");
        assert!(json.contains("success"));
        assert!(json.contains("800"));
        assert!(!json.contains("error")); // Skip serializing None
    }

    #[test]
    fn test_scene_response_with_error() {
        let response = SceneResponse {
            success: false,
            scene: None,
            error: Some("Something went wrong".to_string()),
        };

        let json = serde_json::to_string(&response).expect("should serialize");
        assert!(json.contains("false"));
        assert!(json.contains("Something went wrong"));
    }

    #[test]
    fn test_session_id_validation_rejects_invalid_chars() {
        use crate::validation::validate_session_id;
        // Path traversal attempt
        assert!(validate_session_id("../../../etc/passwd").is_err());
        // Spaces not allowed
        assert!(validate_session_id("my session").is_err());
        // HTML/script injection
        assert!(validate_session_id("<script>").is_err());
        // Empty not allowed
        assert!(validate_session_id("").is_err());
    }

    #[test]
    fn test_session_id_validation_accepts_valid() {
        use crate::validation::validate_session_id;
        assert!(validate_session_id("default").is_ok());
        assert!(validate_session_id("my-session").is_ok());
        assert!(validate_session_id("session_123").is_ok());
        assert!(validate_session_id("ABC-xyz_123").is_ok());
    }

    #[tokio::test]
    async fn test_get_scene_uses_communitas_when_available() {
        let server = MockServer::start().await;
        let document = SceneDocument {
            session_id: "default".into(),
            viewport: ViewportDocument {
                width: 640.0,
                height: 480.0,
                zoom: 1.0,
                pan_x: 0.0,
                pan_y: 0.0,
            },
            elements: vec![],
            timestamp: 123,
        };

        Mock::given(method("POST"))
            .and(path("/mcp"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": { "scene": document }
            })))
            .mount(&server)
            .await;

        let client = CommunitasMcpClient::new(
            server.uri(),
            ClientDescriptor {
                name: "test".into(),
                version: "1.0".into(),
            },
        )
        .expect("client");

        let state = test_state_with_communitas(client);
        let Json(response) = get_scene_for_session(&state, "default").await;
        assert_eq!(response.scene.unwrap().timestamp, 123);
    }
}
