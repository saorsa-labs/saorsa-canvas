//! Integration tests for the POST /api/export endpoint.
//!
//! Tests export of scenes to PNG, JPEG, SVG, and PDF via the canvas-server
//! HTTP API. Uses the shared TestServer harness.

mod common;

use canvas_core::element::{Element, ElementKind, Transform};
use common::TestServer;

/// Helper to create a text element at a given position.
fn text_element(content: &str, x: f32, y: f32) -> Element {
    Element::new(ElementKind::Text {
        content: content.to_string(),
        font_size: 16.0,
        color: "#000000".to_string(),
    })
    .with_transform(Transform {
        x,
        y,
        width: 200.0,
        height: 30.0,
        rotation: 0.0,
        z_index: 0,
    })
}

/// Seed a session with some elements via the sync store.
fn seed_session(server: &TestServer, session_id: &str) {
    let store = server.sync_state().store();
    let _ = store.get_or_create(session_id);
    store
        .update(session_id, |scene| {
            scene.add_element(text_element("Hello export", 10.0, 20.0));
            scene.add_element(text_element("Second line", 10.0, 50.0));
        })
        .expect("seed session");
}

// ==========================================================================
// Success cases
// ==========================================================================

#[tokio::test]
async fn test_export_png_returns_valid_image() {
    let server = TestServer::start().await;
    seed_session(&server, "test-export");

    let client = reqwest::Client::new();
    let resp = client
        .post(server.export_url())
        .json(&serde_json::json!({
            "session_id": "test-export",
            "format": "png"
        }))
        .send()
        .await
        .expect("request");

    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok()),
        Some("image/png")
    );

    let bytes = resp.bytes().await.expect("body");
    // PNG magic bytes: 0x89 P N G
    assert!(bytes.len() > 8, "PNG too small: {} bytes", bytes.len());
    assert_eq!(&bytes[0..4], &[137, 80, 78, 71]);

    server.shutdown().await;
}

#[tokio::test]
async fn test_export_jpeg_returns_valid_image() {
    let server = TestServer::start().await;
    seed_session(&server, "test-jpeg");

    let client = reqwest::Client::new();
    let resp = client
        .post(server.export_url())
        .json(&serde_json::json!({
            "session_id": "test-jpeg",
            "format": "jpeg",
            "quality": 75
        }))
        .send()
        .await
        .expect("request");

    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok()),
        Some("image/jpeg")
    );

    let bytes = resp.bytes().await.expect("body");
    // JPEG magic bytes: 0xFF 0xD8
    assert!(bytes.len() > 2, "JPEG too small: {} bytes", bytes.len());
    assert_eq!(bytes[0], 0xFF);
    assert_eq!(bytes[1], 0xD8);

    server.shutdown().await;
}

#[tokio::test]
async fn test_export_svg_returns_valid_xml() {
    let server = TestServer::start().await;
    seed_session(&server, "test-svg");

    let client = reqwest::Client::new();
    let resp = client
        .post(server.export_url())
        .json(&serde_json::json!({
            "session_id": "test-svg",
            "format": "svg"
        }))
        .send()
        .await
        .expect("request");

    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok()),
        Some("image/svg+xml")
    );

    let body = resp.text().await.expect("body");
    assert!(body.starts_with("<svg"), "SVG should start with <svg tag");
    assert!(body.ends_with("</svg>"), "SVG should end with </svg>");
    assert!(body.contains("Hello export"), "SVG should contain text");

    server.shutdown().await;
}

#[tokio::test]
async fn test_export_pdf_returns_valid_document() {
    let server = TestServer::start().await;
    seed_session(&server, "test-pdf");

    let client = reqwest::Client::new();
    let resp = client
        .post(server.export_url())
        .json(&serde_json::json!({
            "session_id": "test-pdf",
            "format": "pdf"
        }))
        .send()
        .await
        .expect("request");

    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok()),
        Some("application/pdf")
    );

    let bytes = resp.bytes().await.expect("body");
    // PDF header: %PDF-
    assert!(bytes.len() > 5, "PDF too small: {} bytes", bytes.len());
    assert_eq!(&bytes[0..5], b"%PDF-");

    server.shutdown().await;
}

#[tokio::test]
async fn test_export_jpg_alias_works() {
    let server = TestServer::start().await;
    seed_session(&server, "test-jpg-alias");

    let client = reqwest::Client::new();
    let resp = client
        .post(server.export_url())
        .json(&serde_json::json!({
            "session_id": "test-jpg-alias",
            "format": "jpg"
        }))
        .send()
        .await
        .expect("request");

    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok()),
        Some("image/jpeg")
    );

    server.shutdown().await;
}

