//! WebSocket round-trip integration tests.
//!
//! Tests real WebSocket connections to verify message flow.

mod common;

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::time::Duration;
use tokio::time::timeout;
use tokio_tungstenite::{connect_async, tungstenite::Message};

use common::TestServer;

/// Helper to receive and parse a JSON message with timeout.
async fn recv_json(
    stream: &mut (impl StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin),
) -> Option<Value> {
    let msg = timeout(Duration::from_secs(5), stream.next())
        .await
        .ok()??
        .ok()?;

    match msg {
        Message::Text(text) => serde_json::from_str(&text).ok(),
        _ => None,
    }
}

/// Helper to receive multiple messages and find one by type.
async fn recv_until_type(
    stream: &mut (impl StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin),
    msg_type: &str,
    max_messages: usize,
) -> (Option<Value>, Vec<Value>) {
    let mut buffer = Vec::new();
    for _ in 0..max_messages {
        if let Some(msg) = recv_json(stream).await {
            if msg["type"] == msg_type {
                return (Some(msg), buffer);
            }
            buffer.push(msg);
        } else {
            break;
        }
    }
    (None, buffer)
}

/// Helper to send a JSON message.
async fn send_json<S>(sink: &mut S, value: &Value) -> Result<(), String>
where
    S: SinkExt<Message> + Unpin,
{
    let text = serde_json::to_string(value).map_err(|e| e.to_string())?;
    sink.send(Message::Text(text))
        .await
        .map_err(|_| "send failed".to_string())
}

#[tokio::test]
async fn connect_and_receive_welcome() {
    let server = TestServer::start().await;

    let (ws_stream, _) = connect_async(&server.ws_url())
        .await
        .expect("Failed to connect");

    let (_, mut read) = ws_stream.split();

    // First message should be Welcome
    let msg = recv_json(&mut read).await.expect("No welcome message");

    assert_eq!(msg["type"], "welcome");
    assert!(msg["version"].is_string());
    assert!(msg["session_id"].is_string());

    server.shutdown().await;
}

#[tokio::test]
async fn subscribe_and_receive_scene() {
    let server = TestServer::start().await;

    let (ws_stream, _) = connect_async(&server.ws_url())
        .await
        .expect("Failed to connect");

    let (mut write, mut read) = ws_stream.split();

    // Skip welcome and peer_assigned messages
    let _ = recv_until_type(&mut read, "peer_assigned", 3).await;

    // Subscribe to default session
    send_json(
        &mut write,
        &json!({
            "type": "subscribe",
            "session_id": "default"
        }),
    )
    .await
    .expect("Failed to send subscribe");

    // Should receive scene update (use recv_until_type in case of other messages)
    let (msg_opt, _) = recv_until_type(&mut read, "scene_update", 5).await;
    let msg = msg_opt.expect("No scene update");

    assert_eq!(msg["type"], "scene_update");
    assert!(msg["scene"].is_object());
    assert!(msg["scene"]["elements"].is_array());

    server.shutdown().await;
}

#[tokio::test]
async fn add_element_round_trip() {
    let server = TestServer::start().await;

    let (ws_stream, _) = connect_async(&server.ws_url())
        .await
        .expect("Failed to connect");

    let (mut write, mut read) = ws_stream.split();

    // Skip welcome
    let _ = recv_json(&mut read).await;

    // Subscribe first
    send_json(
        &mut write,
        &json!({
            "type": "subscribe",
            "session_id": "default"
        }),
    )
    .await
    .expect("Failed to send subscribe");

    // Skip initial scene update
    let _ = recv_json(&mut read).await;

    // Add an element
    send_json(
        &mut write,
        &json!({
            "type": "add_element",
            "element": {
                "id": "",
                "kind": {
                    "type": "Text",
                    "data": {
                        "content": "Hello Integration Test",
                        "font_size": 24.0,
                        "color": "#ff0000"
                    }
                },
                "transform": {
                    "x": 100.0,
                    "y": 200.0,
                    "width": 300.0,
                    "height": 50.0,
                    "rotation": 0.0,
                    "z_index": 0
                }
            },
            "message_id": "test-add-001"
        }),
    )
    .await
    .expect("Failed to send add_element");

    // Find ack message (may arrive before or after scene_update)
    let (ack_opt, _) = recv_until_type(&mut read, "ack", 5).await;
    let ack = ack_opt.expect("No ack received");

    assert_eq!(ack["message_id"], "test-add-001");
    assert_eq!(ack["success"], true);

    // Element ID should be in result
    let element_id = ack["result"]["id"].as_str().expect("No element ID in ack");
    assert!(!element_id.is_empty());

    // The scene_update with the new element comes AFTER the ack (from broadcast)
    // Keep receiving until we find a scene_update containing our element
    let mut found = false;
    for _ in 0..5 {
        if let Some(msg) = recv_json(&mut read).await {
            if msg["type"] == "scene_update" {
                if let Some(elements) = msg["scene"]["elements"].as_array() {
                    if elements.iter().any(|e| e["id"] == element_id) {
                        found = true;
                        break;
                    }
                }
            }
        } else {
            break;
        }
    }
    assert!(found, "Added element not found in scene update");

    server.shutdown().await;
}

