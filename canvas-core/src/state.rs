//! Canvas state management.

use serde::{Deserialize, Serialize};

use crate::{InputEvent, Scene};

/// Connection status to the AI/MCP.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionStatus {
    /// Fully connected and operational.
    Connected,
    /// Attempting to connect.
    Connecting,
    /// Disconnected but can operate offline.
    Offline,
    /// Connection error.
    Error,
}

/// Interaction mode for direct manipulation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InteractionMode {
    /// Default mode - select and manipulate elements.
    Select,
    /// Pan/zoom the canvas.
    Pan,
    /// Drawing mode (for annotations).
    Draw,
    /// Voice command active.
    Voice,
}

/// The complete canvas state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasState {
    /// The scene graph.
    pub scene: Scene,
    /// Connection status to AI/MCP.
    pub connection: ConnectionStatus,
    /// Current interaction mode.
    pub mode: InteractionMode,
    /// Pending events to sync when reconnected.
    pending_sync: Vec<InputEvent>,
    /// Whether there are unsaved local changes.
    pub has_local_changes: bool,
}

impl CanvasState {
    /// Create a new canvas state with the given viewport size.
    #[must_use]
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            scene: Scene::new(width, height),
            connection: ConnectionStatus::Connecting,
            mode: InteractionMode::Select,
            pending_sync: Vec::new(),
            has_local_changes: false,
        }
    }

    /// Process an input event.
    pub fn process_event(&mut self, event: &InputEvent) {
        // If offline, queue for later sync
        if self.connection == ConnectionStatus::Offline {
            self.pending_sync.push(event.clone());
        }

        match event {
            InputEvent::Touch(touch) => {
                // Find element at touch point
                if let Some(primary) = touch.primary_touch() {
                    if let Some(element_id) = self.scene.element_at(primary.x, primary.y) {
                        tracing::debug!("Touch on element: {element_id}");
                    }
                }
            }
            InputEvent::Gesture(gesture) => {
                tracing::debug!("Gesture: {:?}", gesture);
            }
            InputEvent::Voice(voice) => {
                tracing::debug!("Voice command: {}", voice.transcript);
            }
            _ => {}
        }

        self.has_local_changes = true;
    }

    /// Set connection status.
    pub fn set_connection(&mut self, status: ConnectionStatus) {
        let was_offline = self.connection == ConnectionStatus::Offline;
        self.connection = status;

        // If we just reconnected, we might want to sync pending events
        if was_offline && status == ConnectionStatus::Connected {
            tracing::info!(
                "Reconnected with {} pending events to sync",
                self.pending_sync.len()
            );
        }
    }

    /// Get pending events to sync.
    #[must_use]
    pub fn pending_events(&self) -> &[InputEvent] {
        &self.pending_sync
    }

    /// Clear pending events after successful sync.
    pub fn clear_pending(&mut self) {
        self.pending_sync.clear();
    }

    /// Check if connected to an AI backend.
    #[must_use]
    pub fn is_connected(&self) -> bool {
        self.connection == ConnectionStatus::Connected
    }

    /// Get the current connection status.
    #[must_use]
    pub fn connection_status(&self) -> ConnectionStatus {
        self.connection
    }

    /// Check if we can perform interactive operations.
    #[must_use]
    pub fn can_interact(&self) -> bool {
        // Graceful degradation: allow some operations offline
        match self.connection {
            ConnectionStatus::Connected => true,
            ConnectionStatus::Offline => {
                // Allow view, pan, zoom, select locally
                matches!(self.mode, InteractionMode::Select | InteractionMode::Pan)
            }
            ConnectionStatus::Connecting | ConnectionStatus::Error => false,
        }
    }
}

impl Default for CanvasState {
    fn default() -> Self {
        Self::new(800.0, 600.0)
    }
}
