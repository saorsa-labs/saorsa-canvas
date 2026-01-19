//! AG-UI (Agent-Generated UI) event streaming via Server-Sent Events.
//!
//! This module implements the AG-UI protocol for real-time streaming of
//! UI updates from AI agents to the canvas client. It uses Server-Sent Events
//! (SSE) for efficient one-way streaming.
//!
//! ## Protocol
//!
//! The AG-UI SSE endpoint streams events in JSON format:
//!
//! ```text
//! event: scene_update
//! data: {"session_id": "default", "elements": [...], "timestamp": 1234567890}
//!
//! event: a2ui_render
//! data: {"tree": {...}, "timestamp": 1234567890}
//!
//! event: heartbeat
//! data: {"timestamp": 1234567890}
//! ```
//!
//! ## Endpoints
//!
//! - `GET /ag-ui/stream` - SSE stream for scene updates
//! - `POST /ag-ui/render` - Submit A2UI tree for rendering

use axum::{
    extract::State,
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse,
    },
    Json,
};
use canvas_core::A2UITree;
use futures::stream::Stream;
use serde::{Deserialize, Serialize};
use std::{convert::Infallible, time::Duration};
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

use crate::sync::SyncState;

/// AG-UI event types sent via SSE.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", content = "data")]
pub enum AgUiEvent {
    /// A scene update with new elements.
    #[serde(rename = "scene_update")]
    SceneUpdate {
        /// Session identifier.
        session_id: String,
        /// Number of elements in the scene.
        element_count: usize,
        /// Unix timestamp in seconds.
        timestamp: u64,
    },

    /// An A2UI tree was rendered.
    #[serde(rename = "a2ui_render")]
    A2UIRender {
        /// Number of elements created from the A2UI tree.
        element_count: usize,
        /// Any conversion warnings.
        warnings: Vec<String>,
        /// Unix timestamp in seconds.
        timestamp: u64,
    },

    /// Heartbeat to keep the connection alive.
    #[serde(rename = "heartbeat")]
    Heartbeat {
        /// Unix timestamp in seconds.
        timestamp: u64,
    },
}

/// Request to render an A2UI tree.
#[derive(Debug, Clone, Deserialize)]
pub struct RenderA2UIRequest {
    /// The A2UI tree to render.
    pub tree: A2UITree,
    /// Session ID to render to (defaults to "default").
    #[serde(default = "default_session")]
    pub session_id: String,
    /// Whether to clear existing elements first.
    #[serde(default)]
    pub clear: bool,
}

fn default_session() -> String {
    "default".to_string()
}

/// Response from rendering an A2UI tree.
#[derive(Debug, Clone, Serialize)]
pub struct RenderA2UIResponse {
    /// Whether the render was successful.
    pub success: bool,
    /// Number of elements created.
    pub element_count: usize,
    /// Any conversion warnings.
    pub warnings: Vec<String>,
    /// Error message if failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// State needed for AG-UI endpoints.
#[derive(Clone)]
pub struct AgUiState {
    /// Broadcast sender for AG-UI events.
    pub event_tx: broadcast::Sender<AgUiEvent>,
    /// Reference to sync state for scene mutations.
    pub sync: SyncState,
}

impl AgUiState {
    /// Create a new AG-UI state with the given scene.
    pub fn new(sync: SyncState) -> Self {
        let (event_tx, _) = broadcast::channel(100);
        Self { event_tx, sync }
    }

    /// Broadcast an AG-UI event to all connected clients.
    pub fn broadcast(&self, event: AgUiEvent) {
        // Ignore send errors (no receivers is okay)
        let _ = self.event_tx.send(event);
    }

    /// Get the current timestamp in seconds.
    fn timestamp() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }

    /// Render an A2UI tree to the scene.
    pub fn render_a2ui(&self, request: &RenderA2UIRequest) -> RenderA2UIResponse {
        tracing::info!(
            "Rendering A2UI tree to session '{}', clear={}",
            request.session_id,
            request.clear
        );
        let result = request.tree.to_elements();

        if let Err(e) = self.sync.update_scene(&request.session_id, |scene| {
            if request.clear {
                scene.clear();
            }
            for element in &result.elements {
                scene.add_element(element.clone());
            }
        }) {
            return RenderA2UIResponse {
                success: false,
                element_count: 0,
                warnings: vec![],
                error: Some(e.to_string()),
            };
        }

        // Broadcast the render event
        self.broadcast(AgUiEvent::A2UIRender {
            element_count: result.elements.len(),
            warnings: result.warnings.clone(),
            timestamp: Self::timestamp(),
        });

        RenderA2UIResponse {
            success: true,
            element_count: result.elements.len(),
            warnings: result.warnings,
            error: None,
        }
    }
}

/// SSE stream handler for AG-UI events.
///
/// # Example
///
/// ```text
/// curl -N http://localhost:9473/ag-ui/stream
/// ```
pub async fn stream_handler(
    State(state): State<AgUiState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.event_tx.subscribe();
    let stream = BroadcastStream::new(rx);

    let event_stream = stream.map(|result| {
        let event = match result {
            Ok(event) => event,
            Err(_) => AgUiEvent::Heartbeat {
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0),
            },
        };

        let event_type = match &event {
            AgUiEvent::SceneUpdate { .. } => "scene_update",
            AgUiEvent::A2UIRender { .. } => "a2ui_render",
            AgUiEvent::Heartbeat { .. } => "heartbeat",
        };

        let data = serde_json::to_string(&event).unwrap_or_default();

        Ok(Event::default().event(event_type).data(data))
    });

    Sse::new(event_stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(30))
            .text("heartbeat"),
    )
}

