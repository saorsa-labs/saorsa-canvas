//! Multi-client synchronization and MCP broadcast integration tests.
//!
//! Tests real WebSocket connections with multiple clients to verify broadcast behavior.

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

/// Helper to receive messages until a specific type or timeout.
async fn recv_until_type(
    stream: &mut (impl StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin),
    msg_type: &str,
    max_messages: usize,
) -> Option<Value> {
    for _ in 0..max_messages {
        if let Some(msg) = recv_json(stream).await {
            if msg["type"] == msg_type {
                return Some(msg);
            }
        } else {
            break;
        }
    }
    None
}

#[tokio::test]
async fn two_clients_receive_same_broadcast() {
    let server = TestServer::start().await;

    // Connect two clients
    let (ws1, _) = connect_async(&server.ws_url())
        .await
        .expect("Client 1 failed to connect");
    let (ws2, _) = connect_async(&server.ws_url())
        .await
        .expect("Client 2 failed to connect");

    let (mut write1, mut read1) = ws1.split();
    let (mut write2, mut read2) = ws2.split();

    // Skip all 4 initial messages: welcome, peer_assigned, initial_scene, call_state
    for _ in 0..4 {
        let _ = recv_json(&mut read1).await;
    }
    for _ in 0..4 {
        let _ = recv_json(&mut read2).await;
    }

    // Both clients subscribe to the same session
    send_json(
        &mut write1,
        &json!({
            "type": "subscribe",
            "session_id": "shared-session"
        }),
    )
    .await
    .unwrap();
    send_json(
        &mut write2,
        &json!({
            "type": "subscribe",
            "session_id": "shared-session"
        }),
    )
    .await
    .unwrap();

    // Skip initial scene updates
    let _ = recv_json(&mut read1).await;
    let _ = recv_json(&mut read2).await;

    // Client 1 adds an element
    send_json(
        &mut write1,
        &json!({
            "type": "add_element",
            "element": {
                "id": "",
                "kind": {
                    "type": "Text",
                    "data": {
                        "content": "Shared Content",
                        "font_size": 20.0,
                        "color": "#00ff00"
                    }
                }
            },
            "message_id": "client1-add"
        }),
    )
    .await
    .unwrap();

    // Client 1 should receive ack
    let ack1 = recv_until_type(&mut read1, "ack", 5)
        .await
        .expect("Client 1 should receive ack");
    assert_eq!(ack1["message_id"], "client1-add");
    let element_id = ack1["result"]["id"].as_str().expect("No element ID");

    // Both clients should eventually receive a scene_update or element_added
    // containing the new element
    let mut client1_saw_element = false;
    let mut client2_saw_element = false;

    // Check client 1 receives the broadcast (more retries for CI reliability)
    for _ in 0..10 {
        if let Some(msg) = recv_json(&mut read1).await {
            if msg["type"] == "scene_update" {
                if let Some(elements) = msg["scene"]["elements"].as_array() {
                    if elements.iter().any(|e| e["id"] == element_id) {
                        client1_saw_element = true;
                        break;
                    }
                }
            } else if msg["type"] == "element_added" && msg["element"]["id"] == element_id {
                client1_saw_element = true;
                break;
            }
        } else {
            break;
        }
    }

    // Check client 2 receives the broadcast (more retries for CI reliability)
    for _ in 0..10 {
        if let Some(msg) = recv_json(&mut read2).await {
            if msg["type"] == "scene_update" {
                if let Some(elements) = msg["scene"]["elements"].as_array() {
                    if elements.iter().any(|e| e["id"] == element_id) {
                        client2_saw_element = true;
                        break;
                    }
                }
            } else if msg["type"] == "element_added" && msg["element"]["id"] == element_id {
                client2_saw_element = true;
                break;
            }
        } else {
            break;
        }
    }

    assert!(client1_saw_element, "Client 1 should see the added element");
    assert!(client2_saw_element, "Client 2 should see the added element");

    server.shutdown().await;
}

