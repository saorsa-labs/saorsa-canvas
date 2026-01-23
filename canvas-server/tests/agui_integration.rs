//! # AG-UI Integration Tests
//!
//! End-to-end tests verifying the A2UI → MCP → Scene → SSE pipeline.

use std::time::Duration;

use canvas_core::A2UITree;
use canvas_server::agui::{AgUiEvent, AgUiState, InteractionEvent, RenderA2UIRequest};
use canvas_server::sync::SyncState;
use tokio::time::timeout;

/// Test that A2UI trees render to scene elements correctly.
#[tokio::test]
async fn test_a2ui_render_creates_scene_elements() {
    let state = AgUiState::new(SyncState::new());

    let tree = A2UITree::from_json(
        r#"{
        "root": {
            "component": "container",
            "layout": "vertical",
            "children": [
                { "component": "text", "content": "Header" },
                { "component": "text", "content": "Body" }
            ]
        }
    }"#,
    )
    .expect("should parse");

    let response = state.render_a2ui(&RenderA2UIRequest {
        tree,
        session_id: "test-session".to_string(),
        clear: true,
    });

    assert!(response.success);
    assert_eq!(response.element_count, 2);

    // Verify scene was updated
    let scene = state
        .sync
        .get_scene("test-session")
        .expect("scene should exist");
    assert_eq!(scene.element_count(), 2);
}

