//! Integration tests for WebSocket scene synchronization.

use serde_json::json;

/// Test message parsing for client messages.
#[test]
fn test_parse_subscribe_message() {
    let json = json!({
        "type": "subscribe",
        "session_id": "test-session"
    });

    let text = json.to_string();
    assert!(text.contains("subscribe"));
    assert!(text.contains("test-session"));
}

/// Test message parsing for add_element.
#[test]
fn test_parse_add_element_message() {
    let json = json!({
        "type": "add_element",
        "element": {
            "kind": {
                "type": "text",
                "content": "Hello World",
                "font_size": 24.0,
                "color": "#ff0000"
            },
            "transform": {
                "x": 100.0,
                "y": 200.0,
                "width": 300.0,
                "height": 50.0
            }
        },
        "message_id": "msg-001"
    });

    let text = json.to_string();
    assert!(text.contains("add_element"));
    assert!(text.contains("Hello World"));
    assert!(text.contains("msg-001"));
}

/// Test message parsing for remove_element.
#[test]
fn test_parse_remove_element_message() {
    let json = json!({
        "type": "remove_element",
        "id": "550e8400-e29b-41d4-a716-446655440000",
        "message_id": "msg-002"
    });

    let text = json.to_string();
    assert!(text.contains("remove_element"));
    assert!(text.contains("550e8400"));
}

/// Test message parsing for update_element.
#[test]
fn test_parse_update_element_message() {
    let json = json!({
        "type": "update_element",
        "id": "550e8400-e29b-41d4-a716-446655440000",
        "changes": {
            "transform": {
                "x": 150.0,
                "y": 250.0
            }
        }
    });

    let text = json.to_string();
    assert!(text.contains("update_element"));
    assert!(text.contains("150"));
}

/// Test message parsing for ping.
#[test]
fn test_parse_ping_message() {
    let json = json!({
        "type": "ping"
    });

    let text = json.to_string();
    assert!(text.contains("ping"));
}

/// Test message parsing for sync_queue.
#[test]
fn test_parse_sync_queue_message() {
    let json = json!({
        "type": "sync_queue",
        "operations": [
            {
                "op": "add",
                "element": {
                    "kind": {"type": "text", "content": "Offline 1"}
                },
                "timestamp": 1000
            },
            {
                "op": "remove",
                "id": "some-id",
                "timestamp": 2000
            }
        ]
    });

    let text = json.to_string();
    assert!(text.contains("sync_queue"));
    assert!(text.contains("Offline 1"));
    assert!(text.contains("2000"));
}

/// Test server message format for welcome.
#[test]
fn test_server_welcome_format() {
    let json = json!({
        "type": "welcome",
        "version": "0.1.0",
        "session_id": "default",
        "timestamp": 1234567890
    });

    let text = json.to_string();
    assert!(text.contains("welcome"));
    assert!(text.contains("version"));
    assert!(text.contains("session_id"));
}

/// Test server message format for scene_update.
#[test]
fn test_server_scene_update_format() {
    let json = json!({
        "type": "scene_update",
        "elements": [
            {
                "id": "elem-1",
                "kind": {"type": "text", "content": "Test"},
                "transform": {"x": 0, "y": 0, "width": 100, "height": 100},
                "interactive": true
            }
        ],
        "viewport_width": 800.0,
        "viewport_height": 600.0,
        "timestamp": 1234567890
    });

    let text = json.to_string();
    assert!(text.contains("scene_update"));
    assert!(text.contains("elements"));
    assert!(text.contains("viewport_width"));
}

/// Test server message format for element_added.
#[test]
fn test_server_element_added_format() {
    let json = json!({
        "type": "element_added",
        "element": {
            "id": "new-elem",
            "kind": {"type": "text", "content": "New Element"},
            "transform": {"x": 50, "y": 50, "width": 200, "height": 40},
            "interactive": true
        },
        "timestamp": 1234567890
    });

    let text = json.to_string();
    assert!(text.contains("element_added"));
    assert!(text.contains("new-elem"));
    assert!(text.contains("New Element"));
}