#[tokio::test]
async fn mcp_endpoint_triggers_websocket_update() {
    let server = TestServer::start().await;

    // Connect a WebSocket client
    let (ws, _) = connect_async(&server.ws_url())
        .await
        .expect("Failed to connect");

    let (mut write, mut read) = ws.split();

    // Skip all 4 initial messages: welcome, peer_assigned, initial_scene, call_state
    for _ in 0..4 {
        let _ = recv_json(&mut read).await;
    }

    // Subscribe to default session
    send_json(
        &mut write,
        &json!({
            "type": "subscribe",
            "session_id": "default"
        }),
    )
    .await
    .unwrap();

    // Make an MCP request to add an element via HTTP
    let client = reqwest::Client::new();
    let mcp_response = client
        .post(server.mcp_url())
        .json(&json!({
            "jsonrpc": "2.0",
            "method": "tools/call",
            "params": {
                "name": "canvas_render",
                "arguments": {
                    "session_id": "default",
                    "content": {
                        "type": "Text",
                        "data": {
                            "content": "MCP Added Element",
                            "font_size": 16.0
                        }
                    },
                    "position": {
                        "x": 50.0,
                        "y": 50.0
                    }
                }
            },
            "id": "mcp-test-1"
        }))
        .send()
        .await
        .expect("MCP request failed");

    let mcp_body: serde_json::Value = mcp_response
        .json()
        .await
        .expect("Failed to parse MCP response");

    // Check MCP response indicates success
    assert!(
        mcp_body.get("result").is_some(),
        "MCP request should succeed: {:?}",
        mcp_body
    );

    // WebSocket client should receive a scene_update with the new element
    let mut mcp_element_found = false;
    for _ in 0..10 {
        if let Some(msg) = recv_json(&mut read).await {
            if msg["type"] == "scene_update" {
                if let Some(elements) = msg["scene"]["elements"].as_array() {
                    if elements
                        .iter()
                        .any(|e| e["kind"]["data"]["content"] == "MCP Added Element")
                    {
                        mcp_element_found = true;
                        break;
                    }
                }
            }
        } else {
            break;
        }
    }

    assert!(
        mcp_element_found,
        "WebSocket client should receive scene update from MCP operation"
    );

    server.shutdown().await;
}

#[tokio::test]
async fn concurrent_updates_from_multiple_clients() {
    let server = TestServer::start().await;

    // Connect three clients
    let (ws1, _) = connect_async(&server.ws_url()).await.unwrap();
    let (ws2, _) = connect_async(&server.ws_url()).await.unwrap();
    let (ws3, _) = connect_async(&server.ws_url()).await.unwrap();

    let (mut write1, mut read1) = ws1.split();
    let (mut write2, mut read2) = ws2.split();
    let (mut write3, mut read3) = ws3.split();

    // Skip welcome messages
    let _ = recv_json(&mut read1).await;
    let _ = recv_json(&mut read2).await;
    let _ = recv_json(&mut read3).await;

    // All clients subscribe to the same session
    for (i, write) in [&mut write1, &mut write2, &mut write3]
        .into_iter()
        .enumerate()
    {
        send_json(
            write,
            &json!({
                "type": "subscribe",
                "session_id": "concurrent-session"
            }),
        )
        .await
        .unwrap_or_else(|_| panic!("Client {} failed to subscribe", i + 1));
    }

    // Skip initial scenes
    let _ = recv_json(&mut read1).await;
    let _ = recv_json(&mut read2).await;
    let _ = recv_json(&mut read3).await;

    // Each client adds an element concurrently
    let msg1 = json!({
        "type": "add_element",
        "element": {
            "id": "",
            "kind": {
                "type": "Text",
                "data": {
                    "content": "From Client 1",
                    "font_size": 12.0,
                    "color": "#ff0000"
                }
            }
        },
        "message_id": "c1-add"
    });
    let msg2 = json!({
        "type": "add_element",
        "element": {
            "id": "",
            "kind": {
                "type": "Text",
                "data": {
                    "content": "From Client 2",
                    "font_size": 12.0,
                    "color": "#00ff00"
                }
            }
        },
        "message_id": "c2-add"
    });
    let msg3 = json!({
        "type": "add_element",
        "element": {
            "id": "",
            "kind": {
                "type": "Text",
                "data": {
                    "content": "From Client 3",
                    "font_size": 12.0,
                    "color": "#0000ff"
                }
            }
        },
        "message_id": "c3-add"
    });

    // Send all add_element messages
    send_json(&mut write1, &msg1)
        .await
        .expect("Failed to send c1");
    send_json(&mut write2, &msg2)
        .await
        .expect("Failed to send c2");
    send_json(&mut write3, &msg3)
        .await
        .expect("Failed to send c3");

    // Give the server time to process
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Drain any intermediate messages (acks, partial updates)
    for _ in 0..15 {
        let _ = timeout(Duration::from_millis(100), recv_json(&mut read1)).await;
    }

    // Request the current scene from one client
    send_json(
        &mut write1,
        &json!({
            "type": "get_scene",
            "session_id": "concurrent-session"
        }),
    )
    .await
    .unwrap();

    // Find the scene_update response
    let scene = recv_until_type(&mut read1, "scene_update", 10)
        .await
        .expect("Should receive scene update");

    let elements = scene["scene"]["elements"].as_array().expect("No elements");

    // Verify all three elements are present
    let has_client1 = elements
        .iter()
        .any(|e| e["kind"]["data"]["content"] == "From Client 1");
    let has_client2 = elements
        .iter()
        .any(|e| e["kind"]["data"]["content"] == "From Client 2");
    let has_client3 = elements
        .iter()
        .any(|e| e["kind"]["data"]["content"] == "From Client 3");

    assert!(has_client1, "Scene should contain element from Client 1");
    assert!(has_client2, "Scene should contain element from Client 2");
    assert!(has_client3, "Scene should contain element from Client 3");

    server.shutdown().await;
}

