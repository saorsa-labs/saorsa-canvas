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

    /// User interaction event (touch, button click, form input).
    #[serde(rename = "interaction")]
    Interaction {
        /// Session identifier.
        session_id: String,
        /// The interaction details.
        interaction: InteractionEvent,
        /// Unix timestamp in milliseconds (for precise timing).
        timestamp: u64,
    },

    /// Heartbeat to keep the connection alive.
    #[serde(rename = "heartbeat")]
    Heartbeat {
        /// Unix timestamp in seconds.
        timestamp: u64,
    },
}

/// User interaction event types for AG-UI protocol.
///
/// These events are sent from the canvas client when users interact
/// with rendered elements, allowing AI agents to respond to user input.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InteractionEvent {
    /// Touch/pointer event (tap, drag, pinch).
    Touch {
        /// Element ID that was touched (if any).
        #[serde(skip_serializing_if = "Option::is_none")]
        element_id: Option<String>,
        /// Touch phase: "start", "move", "end", "cancel".
        phase: String,
        /// X coordinate in canvas space.
        x: f32,
        /// Y coordinate in canvas space.
        y: f32,
        /// Pointer ID for multi-touch tracking.
        #[serde(default)]
        pointer_id: u32,
    },

    /// Button click event.
    ButtonClick {
        /// ID of the clicked button element.
        element_id: String,
        /// Action identifier from the A2UI Button component.
        action: String,
    },

    /// Form input event.
    FormInput {
        /// ID of the input element.
        element_id: String,
        /// Input field name.
        field: String,
        /// Current input value.
        value: String,
    },

    /// Element selection event.
    Selection {
        /// ID of the selected element.
        element_id: String,
        /// Whether the element is now selected.
        selected: bool,
    },

    /// Gesture event (pinch, rotate, etc.).
    Gesture {
        /// Gesture type: "pinch", "rotate", "pan".
        gesture_type: String,
        /// Gesture scale factor (for pinch).
        #[serde(skip_serializing_if = "Option::is_none")]
        scale: Option<f32>,
        /// Gesture rotation in degrees (for rotate).
        #[serde(skip_serializing_if = "Option::is_none")]
        rotation: Option<f32>,
        /// Center X coordinate.
        center_x: f32,
        /// Center Y coordinate.
        center_y: f32,
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
    ///
    /// This also starts a background task that forwards interaction events
    /// from WebSocket clients (via `SyncState`) to AG-UI SSE clients.
    pub fn new(sync: SyncState) -> Self {
        let (event_tx, _) = broadcast::channel(100);
        let state = Self { event_tx, sync };

        // Spawn background task to relay interactions from sync to AG-UI
        state.start_interaction_relay();

        state
    }

    /// Start a background task that relays interaction events from SyncState to AG-UI clients.
    fn start_interaction_relay(&self) {
        let mut interaction_rx = self.sync.subscribe_interactions();
        let event_tx = self.event_tx.clone();

        tokio::spawn(async move {
            loop {
                match interaction_rx.recv().await {
                    Ok((session_id, interaction)) => {
                        let timestamp = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_millis() as u64)
                            .unwrap_or(0);

                        let event = AgUiEvent::Interaction {
                            session_id,
                            interaction,
                            timestamp,
                        };

                        // Forward to AG-UI SSE clients
                        let _ = event_tx.send(event);
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        tracing::debug!("Interaction relay channel closed, stopping");
                        break;
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("Interaction relay lagged by {n} messages");
                    }
                }
            }
        });
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

    /// Broadcast an interaction event to all connected AG-UI clients.
    ///
    /// This should be called when a user interacts with the canvas
    /// (touch, button click, form input, etc.).
    pub fn broadcast_interaction(&self, session_id: &str, interaction: InteractionEvent) {
        self.broadcast(AgUiEvent::Interaction {
            session_id: session_id.to_string(),
            interaction,
            timestamp: Self::timestamp_millis(),
        });
    }

    /// Get the current timestamp in milliseconds (for interaction events).
    fn timestamp_millis() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
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
            AgUiEvent::Interaction { .. } => "interaction",
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

    #[tokio::test]
    async fn test_agui_state_render_a2ui() {
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

    #[tokio::test]
    async fn test_agui_state_render_without_clear() {
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

    #[tokio::test]
    async fn test_agui_state_render_with_clear() {
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

    #[tokio::test]
    async fn test_broadcast_event() {
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

    #[test]
    fn test_interaction_touch_event_serialization() {
        let event = InteractionEvent::Touch {
            element_id: Some("btn-1".to_string()),
            phase: "start".to_string(),
            x: 100.0,
            y: 200.0,
            pointer_id: 0,
        };

        let json = serde_json::to_string(&event).expect("should serialize");
        assert!(json.contains("touch"));
        assert!(json.contains("btn-1"));
        assert!(json.contains("start"));
        assert!(json.contains("100"));
    }

    #[test]
    fn test_interaction_button_click_serialization() {
        let event = InteractionEvent::ButtonClick {
            element_id: "submit-btn".to_string(),
            action: "form_submit".to_string(),
        };

        let json = serde_json::to_string(&event).expect("should serialize");
        assert!(json.contains("button_click"));
        assert!(json.contains("submit-btn"));
        assert!(json.contains("form_submit"));
    }

    #[test]
    fn test_interaction_form_input_serialization() {
        let event = InteractionEvent::FormInput {
            element_id: "name-input".to_string(),
            field: "username".to_string(),
            value: "alice".to_string(),
        };

        let json = serde_json::to_string(&event).expect("should serialize");
        assert!(json.contains("form_input"));
        assert!(json.contains("name-input"));
        assert!(json.contains("username"));
        assert!(json.contains("alice"));
    }

    #[test]
    fn test_interaction_event_deserialization() {
        // Touch event
        let json = r#"{"type":"touch","element_id":"el-1","phase":"move","x":50.5,"y":75.0,"pointer_id":1}"#;
        let event: InteractionEvent = serde_json::from_str(json).expect("should deserialize");
        match event {
            InteractionEvent::Touch {
                element_id,
                phase,
                x,
                y,
                pointer_id,
            } => {
                assert_eq!(element_id, Some("el-1".to_string()));
                assert_eq!(phase, "move");
                assert!((x - 50.5).abs() < 0.001);
                assert!((y - 75.0).abs() < 0.001);
                assert_eq!(pointer_id, 1);
            }
            _ => panic!("Expected Touch event"),
        }

        // Button click
        let json = r#"{"type":"button_click","element_id":"btn","action":"click"}"#;
        let event: InteractionEvent = serde_json::from_str(json).expect("should deserialize");
        match event {
            InteractionEvent::ButtonClick { element_id, action } => {
                assert_eq!(element_id, "btn");
                assert_eq!(action, "click");
            }
            _ => panic!("Expected ButtonClick event"),
        }
    }

    #[tokio::test]
    async fn test_broadcast_interaction() {
        let state = AgUiState::new(SyncState::new());
        let mut rx = state.event_tx.subscribe();

        // Broadcast an interaction
        let interaction = InteractionEvent::ButtonClick {
            element_id: "test-btn".to_string(),
            action: "submit".to_string(),
        };
        state.broadcast_interaction("session-1", interaction);

        // Should receive the interaction event
        let received = rx.try_recv().expect("should receive");
        match received {
            AgUiEvent::Interaction {
                session_id,
                interaction,
                timestamp,
            } => {
                assert_eq!(session_id, "session-1");
                assert!(timestamp > 0);
                match interaction {
                    InteractionEvent::ButtonClick { element_id, action } => {
                        assert_eq!(element_id, "test-btn");
                        assert_eq!(action, "submit");
                    }
                    _ => panic!("Expected ButtonClick interaction"),
                }
            }
            _ => panic!("Expected Interaction event"),
        }
    }

    #[test]
    fn test_agui_interaction_event_serialization() {
        let event = AgUiEvent::Interaction {
            session_id: "default".to_string(),
            interaction: InteractionEvent::Touch {
                element_id: None,
                phase: "end".to_string(),
                x: 0.0,
                y: 0.0,
                pointer_id: 0,
            },
            timestamp: 1234567890123,
        };

        let json = serde_json::to_string(&event).expect("should serialize");
        assert!(json.contains("interaction"));
        assert!(json.contains("default"));
        assert!(json.contains("touch"));
        assert!(json.contains("1234567890123"));
    }

    #[test]
    fn test_gesture_event_serialization() {
        let event = InteractionEvent::Gesture {
            gesture_type: "pinch".to_string(),
            scale: Some(1.5),
            rotation: None,
            center_x: 400.0,
            center_y: 300.0,
        };

        let json = serde_json::to_string(&event).expect("should serialize");
        assert!(json.contains("gesture"));
        assert!(json.contains("pinch"));
        assert!(json.contains("1.5"));
        // rotation should not be present when None
        assert!(!json.contains("rotation"));
    }
}