#[tokio::test]
async fn remove_element_round_trip() {
    let server = TestServer::start().await;

    let (ws_stream, _) = connect_async(&server.ws_url())
        .await
        .expect("Failed to connect");

    let (mut write, mut read) = ws_stream.split();

    // Skip welcome
    let _ = recv_json(&mut read).await;

    // Subscribe
    send_json(
        &mut write,
        &json!({
            "type": "subscribe",
            "session_id": "default"
        }),
    )
    .await
    .unwrap();

    // Skip initial scene
    let _ = recv_json(&mut read).await;

    // Add element first
    send_json(
        &mut write,
        &json!({
            "type": "add_element",
            "element": {
                "id": "",
                "kind": {
                    "type": "Text",
                    "data": {
                        "content": "To Be Deleted",
                        "font_size": 16.0,
                        "color": "#000000"
                    }
                }
            },
            "message_id": "test-add-002"
        }),
    )
    .await
    .unwrap();

    // Get the ack with element ID
    let (ack_opt, _) = recv_until_type(&mut read, "ack", 5).await;
    let ack = ack_opt.expect("No ack for add");
    let element_id = ack["result"]["id"]
        .as_str()
        .expect("No element ID")
        .to_string();

    // Drain any remaining scene updates
    let _ = recv_until_type(&mut read, "nonexistent", 2).await;

    // Now remove the element
    send_json(
        &mut write,
        &json!({
            "type": "remove_element",
            "id": element_id,
            "message_id": "test-remove-001"
        }),
    )
    .await
    .unwrap();

    // Should receive ack for removal
    let (remove_ack_opt, _) = recv_until_type(&mut read, "ack", 5).await;
    let remove_ack = remove_ack_opt.expect("No remove ack");

    assert_eq!(remove_ack["message_id"], "test-remove-001");
    assert_eq!(remove_ack["success"], true);

    // The server broadcasts element_removed (not scene_update) after removal
    // Keep receiving until we find the element_removed broadcast
    let mut removed = false;
    for _ in 0..5 {
        if let Some(msg) = recv_json(&mut read).await {
            // Accept either element_removed with our ID, or scene_update without it
            if msg["type"] == "element_removed" && msg["id"] == element_id {
                removed = true;
                break;
            } else if msg["type"] == "scene_update" {
                if let Some(elements) = msg["scene"]["elements"].as_array() {
                    if !elements.iter().any(|e| e["id"] == element_id) {
                        removed = true;
                        break;
                    }
                }
            }
        } else {
            break;
        }
    }
    assert!(removed, "Element should have been removed from scene");

    server.shutdown().await;
}

#[tokio::test]
async fn ping_pong() {
    let server = TestServer::start().await;

    let (ws_stream, _) = connect_async(&server.ws_url())
        .await
        .expect("Failed to connect");

    let (mut write, mut read) = ws_stream.split();

    // Skip welcome
    let _ = recv_json(&mut read).await;

    // Send ping
    send_json(
        &mut write,
        &json!({
            "type": "ping"
        }),
    )
    .await
    .expect("Failed to send ping");

    // Find pong (may have scene updates in between)
    let (pong_opt, _) = recv_until_type(&mut read, "pong", 5).await;
    let pong = pong_opt.expect("No pong received");

    assert!(pong["timestamp"].is_number());

    server.shutdown().await;
}

#[tokio::test]
async fn get_scene_returns_current_state() {
    let server = TestServer::start().await;

    let (ws_stream, _) = connect_async(&server.ws_url())
        .await
        .expect("Failed to connect");

    let (mut write, mut read) = ws_stream.split();

    // Skip welcome
    let _ = recv_json(&mut read).await;

    // Subscribe and add an element
    send_json(
        &mut write,
        &json!({
            "type": "subscribe",
            "session_id": "test-session"
        }),
    )
    .await
    .unwrap();
    let _ = recv_json(&mut read).await; // initial scene

    send_json(
        &mut write,
        &json!({
            "type": "add_element",
            "session_id": "test-session",
            "element": {
                "id": "",
                "kind": {
                    "type": "Text",
                    "data": {
                        "content": "Persisted Element",
                        "font_size": 14.0,
                        "color": "#333333"
                    }
                }
            },
            "message_id": "add-for-get"
        }),
    )
    .await
    .unwrap();

    // Drain ack and scene updates
    let _ = recv_until_type(&mut read, "nonexistent", 3).await;

    // Now request the scene explicitly
    send_json(
        &mut write,
        &json!({
            "type": "get_scene",
            "session_id": "test-session"
        }),
    )
    .await
    .unwrap();

    let (scene_opt, _) = recv_until_type(&mut read, "scene_update", 5).await;
    let scene = scene_opt.expect("No scene response");

    let elements = scene["scene"]["elements"].as_array().unwrap();
    let found = elements
        .iter()
        .any(|e| e["kind"]["data"]["content"] == "Persisted Element");
    assert!(
        found,
        "Previously added element should be in get_scene response"
    );

    server.shutdown().await;
}