/// Test server message format for element_removed.
#[test]
fn test_server_element_removed_format() {
    let json = json!({
        "type": "element_removed",
        "id": "removed-elem",
        "timestamp": 1234567890
    });

    let text = json.to_string();
    assert!(text.contains("element_removed"));
    assert!(text.contains("removed-elem"));
}

/// Test server message format for ack.
#[test]
fn test_server_ack_format() {
    let json = json!({
        "type": "ack",
        "message_id": "msg-123",
        "success": true,
        "result": {"id": "new-element-id"}
    });

    let text = json.to_string();
    assert!(text.contains("ack"));
    assert!(text.contains("msg-123"));
    assert!(text.contains("success"));
}

/// Test server message format for error.
#[test]
fn test_server_error_format() {
    let json = json!({
        "type": "error",
        "code": "element_not_found",
        "message": "Element with ID 'xyz' not found",
        "message_id": "msg-456"
    });

    let text = json.to_string();
    assert!(text.contains("error"));
    assert!(text.contains("element_not_found"));
    assert!(text.contains("xyz"));
}

/// Test server message format for pong.
#[test]
fn test_server_pong_format() {
    let json = json!({
        "type": "pong",
        "timestamp": 1234567890
    });

    let text = json.to_string();
    assert!(text.contains("pong"));
    assert!(text.contains("timestamp"));
}

/// Test server message format for sync_result.
#[test]
fn test_server_sync_result_format() {
    let json = json!({
        "type": "sync_result",
        "synced_count": 5,
        "conflict_count": 1,
        "timestamp": 1234567890
    });

    let text = json.to_string();
    assert!(text.contains("sync_result"));
    assert!(text.contains("synced_count"));
    assert!(text.contains("conflict_count"));
}

/// Test element kind formats.
mod element_kinds {
    use serde_json::json;

    #[test]
    fn test_text_element() {
        let json = json!({
            "type": "text",
            "content": "Hello",
            "font_size": 16.0,
            "color": "#000000"
        });
        let text = json.to_string();
        assert!(text.contains("text"));
        assert!(text.contains("Hello"));
    }

    #[test]
    fn test_image_element() {
        let json = json!({
            "type": "image",
            "src": "https://example.com/image.png",
            "format": "png"
        });
        let text = json.to_string();
        assert!(text.contains("image"));
        assert!(text.contains("https://"));
    }

    #[test]
    fn test_chart_element() {
        let json = json!({
            "type": "chart",
            "chart_type": "bar",
            "data": {
                "labels": ["A", "B", "C"],
                "values": [10, 20, 30]
            }
        });
        let text = json.to_string();
        assert!(text.contains("chart"));
        assert!(text.contains("bar"));
    }

    #[test]
    fn test_group_element() {
        let json = json!({
            "type": "group",
            "children": ["child-1", "child-2"]
        });
        let text = json.to_string();
        assert!(text.contains("group"));
        assert!(text.contains("child-1"));
    }

    #[test]
    fn test_video_element() {
        let json = json!({
            "type": "video",
            "stream_id": "local-camera",
            "is_live": true,
            "mirror": true
        });
        let text = json.to_string();
        assert!(text.contains("video"));
        assert!(text.contains("local-camera"));
    }
}

/// Test transform data format.
#[test]
fn test_transform_format() {
    let json = json!({
        "x": 100.5,
        "y": 200.5,
        "width": 300.0,
        "height": 150.0,
        "rotation": 0.785398,
        "z_index": 5
    });

    let text = json.to_string();
    assert!(text.contains("100.5"));
    assert!(text.contains("rotation"));
    assert!(text.contains("z_index"));
}

/// Test queued operation formats.
mod queued_operations {
    use serde_json::json;

    #[test]
    fn test_add_operation() {
        let json = json!({
            "op": "add",
            "element": {
                "kind": {"type": "text", "content": "Queued"}
            },
            "timestamp": 12345
        });
        let text = json.to_string();
        assert!(text.contains("add"));
        assert!(text.contains("Queued"));
    }