/// Test that rendering with clear=true replaces existing elements.
#[tokio::test]
async fn test_a2ui_render_with_clear_replaces_elements() {
    let state = AgUiState::new(SyncState::new());

    // First render
    let tree1 =
        A2UITree::from_json(r#"{"root": { "component": "text", "content": "First" }}"#).unwrap();
    state.render_a2ui(&RenderA2UIRequest {
        tree: tree1,
        session_id: "test".to_string(),
        clear: true,
    });

    // Second render with clear
    let tree2 =
        A2UITree::from_json(r#"{"root": { "component": "text", "content": "Second" }}"#).unwrap();
    let response = state.render_a2ui(&RenderA2UIRequest {
        tree: tree2,
        session_id: "test".to_string(),
        clear: true,
    });

    assert!(response.success);
    assert_eq!(response.element_count, 1);

    // Scene should only have the second element
    let scene = state.sync.get_scene("test").unwrap();
    assert_eq!(scene.element_count(), 1);
}

/// Test that AG-UI broadcast mechanism works correctly.
#[tokio::test]
async fn test_agui_broadcast_mechanism() {
    let state = AgUiState::new(SyncState::new());

    // Subscribe before broadcasting
    let mut rx = state.event_tx.subscribe();

    // Broadcast a scene update event
    state.broadcast(AgUiEvent::SceneUpdate {
        session_id: "test".to_string(),
        element_count: 5,
        timestamp: 12345,
    });

    // Broadcast a heartbeat
    state.broadcast(AgUiEvent::Heartbeat { timestamp: 12346 });

    // Should receive both events (order guaranteed for same sender)
    let mut received_scene_update = false;
    let mut received_heartbeat = false;

    for _ in 0..10 {
        match timeout(Duration::from_millis(100), rx.recv()).await {
            Ok(Ok(AgUiEvent::SceneUpdate { element_count, .. })) => {
                assert_eq!(element_count, 5);
                received_scene_update = true;
            }
            Ok(Ok(AgUiEvent::Heartbeat { timestamp })) => {
                assert_eq!(timestamp, 12346);
                received_heartbeat = true;
            }
            Ok(Ok(AgUiEvent::Interaction { .. })) => {
                // Skip interaction events from relay task
                continue;
            }
            _ => break,
        }

        if received_scene_update && received_heartbeat {
            break;
        }
    }

    assert!(received_scene_update, "Should have received SceneUpdate");
    assert!(received_heartbeat, "Should have received Heartbeat");
}

/// Test that interaction events are relayed from WebSocket to AG-UI SSE.
#[tokio::test]
async fn test_interaction_events_relayed_to_agui() {
    let sync_state = SyncState::new();
    let agui_state = AgUiState::new(sync_state.clone());

    // Subscribe to AG-UI events
    let mut rx = agui_state.event_tx.subscribe();

    // Broadcast an interaction through sync state
    sync_state.broadcast_interaction(
        "test-session",
        InteractionEvent::Touch {
            element_id: Some("btn-1".to_string()),
            phase: "start".to_string(),
            x: 100.0,
            y: 200.0,
            pointer_id: 0,
        },
    );

    // Give the relay task a moment to process
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Should receive the interaction event via AG-UI
    let received = timeout(Duration::from_millis(100), rx.recv())
        .await
        .expect("should not timeout")
        .expect("should receive");

    match received {
        AgUiEvent::Interaction {
            session_id,
            interaction,
            timestamp,
        } => {
            assert_eq!(session_id, "test-session");
            assert!(timestamp > 0);
            match interaction {
                InteractionEvent::Touch {
                    element_id, phase, ..
                } => {
                    assert_eq!(element_id, Some("btn-1".to_string()));
                    assert_eq!(phase, "start");
                }
                _ => panic!("Expected Touch interaction"),
            }
        }
        _ => panic!("Expected Interaction event"),
    }
}

/// Test button click interaction relay.
#[tokio::test]
async fn test_button_click_interaction_relay() {
    let sync_state = SyncState::new();
    let agui_state = AgUiState::new(sync_state.clone());

    let mut rx = agui_state.event_tx.subscribe();

    sync_state.broadcast_interaction(
        "session-1",
        InteractionEvent::ButtonClick {
            element_id: "submit-btn".to_string(),
            action: "form_submit".to_string(),
        },
    );

    tokio::time::sleep(Duration::from_millis(50)).await;

    let received = timeout(Duration::from_millis(100), rx.recv())
        .await
        .expect("should not timeout")
        .expect("should receive");

    match received {
        AgUiEvent::Interaction { interaction, .. } => match interaction {
            InteractionEvent::ButtonClick { element_id, action } => {
                assert_eq!(element_id, "submit-btn");
                assert_eq!(action, "form_submit");
            }
            _ => panic!("Expected ButtonClick interaction"),
        },
        _ => panic!("Expected Interaction event"),
    }
}

/// Test form input interaction relay.
#[tokio::test]
async fn test_form_input_interaction_relay() {
    let sync_state = SyncState::new();
    let agui_state = AgUiState::new(sync_state.clone());

    let mut rx = agui_state.event_tx.subscribe();

    sync_state.broadcast_interaction(
        "session-1",
        InteractionEvent::FormInput {
            element_id: "name-input".to_string(),
            field: "username".to_string(),
            value: "alice".to_string(),
        },
    );

    tokio::time::sleep(Duration::from_millis(50)).await;

    let received = timeout(Duration::from_millis(100), rx.recv())
        .await
        .expect("should not timeout")
        .expect("should receive");

    match received {
        AgUiEvent::Interaction { interaction, .. } => match interaction {
            InteractionEvent::FormInput {
                element_id,
                field,
                value,
            } => {
                assert_eq!(element_id, "name-input");
                assert_eq!(field, "username");
                assert_eq!(value, "alice");
            }
            _ => panic!("Expected FormInput interaction"),
        },
        _ => panic!("Expected Interaction event"),
    }
}

/// Test multiple concurrent interactions.
#[tokio::test]
async fn test_multiple_concurrent_interactions() {
    let sync_state = SyncState::new();
    let agui_state = AgUiState::new(sync_state.clone());

    let mut rx = agui_state.event_tx.subscribe();

    // Send multiple interactions quickly
    for i in 0..5 {
        sync_state.broadcast_interaction(
            "session-1",
            InteractionEvent::Touch {
                element_id: Some(format!("el-{i}")),
                phase: "move".to_string(),
                x: i as f32 * 10.0,
                y: i as f32 * 10.0,
                pointer_id: 0,
            },
        );
    }

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Should receive all 5 interactions
    let mut received_count = 0;
    while let Ok(Ok(_)) = timeout(Duration::from_millis(50), rx.recv()).await {
        received_count += 1;
    }

    assert_eq!(received_count, 5, "Should receive all 5 interactions");
}

/// Test gesture interaction relay.
#[tokio::test]
async fn test_gesture_interaction_relay() {
    let sync_state = SyncState::new();
    let agui_state = AgUiState::new(sync_state.clone());

    let mut rx = agui_state.event_tx.subscribe();

    sync_state.broadcast_interaction(
        "session-1",
        InteractionEvent::Gesture {
            gesture_type: "pinch".to_string(),
            scale: Some(1.5),
            rotation: Some(45.0),
            center_x: 200.0,
            center_y: 300.0,
        },
    );

    tokio::time::sleep(Duration::from_millis(50)).await;

    let received = timeout(Duration::from_millis(100), rx.recv())
        .await
        .expect("should not timeout")
        .expect("should receive");

    match received {
        AgUiEvent::Interaction { interaction, .. } => match interaction {
            InteractionEvent::Gesture {
                gesture_type,
                scale,
                rotation,
                center_x,
                center_y,
            } => {
                assert_eq!(gesture_type, "pinch");
                assert!((scale.unwrap() - 1.5).abs() < 0.001);
                assert!((rotation.unwrap() - 45.0).abs() < 0.001);
                assert!((center_x - 200.0).abs() < 0.001);
                assert!((center_y - 300.0).abs() < 0.001);
            }
            _ => panic!("Expected Gesture interaction"),
        },
        _ => panic!("Expected Interaction event"),
    }
}

/// Test selection interaction relay.
#[tokio::test]
async fn test_selection_interaction_relay() {
    let sync_state = SyncState::new();
    let agui_state = AgUiState::new(sync_state.clone());

    let mut rx = agui_state.event_tx.subscribe();

    sync_state.broadcast_interaction(
        "session-1",
        InteractionEvent::Selection {
            element_id: "checkbox-1".to_string(),
            selected: true,
        },
    );

    tokio::time::sleep(Duration::from_millis(50)).await;

    let received = timeout(Duration::from_millis(100), rx.recv())
        .await
        .expect("should not timeout")
        .expect("should receive");

    match received {
        AgUiEvent::Interaction { interaction, .. } => match interaction {
            InteractionEvent::Selection {
                element_id,
                selected,
            } => {
                assert_eq!(element_id, "checkbox-1");
                assert!(selected);
            }
            _ => panic!("Expected Selection interaction"),
        },
        _ => panic!("Expected Interaction event"),
    }
}

/// Test empty A2UI tree.
#[tokio::test]
async fn test_empty_a2ui_container() {
    let state = AgUiState::new(SyncState::new());

    let tree = A2UITree::from_json(
        r#"{
        "root": {
            "component": "container",
            "layout": "vertical",
            "children": []
        }
    }"#,
    )
    .expect("should parse");

    let response = state.render_a2ui(&RenderA2UIRequest {
        tree,
        session_id: "test".to_string(),
        clear: true,
    });

    assert!(response.success);
    assert_eq!(response.element_count, 0);
}

/// Test deeply nested A2UI structure.
#[tokio::test]
async fn test_deeply_nested_a2ui() {
    let state = AgUiState::new(SyncState::new());

    let tree = A2UITree::from_json(
        r#"{
        "root": {
            "component": "container",
            "layout": "vertical",
            "children": [{
                "component": "container",
                "layout": "horizontal",
                "children": [{
                    "component": "container",
                    "layout": "vertical",
                    "children": [{
                        "component": "text",
                        "content": "Deep"
                    }]
                }]
            }]
        }
    }"#,
    )
    .expect("should parse");

    let response = state.render_a2ui(&RenderA2UIRequest {
        tree,
        session_id: "test".to_string(),
        clear: true,
    });

    assert!(response.success);
    assert_eq!(response.element_count, 1);
}