/// POST handler to render an A2UI tree.
///
/// # Example
///
/// ```bash
/// curl -X POST http://localhost:9473/ag-ui/render \
///   -H "Content-Type: application/json" \
///   -d '{"tree": {"root": {"component": "text", "content": "Hello!"}}}'
/// ```
pub async fn render_handler(
    State(state): State<AgUiState>,
    Json(request): Json<RenderA2UIRequest>,
) -> impl IntoResponse {
    let response = state.render_a2ui(&request);
    Json(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agui_event_serialization() {
        let event = AgUiEvent::SceneUpdate {
            session_id: "test".to_string(),
            element_count: 5,
            timestamp: 1234567890,
        };

        let json = serde_json::to_string(&event).expect("should serialize");
        assert!(json.contains("scene_update"));
        assert!(json.contains("test"));
        assert!(json.contains("1234567890"));
    }

    #[test]
    fn test_agui_heartbeat_serialization() {
        let event = AgUiEvent::Heartbeat {
            timestamp: 9876543210,
        };

        let json = serde_json::to_string(&event).expect("should serialize");
        assert!(json.contains("heartbeat"));
        assert!(json.contains("9876543210"));
    }

    #[test]
    fn test_render_request_default_session() {
        let json = r#"{
            "tree": {
                "root": { "component": "text", "content": "Hello" }
            }
        }"#;

        let request: RenderA2UIRequest = serde_json::from_str(json).expect("should deserialize");
        assert_eq!(request.session_id, "default");
        assert!(!request.clear);
    }

    #[test]
    fn test_render_request_with_options() {
        let json = r#"{
            "tree": {
                "root": { "component": "text", "content": "Hello" }
            },
            "session_id": "custom",
            "clear": true
        }"#;

        let request: RenderA2UIRequest = serde_json::from_str(json).expect("should deserialize");
        assert_eq!(request.session_id, "custom");
        assert!(request.clear);
    }

    #[test]
    fn test_agui_state_render_a2ui() {
        let state = AgUiState::new(SyncState::new());

        let tree = A2UITree::from_json(
            r#"{
            "root": {
                "component": "container",
                "layout": "vertical",
                "children": [
                    { "component": "text", "content": "Line 1" },
                    { "component": "text", "content": "Line 2" }
                ]
            }
        }"#,
        )
        .expect("should parse");

        let request = RenderA2UIRequest {
            tree,
            session_id: "test".to_string(),
            clear: true,
        };

        let response = state.render_a2ui(&request);

        assert!(response.success);
        assert_eq!(response.element_count, 2);
        assert!(response.warnings.is_empty());
        assert!(response.error.is_none());

        // Verify scene was updated
        let scene = state
            .sync
            .get_scene("test")
            .expect("scene should exist after render");
        assert_eq!(scene.element_count(), 2);
    }

    #[test]
    fn test_agui_state_render_without_clear() {
        let state = AgUiState::new(SyncState::new());

        // First render
        let tree1 = A2UITree::from_json(r#"{"root": { "component": "text", "content": "First" }}"#)
            .expect("should parse");

        state.render_a2ui(&RenderA2UIRequest {
            tree: tree1,
            session_id: "test".to_string(),
            clear: false,
        });

        // Second render without clear
        let tree2 =
            A2UITree::from_json(r#"{"root": { "component": "text", "content": "Second" }}"#)
                .expect("should parse");

        let response = state.render_a2ui(&RenderA2UIRequest {
            tree: tree2,
            session_id: "test".to_string(),
            clear: false,
        });

        assert!(response.success);
        assert_eq!(response.element_count, 1);

        // Scene should have both elements
        let scene = state.sync.get_scene("test").expect("scene should exist");
        assert_eq!(scene.element_count(), 2);
    }

    #[test]
    fn test_agui_state_render_with_clear() {
        let state = AgUiState::new(SyncState::new());

        // First render
        let tree1 = A2UITree::from_json(r#"{"root": { "component": "text", "content": "First" }}"#)
            .expect("should parse");

        state.render_a2ui(&RenderA2UIRequest {
            tree: tree1,
            session_id: "test".to_string(),
            clear: false,
        });

        // Second render WITH clear
        let tree2 =
            A2UITree::from_json(r#"{"root": { "component": "text", "content": "Second" }}"#)
                .expect("should parse");

        let response = state.render_a2ui(&RenderA2UIRequest {
            tree: tree2,
            session_id: "test".to_string(),
            clear: true,
        });

        assert!(response.success);
        assert_eq!(response.element_count, 1);

        // Scene should only have the second element
        let scene = state.sync.get_scene("test").expect("scene should exist");
        assert_eq!(scene.element_count(), 1);
    }

    #[test]
    fn test_broadcast_event() {
        let state = AgUiState::new(SyncState::new());

        // Subscribe before broadcasting
        let mut rx = state.event_tx.subscribe();

        // Broadcast an event
        state.broadcast(AgUiEvent::Heartbeat { timestamp: 12345 });

        // Should receive the event
        let received = rx.try_recv().expect("should receive");
        match received {
            AgUiEvent::Heartbeat { timestamp } => assert_eq!(timestamp, 12345),
            _ => panic!("Expected Heartbeat event"),
        }
    }
}