// ==========================================================================
// Error cases
// ==========================================================================

#[tokio::test]
async fn test_export_missing_session_returns_404() {
    let server = TestServer::start().await;

    let client = reqwest::Client::new();
    let resp = client
        .post(server.export_url())
        .json(&serde_json::json!({
            "session_id": "nonexistent",
            "format": "png"
        }))
        .send()
        .await
        .expect("request");

    assert_eq!(resp.status(), 404);

    let body: serde_json::Value = resp.json().await.expect("json");
    assert_eq!(body["success"], false);
    assert!(body["error"]
        .as_str()
        .unwrap_or_default()
        .contains("not found"));

    server.shutdown().await;
}

#[tokio::test]
async fn test_export_invalid_format_returns_400() {
    let server = TestServer::start().await;
    seed_session(&server, "test-bad-format");

    let client = reqwest::Client::new();
    let resp = client
        .post(server.export_url())
        .json(&serde_json::json!({
            "session_id": "test-bad-format",
            "format": "bmp"
        }))
        .send()
        .await
        .expect("request");

    assert_eq!(resp.status(), 400);

    let body: serde_json::Value = resp.json().await.expect("json");
    assert_eq!(body["success"], false);
    assert!(body["error"]
        .as_str()
        .unwrap_or_default()
        .contains("Unsupported format"));

    server.shutdown().await;
}

#[tokio::test]
async fn test_export_invalid_session_id_returns_400() {
    let server = TestServer::start().await;

    let client = reqwest::Client::new();
    let resp = client
        .post(server.export_url())
        .json(&serde_json::json!({
            "session_id": "../../../etc/passwd",
            "format": "png"
        }))
        .send()
        .await
        .expect("request");

    assert_eq!(resp.status(), 400);

    let body: serde_json::Value = resp.json().await.expect("json");
    assert_eq!(body["success"], false);

    server.shutdown().await;
}

// ==========================================================================
// Configuration options
// ==========================================================================

#[tokio::test]
async fn test_export_with_custom_dimensions() {
    let server = TestServer::start().await;
    seed_session(&server, "test-dims");

    let client = reqwest::Client::new();
    let resp = client
        .post(server.export_url())
        .json(&serde_json::json!({
            "session_id": "test-dims",
            "format": "svg",
            "width": 400,
            "height": 300
        }))
        .send()
        .await
        .expect("request");

    assert_eq!(resp.status(), 200);

    let body = resp.text().await.expect("body");
    assert!(body.contains("width=\"400\""));
    assert!(body.contains("height=\"300\""));

    server.shutdown().await;
}

#[tokio::test]
async fn test_export_with_scale_factor() {
    let server = TestServer::start().await;
    seed_session(&server, "test-scale");

    let client = reqwest::Client::new();
    let resp = client
        .post(server.export_url())
        .json(&serde_json::json!({
            "session_id": "test-scale",
            "format": "svg",
            "scale": 2.0
        }))
        .send()
        .await
        .expect("request");

    assert_eq!(resp.status(), 200);

    let body = resp.text().await.expect("body");
    // Default scene is 800x600; at 2x scale output should be 1600x1200
    assert!(body.contains("width=\"1600\""));
    assert!(body.contains("height=\"1200\""));

    server.shutdown().await;
}

// ==========================================================================
// Large scene
// ==========================================================================

#[tokio::test]
async fn test_export_large_scene() {
    let server = TestServer::start().await;
    let store = server.sync_state().store();
    let _ = store.get_or_create("large-scene");
    store
        .update("large-scene", |scene| {
            for i in 0..100 {
                #[allow(clippy::cast_precision_loss)]
                let y = (i as f32) * 20.0;
                scene.add_element(text_element(&format!("Line {i}"), 10.0, y));
            }
        })
        .expect("seed large scene");

    let client = reqwest::Client::new();
    let resp = client
        .post(server.export_url())
        .json(&serde_json::json!({
            "session_id": "large-scene",
            "format": "png"
        }))
        .send()
        .await
        .expect("request");

    assert_eq!(resp.status(), 200);

    let bytes = resp.bytes().await.expect("body");
    // PNG should be valid and reasonably sized
    assert!(bytes.len() > 100, "Large scene PNG too small");
    assert_eq!(&bytes[0..4], &[137, 80, 78, 71]);

    server.shutdown().await;
}
