//! API route handlers.

use axum::{extract::Json, response::IntoResponse};

use canvas_core::Scene;

/// Get the current scene.
pub async fn get_scene() -> impl IntoResponse {
    // TODO: Get from shared state
    let scene = Scene::new(800.0, 600.0);

    match scene.to_json() {
        Ok(json) => axum::response::Json(serde_json::from_str::<serde_json::Value>(&json).unwrap()),
        Err(e) => axum::response::Json(serde_json::json!({
            "error": e.to_string()
        })),
    }
}

/// Update the scene.
pub async fn update_scene(Json(payload): Json<serde_json::Value>) -> impl IntoResponse {
    tracing::debug!("Scene update: {:?}", payload);

    // TODO: Parse and apply scene update
    axum::response::Json(serde_json::json!({
        "status": "ok"
    }))
}