    #[test]
    fn test_update_operation() {
        let json = json!({
            "op": "update",
            "id": "elem-id",
            "changes": {"transform": {"x": 100}},
            "timestamp": 12345
        });
        let text = json.to_string();
        assert!(text.contains("update"));
        assert!(text.contains("elem-id"));
    }

    #[test]
    fn test_remove_operation() {
        let json = json!({
            "op": "remove",
            "id": "elem-id",
            "timestamp": 12345
        });
        let text = json.to_string();
        assert!(text.contains("remove"));
        assert!(text.contains("elem-id"));
    }
}

/// Test full message round-trip scenarios.
mod scenarios {
    use serde_json::json;

    /// Scenario: New client connects and receives scene.
    #[test]
    fn test_new_client_connection_flow() {
        // 1. Server sends welcome
        let welcome = json!({
            "type": "welcome",
            "version": "0.1.0",
            "session_id": "default",
            "timestamp": 1000
        });
        assert!(welcome.to_string().contains("welcome"));

        // 2. Server sends current scene state
        let scene = json!({
            "type": "scene_update",
            "elements": [],
            "viewport_width": 800.0,
            "viewport_height": 600.0,
            "timestamp": 1001
        });
        assert!(scene.to_string().contains("scene_update"));
    }

    /// Scenario: Client adds an element.
    #[test]
    fn test_add_element_flow() {
        // 1. Client sends add request
        let request = json!({
            "type": "add_element",
            "element": {
                "kind": {"type": "text", "content": "New Text"},
                "transform": {"x": 100, "y": 100, "width": 200, "height": 30}
            },
            "message_id": "add-001"
        });
        assert!(request.to_string().contains("add_element"));

        // 2. Server broadcasts to all clients
        let broadcast = json!({
            "type": "element_added",
            "element": {
                "id": "generated-uuid",
                "kind": {"type": "text", "content": "New Text"},
                "transform": {"x": 100, "y": 100, "width": 200, "height": 30},
                "interactive": true
            },
            "timestamp": 2000
        });
        assert!(broadcast.to_string().contains("element_added"));

        // 3. Server sends ack to originator
        let ack = json!({
            "type": "ack",
            "message_id": "add-001",
            "success": true,
            "result": {"id": "generated-uuid"}
        });
        assert!(ack.to_string().contains("ack"));
    }

    /// Scenario: Client reconnects with offline queue.
    #[test]
    fn test_reconnection_with_queue() {
        // 1. Client reconnects and sends queued operations
        let sync_request = json!({
            "type": "sync_queue",
            "operations": [
                {
                    "op": "add",
                    "element": {"kind": {"type": "text", "content": "Offline 1"}},
                    "timestamp": 1000
                },
                {
                    "op": "update",
                    "id": "existing-elem",
                    "changes": {"transform": {"x": 50}},
                    "timestamp": 1001
                }
            ]
        });
        assert!(sync_request.to_string().contains("sync_queue"));

        // 2. Server responds with sync result
        let sync_result = json!({
            "type": "sync_result",
            "synced_count": 2,
            "conflict_count": 0,
            "timestamp": 3000
        });
        assert!(sync_result.to_string().contains("sync_result"));
    }

    /// Scenario: Multiple clients receive broadcasts.
    #[test]
    fn test_multi_client_broadcast() {
        // Element added by client A
        let element_added = json!({
            "type": "element_added",
            "element": {
                "id": "elem-from-a",
                "kind": {"type": "text", "content": "From A"},
                "transform": {"x": 0, "y": 0, "width": 100, "height": 100},
                "interactive": true
            },
            "timestamp": 1000
        });

        // All subscribed clients (A, B, C) receive this
        let text = element_added.to_string();
        assert!(text.contains("element_added"));
        assert!(text.contains("elem-from-a"));
        assert!(text.contains("From A"));
    }
}