#[tokio::test]
async fn clients_in_different_sessions_isolated() {
    let server = TestServer::start().await;

    // Connect two clients to different sessions
    let (ws1, _) = connect_async(&server.ws_url()).await.unwrap();
    let (ws2, _) = connect_async(&server.ws_url()).await.unwrap();

    let (mut write1, mut read1) = ws1.split();
    let (mut write2, mut read2) = ws2.split();

    // Skip welcome
    let _ = recv_json(&mut read1).await;
    let _ = recv_json(&mut read2).await;

    // Subscribe to different sessions
    send_json(
        &mut write1,
        &json!({
            "type": "subscribe",
            "session_id": "session-alpha"
        }),
    )
    .await
    .unwrap();
    send_json(
        &mut write2,
        &json!({
            "type": "subscribe",
            "session_id": "session-beta"
        }),
    )
    .await
    .unwrap();

    // Skip initial scenes
    let _ = recv_json(&mut read1).await;
    let _ = recv_json(&mut read2).await;

    // Client 1 adds an element to session-alpha
    send_json(
        &mut write1,
        &json!({
            "type": "add_element",
            "element": {
                "id": "",
                "kind": {
                    "type": "Text",
                    "data": {
                        "content": "Alpha Only",
                        "font_size": 14.0,
                        "color": "#ff0000"
                    }
                }
            },
            "message_id": "alpha-add"
        }),
    )
    .await
    .unwrap();

    // Client 1 should receive ack
    let ack = recv_until_type(&mut read1, "ack", 5)
        .await
        .expect("Client 1 should receive ack");
    assert_eq!(ack["message_id"], "alpha-add");

    // Client 2 (session-beta) should NOT receive any element broadcasts
    // Try to receive for a short time
    let received = timeout(Duration::from_millis(200), recv_json(&mut read2)).await;

    // Either timeout (Ok(None)) or no scene_update with the element
    match received {
        Ok(Some(msg)) => {
            // If we got a message, it should not be about the alpha element
            if msg["type"] == "scene_update" {
                if let Some(elements) = msg["scene"]["elements"].as_array() {
                    let has_alpha = elements
                        .iter()
                        .any(|e| e["kind"]["data"]["content"] == "Alpha Only");
                    assert!(
                        !has_alpha,
                        "Session beta should not receive session alpha elements"
                    );
                }
            } else if msg["type"] == "element_added" {
                assert_ne!(
                    msg["element"]["kind"]["data"]["content"], "Alpha Only",
                    "Session beta should not receive session alpha broadcasts"
                );
            }
        }
        _ => {
            // Timeout or no message is expected - sessions are isolated
        }
    }

    server.shutdown().await;
}
