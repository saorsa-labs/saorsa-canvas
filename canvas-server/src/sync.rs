//! # WebSocket Scene Synchronization
//!
//! Real-time scene synchronization over WebSocket connections.
//!
//! ## Message Protocol
//!
//! ### Client -> Server (Scene)
//!
//! - `{"type": "subscribe", "session_id": "default"}`
//! - `{"type": "add_element", "element": {...}}`
//! - `{"type": "update_element", "id": "...", "changes": {...}}`
//! - `{"type": "remove_element", "id": "..."}`
//! - `{"type": "ping"}`
//! - `{"type": "sync_queue", "operations": [...]}`
//!
//! ### Client -> Server (WebRTC Signaling)
//!
//! - `{"type": "start_call", "target_peer_id": "...", "session_id": "..."}`
//! - `{"type": "offer", "target_peer_id": "...", "sdp": "..."}`
//! - `{"type": "answer", "target_peer_id": "...", "sdp": "..."}`
//! - `{"type": "ice_candidate", "target_peer_id": "...", "candidate": "..."}`
//! - `{"type": "end_call", "target_peer_id": "..."}`
//!
//! ### Server -> Client (Scene)
//!
//! - `{"type": "welcome", "version": "...", "session_id": "..."}`
//! - `{"type": "scene_update", "elements": [...]}`
//! - `{"type": "element_added", "element": {...}}`
//! - `{"type": "element_removed", "id": "..."}`
//! - `{"type": "ack", "message_id": "..."}`
//! - `{"type": "error", "code": "...", "message": "..."}`
//!
//! ### Server -> Client (WebRTC Signaling)
//!
//! - `{"type": "peer_assigned", "peer_id": "..."}`
//! - `{"type": "incoming_call", "from_peer_id": "...", "session_id": "..."}`
//! - `{"type": "relay_offer", "from_peer_id": "...", "sdp": "..."}`
//! - `{"type": "relay_answer", "from_peer_id": "...", "sdp": "..."}`
//! - `{"type": "relay_ice_candidate", "from_peer_id": "...", "candidate": "..."}`
//! - `{"type": "call_ended", "from_peer_id": "...", "reason": "..."}`

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use axum::extract::ws::{Message, WebSocket};
use canvas_core::{
    ConflictResolution, ConflictStrategy, Element, ElementDocument, ElementId, OfflineQueue,
    Operation, Scene, SceneDocument, SceneStore, StoreError,
};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::agui::InteractionEvent;
use crate::communitas::CommunitasMcpClient;
use crate::metrics::{record_rate_limited, record_validation_failure};
use crate::validation::{
    validate_element_id, validate_ice_candidate, validate_message_size, validate_peer_id,
    validate_sdp, validate_session_id, ValidationError,
};

/// Default burst capacity for rate limiting (messages).
const DEFAULT_RATE_LIMIT_BURST: u32 = 100;
/// Default sustained rate for rate limiting (messages per second).
const DEFAULT_RATE_LIMIT_SUSTAINED: u32 = 10;

/// Token bucket rate limiter for WebSocket connections.
///
/// Allows burst traffic up to `capacity` tokens, refilling at `refill_rate` tokens per second.
pub struct RateLimiter {
    /// Current number of available tokens.
    tokens: f64,
    /// Maximum token capacity (burst limit).
    capacity: f64,
    /// Tokens added per second (sustained rate).
    refill_rate: f64,
    /// Last time tokens were refilled.
    last_refill: Instant,
}

impl RateLimiter {
    /// Create a new rate limiter.
    ///
    /// # Arguments
    ///
    /// * `burst_capacity` - Maximum number of tokens (burst limit)
    /// * `sustained_rate` - Tokens added per second (sustained rate)
    #[must_use]
    pub fn new(burst_capacity: u32, sustained_rate: u32) -> Self {
        Self {
            tokens: f64::from(burst_capacity),
            capacity: f64::from(burst_capacity),
            refill_rate: f64::from(sustained_rate),
            last_refill: Instant::now(),
        }
    }

    /// Create a rate limiter from environment variables or defaults.
    ///
    /// Environment variables:
    /// - `WS_RATE_LIMIT_BURST`: Burst capacity (default: 100)
    /// - `WS_RATE_LIMIT_SUSTAINED`: Sustained rate per second (default: 10)
    #[must_use]
    pub fn from_env() -> Self {
        let burst = std::env::var("WS_RATE_LIMIT_BURST")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_RATE_LIMIT_BURST);
        let sustained = std::env::var("WS_RATE_LIMIT_SUSTAINED")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_RATE_LIMIT_SUSTAINED);
        Self::new(burst, sustained)
    }

    /// Try to consume one token. Returns true if allowed, false if rate limited.
    pub fn try_consume(&mut self) -> bool {
        self.refill();
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    /// Refill tokens based on elapsed time.
    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill);
        let new_tokens = elapsed.as_secs_f64() * self.refill_rate;
        self.tokens = (self.tokens + new_tokens).min(self.capacity);
        self.last_refill = now;
    }

    /// Get the time until the next token is available.
    ///
    /// Returns `None` if tokens are already available.
    #[must_use]
    pub fn time_until_available(&self) -> Option<Duration> {
        if self.tokens >= 1.0 {
            None
        } else {
            let needed = 1.0 - self.tokens;
            let seconds = needed / self.refill_rate;
            Some(Duration::from_secs_f64(seconds))
        }
    }
}

/// Client-to-server WebSocket message types.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    /// Subscribe to scene updates for a session.
    Subscribe {
        /// Session ID to subscribe to.
        #[serde(default = "default_session")]
        session_id: String,
    },
    /// Add a new element to the scene.
    AddElement {
        /// The element to add.
        element: ElementDocument,
        /// Optional message ID for acknowledgment.
        #[serde(default)]
        message_id: Option<String>,
    },
    /// Update an existing element.
    UpdateElement {
        /// Element ID to update.
        id: String,
        /// Changes to apply (partial element data).
        changes: serde_json::Value,
        /// Optional message ID for acknowledgment.
        #[serde(default)]
        message_id: Option<String>,
    },
    /// Remove an element from the scene.
    RemoveElement {
        /// Element ID to remove.
        id: String,
        /// Optional message ID for acknowledgment.
        #[serde(default)]
        message_id: Option<String>,
    },
    /// Ping to keep connection alive.
    Ping,
    /// Sync queued offline operations.
    SyncQueue {
        /// Queued operations to sync.
        operations: Vec<QueuedOperation>,
    },
    /// Request current scene state.
    GetScene,

    // === WebRTC Signaling Messages ===
    /// Start a call to a peer.
    StartCall {
        /// Target peer ID to call.
        target_peer_id: String,
        /// Session ID for the call.
        session_id: String,
    },
    /// SDP offer from caller.
    Offer {
        /// Target peer ID.
        target_peer_id: String,
        /// SDP offer string.
        sdp: String,
    },
    /// SDP answer from callee.
    Answer {
        /// Target peer ID.
        target_peer_id: String,
        /// SDP answer string.
        sdp: String,
    },
    /// ICE candidate exchange.
    IceCandidate {
        /// Target peer ID.
        target_peer_id: String,
        /// ICE candidate string.
        candidate: String,
        /// SDP media ID.
        #[serde(default)]
        sdp_mid: Option<String>,
        /// SDP media line index.
        #[serde(default)]
        sdp_m_line_index: Option<u16>,
    },
    /// End a call with a peer.
    EndCall {
        /// Target peer ID.
        target_peer_id: String,
    },

    // === Communitas Call Control Messages ===
    /// Start a new Communitas-backed call for the current session.
    StartCommunitasCall {
        /// Whether video should be enabled for this call.
        #[serde(default)]
        video_enabled: bool,
        /// Optional message ID for acknowledgment.
        #[serde(default)]
        message_id: Option<String>,
    },
    /// Join an existing Communitas call by call ID.
    JoinCommunitasCall {
        /// The call ID to join.
        call_id: String,
        /// Optional message ID for acknowledgment.
        #[serde(default)]
        message_id: Option<String>,
    },
    /// Leave the current Communitas call.
    LeaveCommunitasCall {
        /// Optional message ID for acknowledgment.
        #[serde(default)]
        message_id: Option<String>,
    },

    // === Interaction Events (AG-UI) ===
    /// Report a user interaction on the canvas.
    Interaction {
        /// Interaction type: "touch", "button_click", "form_input", "selection", "gesture".
        interaction_type: String,
        /// Element ID involved in the interaction (if any).
        #[serde(default)]
        element_id: Option<String>,
        /// Interaction-specific data.
        data: serde_json::Value,
        /// Optional message ID for acknowledgment.
        #[serde(default)]
        message_id: Option<String>,
    },
}

/// Server-to-client WebSocket message types.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    /// Welcome message on connection.
    Welcome {
        /// Server version.
        version: String,
        /// Assigned session ID.
        session_id: String,
        /// Connection timestamp.
        timestamp: u64,
        /// Whether legacy (browser-native) signaling is enabled.
        #[serde(skip_serializing_if = "Option::is_none")]
        legacy_signaling: Option<bool>,
    },
    /// Full scene state update.
    SceneUpdate {
        /// Canonical scene document.
        scene: SceneDocument,
    },
    /// Single element added to scene.
    ElementAdded {
        /// The added element.
        element: ElementDocument,
        /// Event timestamp.
        timestamp: u64,
    },
    /// Single element updated.
    ElementUpdated {
        /// The updated element data.
        element: ElementDocument,
        /// Event timestamp.
        timestamp: u64,
    },
    /// Single element removed from scene.
    ElementRemoved {
        /// ID of removed element.
        id: String,
        /// Event timestamp.
        timestamp: u64,
    },
    /// Acknowledgment of a client message.
    Ack {
        /// The message ID being acknowledged.
        message_id: String,
        /// Whether the operation succeeded.
        success: bool,
        /// Optional result data.
        #[serde(skip_serializing_if = "Option::is_none")]
        result: Option<serde_json::Value>,
    },
    /// Error response.
    Error {
        /// Error code.
        code: String,
        /// Human-readable error message.
        message: String,
        /// Related message ID if applicable.
        #[serde(skip_serializing_if = "Option::is_none")]
        message_id: Option<String>,
    },
    /// Pong response to ping.
    Pong {
        /// Response timestamp.
        timestamp: u64,
    },
    /// Sync result after processing queued operations.
    SyncResult {
        /// Number of operations synced.
        synced_count: usize,
        /// Number of conflicts encountered.
        conflict_count: usize,
        /// Event timestamp.
        timestamp: u64,
        /// Details of failed operations (up to 10).
        #[serde(skip_serializing_if = "Vec::is_empty")]
        failed_operations: Vec<FailedOperationInfo>,
    },
    /// Communitas call state update for this session.
    CallState {
        /// Session identifier for this call state.
        session_id: String,
        /// Active Communitas call ID (if established).
        #[serde(skip_serializing_if = "Option::is_none")]
        call_id: Option<String>,
        /// Active peer IDs participating in the call.
        participants: Vec<String>,
    },
    /// Result of a Communitas call operation.
    CommunitasCallResult {
        /// The operation that was performed.
        operation: String,
        /// Whether the operation succeeded.
        success: bool,
        /// Call ID if available.
        #[serde(skip_serializing_if = "Option::is_none")]
        call_id: Option<String>,
        /// Error message if operation failed.
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        /// Related message ID for acknowledgment.
        #[serde(skip_serializing_if = "Option::is_none")]
        message_id: Option<String>,
    },

    // === WebRTC Signaling Messages ===
    /// Incoming call notification.
    IncomingCall {
        /// Peer ID initiating the call.
        from_peer_id: String,
        /// Session ID for the call.
        session_id: String,
    },
    /// Relay SDP offer to target peer.
    RelayOffer {
        /// Peer ID sending the offer.
        from_peer_id: String,
        /// SDP offer string.
        sdp: String,
    },
    /// Relay SDP answer to target peer.
    RelayAnswer {
        /// Peer ID sending the answer.
        from_peer_id: String,
        /// SDP answer string.
        sdp: String,
    },
    /// Relay ICE candidate to target peer.
    RelayIceCandidate {
        /// Peer ID sending the candidate.
        from_peer_id: String,
        /// ICE candidate string.
        candidate: String,
        /// SDP media ID.
        #[serde(skip_serializing_if = "Option::is_none")]
        sdp_mid: Option<String>,
        /// SDP media line index.
        #[serde(skip_serializing_if = "Option::is_none")]
        sdp_m_line_index: Option<u16>,
    },
    /// Call ended notification.
    CallEnded {
        /// Peer ID that ended the call.
        from_peer_id: String,
        /// Reason for call ending.
        reason: String,
    },
    /// Your assigned peer ID (sent on connection).
    PeerAssigned {
        /// Assigned unique peer ID.
        peer_id: String,
    },
}

/// A queued operation from offline queue.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum QueuedOperation {
    /// Add element operation.
    Add {
        /// Element to add.
        element: ElementDocument,
        /// Operation timestamp.
        timestamp: u64,
    },
    /// Update element operation.
    Update {
        /// Element ID.
        id: String,
        /// Changes to apply.
        changes: serde_json::Value,
        /// Operation timestamp.
        timestamp: u64,
    },
    /// Remove element operation.
    Remove {
        /// Element ID.
        id: String,
        /// Operation timestamp.
        timestamp: u64,
    },
}

impl QueuedOperation {
    /// Get the type of this operation.
    #[must_use]
    pub fn operation_type(&self) -> OperationType {
        match self {
            Self::Add { .. } => OperationType::Add,
            Self::Update { .. } => OperationType::Update,
            Self::Remove { .. } => OperationType::Remove,
        }
    }

    /// Get the element ID affected by this operation (if any).
    #[must_use]
    pub fn element_id(&self) -> Option<&str> {
        match self {
            Self::Add { element, .. } => Some(&element.id),
            Self::Update { id, .. } | Self::Remove { id, .. } => Some(id),
        }
    }
}

/// Type of sync operation.
///
/// Provides type-safe representation of operation types rather than using strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationType {
    /// Add a new element.
    Add,
    /// Update an existing element.
    Update,
    /// Remove an element.
    Remove,
}

impl std::fmt::Display for OperationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Add => write!(f, "add"),
            Self::Update => write!(f, "update"),
            Self::Remove => write!(f, "remove"),
        }
    }
}

/// Information about a failed sync operation sent to clients.
///
/// This struct provides minimal but useful details about operations
/// that failed during queue processing, enabling client-side reconciliation.
#[derive(Debug, Clone, Serialize)]
pub struct FailedOperationInfo {
    /// Type of operation that failed.
    pub operation: OperationType,
    /// Element ID involved in the failed operation, if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub element_id: Option<String>,
    /// Human-readable error message.
    pub error: String,
}

impl FailedOperationInfo {
    /// Maximum number of failed operations to include in a sync result.
    pub const MAX_FAILURES_IN_RESPONSE: usize = 10;

    /// Create from a failed operation and its error message.
    #[must_use]
    pub fn from_failed_op(op: &QueuedOperation, error: &str) -> Self {
        Self {
            operation: op.operation_type(),
            element_id: op.element_id().map(String::from),
            error: error.to_string(),
        }
    }
}

/// Result of processing queued offline operations.
#[derive(Debug, Clone)]
pub struct ProcessQueueResult {
    /// Number of operations successfully processed.
    pub processed_count: usize,
    /// Number of operations that failed.
    pub failed_count: usize,
    /// Failed operations with error messages.
    pub failed_ops: Vec<(QueuedOperation, String)>,
    /// Processing timestamp.
    pub timestamp: u64,
}

impl ProcessQueueResult {
    /// Convert to a ServerMessage for broadcasting.
    ///
    /// Failed operations are included up to a maximum of 10 to avoid
    /// excessively large payloads while still providing useful debug info.
    #[must_use]
    pub fn into_server_message(self) -> ServerMessage {
        let failed_operations: Vec<FailedOperationInfo> = self
            .failed_ops
            .iter()
            .take(FailedOperationInfo::MAX_FAILURES_IN_RESPONSE)
            .map(|(op, err)| FailedOperationInfo::from_failed_op(op, err))
            .collect();

        ServerMessage::SyncResult {
            synced_count: self.processed_count,
            conflict_count: self.failed_count,
            timestamp: self.timestamp,
            failed_operations,
        }
    }
}

/// Origin of a scene event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncOrigin {
    /// Local mutation originating from this server.
    Local,
    /// Remote mutation applied from an upstream source.
    Remote,
}

/// Event broadcast to connected WebSocket clients.
#[derive(Debug, Clone)]
pub struct SyncEvent {
    /// Session ID this event applies to.
    pub session_id: String,
    /// The message to broadcast.
    pub message: ServerMessage,
    /// Where the event originated.
    pub origin: SyncOrigin,
}

/// Information about a connected peer.
#[derive(Debug)]
pub struct PeerInfo {
    /// Session the peer is subscribed to.
    pub session_id: String,
    /// Channel to send messages to this peer.
    pub sender: mpsc::UnboundedSender<ServerMessage>,
}

/// Registry of connected peers for signaling.
type PeerRegistry = Arc<RwLock<HashMap<String, PeerInfo>>>;

/// Active Communitas call metadata per session.
#[derive(Debug, Clone)]
struct ActiveCall {
    /// Call identifier assigned by Communitas.
    call_id: Option<String>,
    /// Entity/channel identifier mirrored from Communitas (defaults to session ID).
    entity_id: String,
    /// Connected peer IDs participating in this call.
    participants: HashSet<String>,
}

impl ActiveCall {
    fn new(session_id: &str) -> Self {
        Self {
            call_id: None,
            entity_id: session_id.to_string(),
            participants: HashSet::new(),
        }
    }
}

/// Lightweight snapshot for broadcasting to clients.
#[derive(Debug, Clone, Default)]
struct CallSnapshot {
    call_id: Option<String>,
    participants: Vec<String>,
}

/// Shared state for WebSocket synchronization.
///
/// Wraps a [`SceneStore`] and adds broadcast notifications for real-time sync.
#[derive(Clone)]
pub struct SyncState {
    /// Scene storage delegated to SceneStore.
    store: SceneStore,
    /// Broadcast channel for sync events.
    event_tx: broadcast::Sender<SyncEvent>,
    /// Broadcast channel for interaction events (session_id, event).
    interaction_tx: broadcast::Sender<(String, InteractionEvent)>,
    /// Offline queue for reconnection support.
    #[allow(dead_code)]
    offline_queue: Arc<RwLock<OfflineQueue>>,
    /// Registry of connected peers for WebRTC signaling.
    peers: PeerRegistry,
    /// Active Communitas-backed call state keyed by session.
    active_calls: Arc<RwLock<HashMap<String, ActiveCall>>>,
    /// Optional Communitas MCP client for upstream call management.
    communitas: Arc<RwLock<Option<CommunitasMcpClient>>>,
    /// Counter for sync conflicts/failures.
    conflict_count: Arc<AtomicU64>,
}

impl SyncState {
    /// Create a new sync state.
    #[allow(dead_code)]
    #[must_use]
    pub fn new() -> Self {
        let (event_tx, _) = broadcast::channel(100);
        let (interaction_tx, _) = broadcast::channel(100);
        Self {
            store: SceneStore::new(),
            event_tx,
            interaction_tx,
            offline_queue: Arc::new(RwLock::new(OfflineQueue::new())),
            peers: Arc::new(RwLock::new(HashMap::new())),
            active_calls: Arc::new(RwLock::new(HashMap::new())),
            communitas: Arc::new(RwLock::new(None)),
            conflict_count: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Create a new sync state with filesystem persistence.
    ///
    /// Sessions are saved as JSON files in `data_dir` and auto-saved on every
    /// mutation. The directory is created if it doesn't exist.
    ///
    /// # Errors
    ///
    /// Returns an error if the data directory cannot be created.
    pub fn with_data_dir(
        data_dir: impl Into<std::path::PathBuf>,
    ) -> Result<Self, canvas_core::StoreError> {
        let (event_tx, _) = broadcast::channel(100);
        let (interaction_tx, _) = broadcast::channel(100);
        Ok(Self {
            store: SceneStore::with_data_dir(data_dir)?,
            event_tx,
            interaction_tx,
            offline_queue: Arc::new(RwLock::new(OfflineQueue::new())),
            peers: Arc::new(RwLock::new(HashMap::new())),
            active_calls: Arc::new(RwLock::new(HashMap::new())),
            communitas: Arc::new(RwLock::new(None)),
            conflict_count: Arc::new(AtomicU64::new(0)),
        })
    }

    /// Install a Communitas MCP client for upstream media coordination.
    pub fn set_communitas_client(&self, client: CommunitasMcpClient) {
        match self.communitas.write() {
            Ok(mut guard) => {
                *guard = Some(client);
                tracing::info!("Communitas MCP client installed");
            }
            Err(e) => {
                tracing::error!(
                    "Failed to set Communitas client: lock poisoned ({}). \
                     Legacy signaling will remain enabled.",
                    e
                );
            }
        }
    }

    /// Returns true when a Communitas MCP client is configured.
    pub fn communitas_enabled(&self) -> bool {
        match self.communitas.read() {
            Ok(guard) => guard.is_some(),
            Err(e) => {
                tracing::error!(
                    "Failed to check communitas_enabled: lock poisoned ({}). \
                     Assuming disabled for safety.",
                    e
                );
                false
            }
        }
    }

    /// Returns true if legacy WebRTC signaling should remain available.
    pub fn legacy_signaling_enabled(&self) -> bool {
        !self.communitas_enabled()
    }

    fn communitas_client(&self) -> Option<CommunitasMcpClient> {
        match self.communitas.read() {
            Ok(guard) => guard.clone(),
            Err(e) => {
                tracing::error!("Failed to get Communitas client: lock poisoned ({})", e);
                None
            }
        }
    }

    fn call_snapshot(&self, session_id: &str) -> CallSnapshot {
        match self.active_calls.read() {
            Ok(calls) => {
                if let Some(call) = calls.get(session_id) {
                    let mut participants: Vec<_> = call.participants.iter().cloned().collect();
                    participants.sort();
                    return CallSnapshot {
                        call_id: call.call_id.clone(),
                        participants,
                    };
                }
                CallSnapshot::default()
            }
            Err(e) => {
                tracing::error!(
                    session_id = %session_id,
                    "Failed to get call snapshot: lock poisoned ({})",
                    e
                );
                CallSnapshot::default()
            }
        }
    }

    fn broadcast_call_state(&self, session_id: &str) {
        let snapshot = self.call_snapshot(session_id);
        let message = ServerMessage::CallState {
            session_id: session_id.to_string(),
            call_id: snapshot.call_id,
            participants: snapshot.participants,
        };
        self.broadcast(session_id, message, SyncOrigin::Local);
    }

    /// Track a peer joining the active session call (creating it if needed).
    ///
    /// This method:
    /// - Creates a new call via `start_voice_call` if no call exists
    /// - Joins the existing call via `join_call` if a call is already active
    pub fn add_call_participant(&self, session_id: &str, peer_id: &str) {
        let (should_start, should_join) = {
            let mut calls = match self.active_calls.write() {
                Ok(c) => c,
                Err(_) => {
                    tracing::error!("Failed to acquire calls lock for add_call_participant");
                    return;
                }
            };

            let entry = calls
                .entry(session_id.to_string())
                .or_insert_with(|| ActiveCall::new(session_id));

            // Only proceed if this is a new participant
            if !entry.participants.insert(peer_id.to_string()) {
                // Already a participant
                (false, None)
            } else if entry.call_id.is_none() {
                // First participant, need to start a call
                (true, None)
            } else {
                // Existing call, need to join
                (false, entry.call_id.clone())
            }
        };

        self.broadcast_call_state(session_id);

        if should_start {
            self.spawn_communitas_start(session_id.to_string(), peer_id.to_string());
        } else if let Some(call_id) = should_join {
            self.spawn_communitas_join(session_id.to_string(), peer_id.to_string(), call_id);
        }
    }

    /// Track a peer leaving the session call.
    ///
    /// This method always invokes `end_call` upstream for the leaving peer,
    /// ensuring proper cleanup on the Communitas side.
    pub fn remove_call_participant(&self, session_id: &str, peer_id: &str) {
        let call_id = {
            let mut calls = match self.active_calls.write() {
                Ok(c) => c,
                Err(_) => {
                    tracing::error!("Failed to acquire calls lock for remove_call_participant");
                    return;
                }
            };

            if let Some(call) = calls.get_mut(session_id) {
                call.participants.remove(peer_id);
                let call_id = call.call_id.clone();

                // Remove the call entry if no participants left
                if call.participants.is_empty() {
                    calls.remove(session_id);
                }

                call_id
            } else {
                None
            }
        };

        self.broadcast_call_state(session_id);

        // Always notify upstream when a peer leaves (not just the last one)
        if let Some(call_id) = call_id {
            self.spawn_communitas_leave(session_id.to_string(), peer_id.to_string(), call_id);
        }
    }

    fn set_call_metadata(&self, session_id: &str, call_id: String, entity_id: String) {
        let updated = match self.active_calls.write() {
            Ok(mut calls) => {
                if let Some(entry) = calls.get_mut(session_id) {
                    entry.call_id = Some(call_id);
                    entry.entity_id = entity_id;
                    true
                } else {
                    false
                }
            }
            Err(e) => {
                tracing::error!(
                    session_id = %session_id,
                    "Failed to set call metadata: lock poisoned ({})",
                    e
                );
                false
            }
        };
        if updated {
            self.broadcast_call_state(session_id);
        }
    }

    fn spawn_communitas_start(&self, session_id: String, peer_id: String) {
        if let Some(client) = self.communitas_client() {
            let state = self.clone();
            tokio::spawn(async move {
                match client.start_voice_call(&session_id, true).await {
                    Ok(result) => {
                        state.set_call_metadata(
                            &session_id,
                            result.call_id.clone(),
                            result.entity_id,
                        );
                        tracing::info!(
                            "Peer {} started Communitas call {} in session {}",
                            peer_id,
                            result.call_id,
                            session_id
                        );
                    }
                    Err(err) => {
                        tracing::warn!(
                            "Failed to start Communitas call for peer {} in session {}: {}",
                            peer_id,
                            session_id,
                            err
                        );
                        // Notify the peer of the failure
                        state.send_to_peer(
                            &peer_id,
                            ServerMessage::CommunitasCallResult {
                                operation: "start".to_string(),
                                success: false,
                                call_id: None,
                                error: Some(err.to_string()),
                                message_id: None,
                            },
                        );
                    }
                }
            });
        }
    }

    fn spawn_communitas_join(&self, session_id: String, peer_id: String, call_id: String) {
        if let Some(client) = self.communitas_client() {
            let state = self.clone();
            let call_id_clone = call_id.clone();
            tokio::spawn(async move {
                match client.join_call(&call_id_clone).await {
                    Ok(result) => {
                        if result.success {
                            tracing::info!(
                                "Peer {} joined Communitas call {} in session {}",
                                peer_id,
                                call_id_clone,
                                session_id
                            );
                        } else {
                            tracing::warn!(
                                "join_call returned failure for peer {} in call {}",
                                peer_id,
                                call_id_clone
                            );
                            state.send_to_peer(
                                &peer_id,
                                ServerMessage::CommunitasCallResult {
                                    operation: "join".to_string(),
                                    success: false,
                                    call_id: Some(call_id_clone),
                                    error: Some("join_call returned failure".to_string()),
                                    message_id: None,
                                },
                            );
                        }
                    }
                    Err(err) => {
                        tracing::warn!(
                            "Failed to join Communitas call {} for peer {} in session {}: {}",
                            call_id_clone,
                            peer_id,
                            session_id,
                            err
                        );
                        state.send_to_peer(
                            &peer_id,
                            ServerMessage::CommunitasCallResult {
                                operation: "join".to_string(),
                                success: false,
                                call_id: Some(call_id_clone),
                                error: Some(err.to_string()),
                                message_id: None,
                            },
                        );
                    }
                }
            });
        }
    }

    fn spawn_communitas_leave(&self, session_id: String, peer_id: String, call_id: String) {
        if let Some(client) = self.communitas_client() {
            let state = self.clone();
            let call_id_clone = call_id.clone();
            tokio::spawn(async move {
                match client.end_call(&call_id_clone).await {
                    Ok(result) => {
                        if result.success {
                            tracing::info!(
                                "Peer {} left Communitas call {} in session {}",
                                peer_id,
                                call_id_clone,
                                session_id
                            );
                        } else {
                            tracing::warn!(
                                "end_call returned failure for peer {} leaving call {}",
                                peer_id,
                                call_id_clone
                            );
                        }
                    }
                    Err(err) => {
                        tracing::warn!(
                            "Failed to end Communitas call {} for peer {} leaving session {}: {}",
                            call_id_clone,
                            peer_id,
                            session_id,
                            err
                        );
                        // Notify peer of leave failure (may help with retry logic)
                        state.send_to_peer(
                            &peer_id,
                            ServerMessage::CommunitasCallResult {
                                operation: "leave".to_string(),
                                success: false,
                                call_id: Some(call_id_clone),
                                error: Some(err.to_string()),
                                message_id: None,
                            },
                        );
                    }
                }
            });
        }
    }

    /// Start a new Communitas call for the session.
    ///
    /// Returns the call ID on success, or an error message on failure.
    pub async fn start_communitas_call_async(
        &self,
        session_id: &str,
        peer_id: &str,
        video_enabled: bool,
    ) -> Result<String, String> {
        let client = self
            .communitas_client()
            .ok_or_else(|| "Communitas not configured".to_string())?;

        let result = client
            .start_voice_call(session_id, video_enabled)
            .await
            .map_err(|e| e.to_string())?;

        // Update call metadata
        self.set_call_metadata(session_id, result.call_id.clone(), result.entity_id);
        // Add this peer as participant
        match self.active_calls.write() {
            Ok(mut calls) => {
                if let Some(call) = calls.get_mut(session_id) {
                    call.participants.insert(peer_id.to_string());
                }
            }
            Err(e) => {
                tracing::error!(
                    session_id = %session_id,
                    peer_id = %peer_id,
                    "Failed to add participant to call: lock poisoned ({})",
                    e
                );
            }
        }
        self.broadcast_call_state(session_id);

        tracing::info!(
            "Started Communitas call {} for session {} (video={})",
            result.call_id,
            session_id,
            video_enabled
        );

        Ok(result.call_id)
    }

    /// Join an existing Communitas call.
    ///
    /// Returns Ok on success, or an error message on failure.
    pub async fn join_communitas_call_async(
        &self,
        session_id: &str,
        peer_id: &str,
        call_id: &str,
    ) -> Result<(), String> {
        let client = self
            .communitas_client()
            .ok_or_else(|| "Communitas not configured".to_string())?;

        let result = client.join_call(call_id).await.map_err(|e| e.to_string())?;

        if !result.success {
            return Err("join_call returned failure".to_string());
        }

        // Add participant to local state
        match self.active_calls.write() {
            Ok(mut calls) => {
                let entry = calls
                    .entry(session_id.to_string())
                    .or_insert_with(|| ActiveCall::new(session_id));
                entry.call_id = Some(call_id.to_string());
                entry.participants.insert(peer_id.to_string());
            }
            Err(e) => {
                tracing::error!(
                    session_id = %session_id,
                    peer_id = %peer_id,
                    call_id = %call_id,
                    "Failed to add participant after join: lock poisoned ({})",
                    e
                );
            }
        }
        self.broadcast_call_state(session_id);

        tracing::info!(
            "Peer {} joined Communitas call {} in session {}",
            peer_id,
            call_id,
            session_id
        );

        Ok(())
    }

    /// Leave the current Communitas call.
    ///
    /// Returns Ok on success, or an error message on failure.
    pub async fn leave_communitas_call_async(
        &self,
        session_id: &str,
        peer_id: &str,
    ) -> Result<(), String> {
        let call_id = {
            let calls = self.active_calls.read().map_err(|_| "lock poisoned")?;
            calls
                .get(session_id)
                .and_then(|c| c.call_id.clone())
                .ok_or_else(|| "no active call in session".to_string())?
        };

        let client = self
            .communitas_client()
            .ok_or_else(|| "Communitas not configured".to_string())?;

        // Call end_call to signal we're leaving
        let result = client.end_call(&call_id).await.map_err(|e| e.to_string())?;

        if !result.success {
            tracing::warn!(
                "end_call returned failure for peer {} leaving call {}",
                peer_id,
                call_id
            );
        }

        // Remove participant from local state
        let mut should_end = false;
        match self.active_calls.write() {
            Ok(mut calls) => {
                if let Some(call) = calls.get_mut(session_id) {
                    call.participants.remove(peer_id);
                    if call.participants.is_empty() {
                        should_end = true;
                        calls.remove(session_id);
                    }
                }
            }
            Err(e) => {
                tracing::error!(
                    session_id = %session_id,
                    peer_id = %peer_id,
                    call_id = %call_id,
                    "Failed to remove participant on leave: lock poisoned ({})",
                    e
                );
            }
        }
        self.broadcast_call_state(session_id);

        tracing::info!(
            "Peer {} left Communitas call {} in session {} (ended={})",
            peer_id,
            call_id,
            session_id,
            should_end
        );

        Ok(())
    }

    /// Clear the Communitas client reference, re-enabling legacy signaling.
    pub fn clear_communitas_client(&self) {
        match self.communitas.write() {
            Ok(mut guard) => {
                *guard = None;
                tracing::info!("Cleared Communitas client, legacy signaling re-enabled");
            }
            Err(e) => {
                tracing::error!("Failed to clear Communitas client: lock poisoned ({})", e);
            }
        }
    }

    /// Register a peer connection.
    ///
    /// Returns a receiver for messages sent to this peer.
    pub fn register_peer(
        &self,
        peer_id: &str,
        session_id: &str,
    ) -> mpsc::UnboundedReceiver<ServerMessage> {
        let (tx, rx) = mpsc::unbounded_channel();
        match self.peers.write() {
            Ok(mut peers) => {
                peers.insert(
                    peer_id.to_string(),
                    PeerInfo {
                        session_id: session_id.to_string(),
                        sender: tx,
                    },
                );
                tracing::info!("Registered peer {} in session {}", peer_id, session_id);
            }
            Err(e) => {
                tracing::error!(
                    peer_id = %peer_id,
                    session_id = %session_id,
                    "Failed to register peer: lock poisoned ({}). \
                     Peer will not receive messages.",
                    e
                );
            }
        }
        rx
    }

    /// Update a peer's session.
    pub fn update_peer_session(&self, peer_id: &str, session_id: &str) {
        let mut previous: Option<String> = None;
        match self.peers.write() {
            Ok(mut peers) => {
                if let Some(info) = peers.get_mut(peer_id) {
                    if info.session_id != session_id {
                        previous = Some(info.session_id.clone());
                        info.session_id = session_id.to_string();
                        tracing::debug!("Updated peer {} to session {}", peer_id, session_id);
                    }
                }
            }
            Err(e) => {
                tracing::error!(
                    peer_id = %peer_id,
                    session_id = %session_id,
                    "Failed to update peer session: lock poisoned ({})",
                    e
                );
            }
        }
        if let Some(old_session) = previous {
            self.remove_call_participant(&old_session, peer_id);
        }
    }

    /// Unregister a peer connection.
    pub fn unregister_peer(&self, peer_id: &str) {
        let session = match self.peers.write() {
            Ok(mut peers) => peers.remove(peer_id).map(|info| info.session_id),
            Err(e) => {
                tracing::error!(
                    peer_id = %peer_id,
                    "Failed to unregister peer: lock poisoned ({})",
                    e
                );
                None
            }
        };
        if let Some(session_id) = session {
            tracing::info!("Unregistered peer {} from session {}", peer_id, session_id);
            self.remove_call_participant(&session_id, peer_id);
        } else {
            tracing::info!("Unregistered peer {}", peer_id);
        }
    }

    /// Send a message to a specific peer.
    ///
    /// Returns true if the peer exists and the message was queued.
    pub fn send_to_peer(&self, peer_id: &str, message: ServerMessage) -> bool {
        match self.peers.read() {
            Ok(peers) => {
                if let Some(info) = peers.get(peer_id) {
                    return info.sender.send(message).is_ok();
                }
                false
            }
            Err(e) => {
                tracing::error!(
                    peer_id = %peer_id,
                    "Failed to send to peer: lock poisoned ({})",
                    e
                );
                false
            }
        }
    }

    /// Get the session ID for a peer.
    #[must_use]
    pub fn get_peer_session(&self, peer_id: &str) -> Option<String> {
        match self.peers.read() {
            Ok(peers) => peers.get(peer_id).map(|info| info.session_id.clone()),
            Err(e) => {
                tracing::error!(
                    peer_id = %peer_id,
                    "Failed to get peer session: lock poisoned ({})",
                    e
                );
                None
            }
        }
    }

    /// Check if a peer is in the same session as another peer.
    #[must_use]
    pub fn peers_in_same_session(&self, peer_a: &str, peer_b: &str) -> bool {
        match self.peers.read() {
            Ok(peers) => match (peers.get(peer_a), peers.get(peer_b)) {
                (Some(a), Some(b)) => a.session_id == b.session_id,
                _ => false,
            },
            Err(e) => {
                tracing::error!(
                    peer_a = %peer_a,
                    peer_b = %peer_b,
                    "Failed to check peers_in_same_session: lock poisoned ({})",
                    e
                );
                false
            }
        }
    }
    /// Get the underlying `SceneStore` for sharing with MCP.
    #[allow(dead_code)]
    #[must_use]
    pub fn store(&self) -> SceneStore {
        self.store.clone()
    }

    /// Subscribe to sync events.
    #[allow(dead_code)]
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<SyncEvent> {
        self.event_tx.subscribe()
    }

    /// Get the broadcast sender.
    #[allow(dead_code)]
    #[must_use]
    pub fn sender(&self) -> broadcast::Sender<SyncEvent> {
        self.event_tx.clone()
    }

    /// Subscribe to interaction events.
    ///
    /// Returns a receiver that yields `(session_id, InteractionEvent)` tuples
    /// for each interaction event broadcast by WebSocket clients.
    #[must_use]
    pub fn subscribe_interactions(&self) -> broadcast::Receiver<(String, InteractionEvent)> {
        self.interaction_tx.subscribe()
    }

    /// Broadcast an interaction event from a WebSocket client.
    ///
    /// This allows AG-UI clients to receive user interactions in real-time.
    pub fn broadcast_interaction(&self, session_id: &str, interaction: InteractionEvent) {
        // Ignore send errors (no receivers is okay)
        let _ = self
            .interaction_tx
            .send((session_id.to_string(), interaction));
    }

    /// Get or create a scene for the given session ID.
    #[must_use]
    pub fn get_or_create_scene(&self, session_id: &str) -> Scene {
        self.store.get_or_create(session_id)
    }

    /// Replace the entire scene for a session with a new snapshot.
    ///
    /// # Errors
    ///
    /// Returns [`SyncError`] if the store operation fails.
    pub fn replace_scene(
        &self,
        session_id: &str,
        scene: Scene,
        origin: SyncOrigin,
    ) -> Result<(), SyncError> {
        self.store.replace(session_id, scene.clone())?;

        let document = SceneDocument::from_scene(session_id, &scene, current_timestamp());
        self.broadcast(
            session_id,
            ServerMessage::SceneUpdate { scene: document },
            origin,
        );
        Ok(())
    }

    /// Get a scene by session ID.
    #[allow(dead_code)]
    #[must_use]
    pub fn get_scene(&self, session_id: &str) -> Option<Scene> {
        self.store.get(session_id)
    }

    /// Get the current scene document for a session.
    #[must_use]
    pub fn scene_document(&self, session_id: &str) -> SceneDocument {
        self.store.scene_document(session_id)
    }

    /// Update a scene and broadcast the change.
    ///
    /// # Errors
    ///
    /// Returns [`SyncError`] if the session is not found.
    pub fn update_scene<F>(&self, session_id: &str, f: F) -> Result<(), SyncError>
    where
        F: FnOnce(&mut Scene),
    {
        // Ensure the scene exists first
        let _ = self.store.get_or_create(session_id);

        self.store.update(session_id, f)?;

        let document = self.store.scene_document(session_id);
        self.broadcast(
            session_id,
            ServerMessage::SceneUpdate { scene: document },
            SyncOrigin::Local,
        );
        Ok(())
    }

    /// Add an element to a session's scene.
    ///
    /// # Errors
    ///
    /// Returns [`SyncError`] if the element data is invalid.
    pub fn add_element(
        &self,
        session_id: &str,
        element_data: &ElementDocument,
    ) -> Result<ElementId, SyncError> {
        let element = element_from_data(element_data)?;
        let id = element.id;

        self.store.add_element(session_id, element)?;

        // Broadcast the addition
        let message = ServerMessage::ElementAdded {
            element: element_data.clone(),
            timestamp: current_timestamp(),
        };
        self.broadcast(session_id, message, SyncOrigin::Local);

        // Also broadcast full scene update
        let document = self.store.scene_document(session_id);
        self.broadcast(
            session_id,
            ServerMessage::SceneUpdate { scene: document },
            SyncOrigin::Local,
        );

        Ok(id)
    }

    /// Remove an element from a session's scene.
    ///
    /// # Errors
    ///
    /// Returns [`SyncError`] if the element is not found or the ID is invalid.
    pub fn remove_element(&self, session_id: &str, id: &str) -> Result<(), SyncError> {
        let element_id = parse_element_id(id)?;

        self.store.remove_element(session_id, element_id)?;

        // Broadcast the removal
        let message = ServerMessage::ElementRemoved {
            id: id.to_string(),
            timestamp: current_timestamp(),
        };
        self.broadcast(session_id, message, SyncOrigin::Local);

        Ok(())
    }

    /// Update an element in a session's scene.
    ///
    /// # Errors
    ///
    /// Returns [`SyncError`] if the element is not found or the ID is invalid.
    pub fn update_element(
        &self,
        session_id: &str,
        id: &str,
        changes: &serde_json::Value,
    ) -> Result<ElementDocument, SyncError> {
        let element_id = parse_element_id(id)?;

        // Clone changes for the closure
        let changes_clone = changes.clone();

        self.store
            .update_element(session_id, element_id, |element| {
                apply_changes_to_element(element, &changes_clone);
            })?;

        // Get the updated element for the response
        let scene = self
            .store
            .get(session_id)
            .ok_or_else(|| SyncError::SessionNotFound(session_id.to_string()))?;
        let element = scene
            .get_element(element_id)
            .ok_or_else(|| SyncError::ElementNotFound(id.to_string()))?;
        let updated_element = element_to_data(element);

        // Broadcast the update
        let message = ServerMessage::ElementUpdated {
            element: updated_element.clone(),
            timestamp: current_timestamp(),
        };
        self.broadcast(session_id, message, SyncOrigin::Local);

        Ok(updated_element)
    }

    /// Get full scene state as a server message.
    #[must_use]
    pub fn get_scene_update(&self, session_id: &str) -> ServerMessage {
        let document = self.store.scene_document(session_id);
        ServerMessage::SceneUpdate { scene: document }
    }

    /// Process queued offline operations with full error tracking.
    ///
    /// Returns a detailed result with processed/failed counts and error details
    /// for operations that could not be applied.
    #[must_use]
    pub fn process_queue(
        &self,
        session_id: &str,
        operations: Vec<QueuedOperation>,
    ) -> ProcessQueueResult {
        let mut processed_count = 0;
        let mut failed_ops: Vec<(QueuedOperation, String)> = Vec::new();

        for op in operations {
            let result = match &op {
                QueuedOperation::Add { element, .. } => {
                    self.add_element(session_id, element).map(|_| ())
                }
                QueuedOperation::Update { id, changes, .. } => {
                    self.update_element(session_id, id, changes).map(|_| ())
                }
                QueuedOperation::Remove { id, .. } => self.remove_element(session_id, id),
            };

            match result {
                Ok(()) => processed_count += 1,
                Err(err) => {
                    let error_msg = err.to_string();
                    tracing::warn!(
                        "process_queue failed for op {:?}: {}",
                        op.operation_type(),
                        error_msg
                    );
                    failed_ops.push((op, error_msg));
                }
            }
        }

        let failed_count = failed_ops.len();

        // Update conflict counter
        if failed_count > 0 {
            self.conflict_count
                .fetch_add(failed_count as u64, Ordering::Relaxed);
        }

        ProcessQueueResult {
            processed_count,
            failed_count,
            failed_ops,
            timestamp: current_timestamp(),
        }
    }

    /// Get total conflict count since server start.
    #[must_use]
    pub fn total_conflict_count(&self) -> u64 {
        self.conflict_count.load(Ordering::Relaxed)
    }

    /// Broadcast a message to all clients subscribed to a session.
    fn broadcast(&self, session_id: &str, message: ServerMessage, origin: SyncOrigin) {
        let event = SyncEvent {
            session_id: session_id.to_string(),
            message,
            origin,
        };
        if let Err(e) = self.event_tx.send(event) {
            // No receivers is expected during startup or when no clients are connected.
            // Log at debug level to aid troubleshooting without spamming logs.
            tracing::debug!(
                session_id = %session_id,
                "Broadcast skipped: no receivers ({})",
                e
            );
        }
    }
}

impl Default for SyncState {
    fn default() -> Self {
        Self::new()
    }
}

/// Errors that can occur during sync operations.
#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    /// Lock was poisoned.
    #[error("Internal lock error")]
    LockPoisoned,
    /// Element not found.
    #[error("Element not found: {0}")]
    ElementNotFound(String),
    /// Session not found.
    #[error("Session not found: {0}")]
    SessionNotFound(String),
    /// Invalid element ID format.
    #[error("Invalid element ID: {0}")]
    InvalidElementId(String),
    /// Invalid message format.
    #[error("Invalid message: {0}")]
    #[allow(dead_code)]
    InvalidMessage(String),
    /// Serialization error.
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

impl From<StoreError> for SyncError {
    fn from(e: StoreError) -> Self {
        match e {
            StoreError::LockPoisoned => SyncError::LockPoisoned,
            StoreError::SessionNotFound(s) => SyncError::SessionNotFound(s),
            StoreError::ElementNotFound(s) => SyncError::ElementNotFound(s),
            StoreError::SceneError(s) => SyncError::InvalidMessage(s),
            StoreError::Io(e) => SyncError::InvalidMessage(e.to_string()),
            StoreError::Serialization(s) => SyncError::InvalidMessage(s),
        }
    }
}

/// A failed operation with its error message.
///
/// This struct captures detailed information about why an operation failed,
/// including whether the failure is retryable (transient) or permanent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailedOperation {
    /// The operation that failed.
    pub operation: Operation,
    /// Human-readable error description.
    pub error: String,
    /// Whether this failure is retryable.
    ///
    /// - `true`: Transient failure (network issues, temporary conflicts) - client should retry
    /// - `false`: Permanent failure (invalid data, authorization) - no point retrying
    pub retryable: bool,
}

impl FailedOperation {
    /// Create a new failed operation.
    #[must_use]
    pub const fn new(operation: Operation, error: String, retryable: bool) -> Self {
        Self {
            operation,
            error,
            retryable,
        }
    }

    /// Create a retryable failed operation (transient failure).
    #[must_use]
    pub const fn retryable(operation: Operation, error: String) -> Self {
        Self::new(operation, error, true)
    }

    /// Create a non-retryable failed operation (permanent failure).
    #[must_use]
    pub const fn permanent(operation: Operation, error: String) -> Self {
        Self::new(operation, error, false)
    }
}

/// Reason why a conflict was detected during sync.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConflictReason {
    /// Tried to update or remove an element that does not exist.
    ElementNotFound,
    /// Tried to add an element with an ID that already exists.
    ElementAlreadyExists,
    /// Operation has a timestamp older than the current element state.
    StaleTimestamp {
        /// Timestamp from the local/current state.
        local: u64,
        /// Timestamp from the incoming remote operation.
        remote: u64,
    },
    /// Multiple operations modified the same element concurrently.
    ConcurrentModification,
}

impl std::fmt::Display for ConflictReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ElementNotFound => write!(f, "element not found"),
            Self::ElementAlreadyExists => write!(f, "element already exists"),
            Self::StaleTimestamp { local, remote } => {
                write!(f, "stale timestamp (local={}, remote={})", local, remote)
            }
            Self::ConcurrentModification => write!(f, "concurrent modification"),
        }
    }
}

/// A detected conflict during sync processing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conflict {
    /// The operation that caused the conflict.
    pub operation: Operation,
    /// The reason for the conflict.
    pub reason: ConflictReason,
    /// Whether this conflict has been resolved.
    pub resolved: bool,
}

impl Conflict {
    /// Create a new unresolved conflict.
    #[must_use]
    pub fn new(operation: Operation, reason: ConflictReason) -> Self {
        Self {
            operation,
            reason,
            resolved: false,
        }
    }

    /// Mark this conflict as resolved.
    pub fn mark_resolved(&mut self) {
        self.resolved = true;
    }
}

/// Result from batch operation processing.
///
/// Contains detailed metrics about the sync operation including timing,
/// success/failure counts, and categorization of failures.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncProcessorResult {
    /// Whether the overall sync operation succeeded (no failures).
    pub success: bool,
    /// Number of operations successfully synced.
    pub synced_count: usize,
    /// Number of conflicts detected during processing.
    pub conflict_count: usize,
    /// Number of operations that failed.
    pub failed_count: usize,
    /// Conflicts detected during processing.
    pub conflicts: Vec<Conflict>,
    /// Details of failed operations (for debugging/retry decisions).
    pub failed_operations: Vec<FailedOperation>,
    /// Duration of the sync operation in milliseconds.
    pub duration_ms: u64,
    /// Timestamp when the sync completed (ms since Unix epoch).
    pub timestamp: u64,
}

impl Default for SyncProcessorResult {
    fn default() -> Self {
        Self {
            success: true,
            synced_count: 0,
            conflict_count: 0,
            failed_count: 0,
            conflicts: Vec::new(),
            failed_operations: Vec::new(),
            duration_ms: 0,
            timestamp: 0,
        }
    }
}

impl SyncProcessorResult {
    /// Create a new result with the current timestamp.
    #[must_use]
    pub fn new() -> Self {
        Self {
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
            ..Default::default()
        }
    }

    /// Record a successful operation.
    pub fn record_success(&mut self) {
        self.synced_count += 1;
    }

    /// Record a failed operation.
    pub fn record_failure(&mut self, failed_op: FailedOperation) {
        self.failed_count += 1;
        self.failed_operations.push(failed_op);
        self.success = false;
    }

    /// Record a conflict.
    pub fn record_conflict(&mut self, conflict: Conflict) {
        self.conflict_count += 1;
        self.conflicts.push(conflict);
    }

    /// Finalize the result with duration.
    pub fn finalize(&mut self, start_time: Instant) {
        self.duration_ms = start_time.elapsed().as_millis() as u64;
        self.timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
    }

    /// Get the number of retryable failures.
    #[must_use]
    pub fn retryable_count(&self) -> usize {
        self.failed_operations
            .iter()
            .filter(|op| op.retryable)
            .count()
    }

    /// Get the number of permanent failures.
    #[must_use]
    pub fn permanent_count(&self) -> usize {
        self.failed_operations
            .iter()
            .filter(|op| !op.retryable)
            .count()
    }
}

/// Configuration for retry behavior with exponential backoff.
///
/// Controls how failed operations are retried, with increasing delays
/// between attempts to avoid overwhelming the system during transient failures.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    /// Maximum number of retry attempts before giving up.
    pub max_retries: u32,
    /// Initial delay between retries in milliseconds.
    pub initial_delay_ms: u64,
    /// Maximum delay between retries in milliseconds (cap for backoff).
    pub max_delay_ms: u64,
    /// Multiplier applied to delay after each retry (e.g., 2.0 for doubling).
    pub backoff_multiplier: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_delay_ms: 100,
            max_delay_ms: 5000,
            backoff_multiplier: 2.0,
        }
    }
}

impl RetryConfig {
    /// Create a new retry configuration with custom values.
    #[must_use]
    pub const fn new(
        max_retries: u32,
        initial_delay_ms: u64,
        max_delay_ms: u64,
        backoff_multiplier: f64,
    ) -> Self {
        Self {
            max_retries,
            initial_delay_ms,
            max_delay_ms,
            backoff_multiplier,
        }
    }

    /// Calculate the delay for a given retry attempt (0-indexed).
    ///
    /// Uses exponential backoff: `delay = initial * multiplier^attempt`
    /// The result is capped at `max_delay_ms`.
    #[must_use]
    pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
        let base_delay = self.initial_delay_ms as f64;
        let multiplier = self.backoff_multiplier.powi(attempt as i32);
        let delay_ms = (base_delay * multiplier).min(self.max_delay_ms as f64) as u64;
        Duration::from_millis(delay_ms)
    }
}

/// Processes batched scene operations with conflict resolution.
///
/// The `SyncProcessor` handles batch processing of operations from clients,
/// applying them to the scene store with the configured conflict strategy.
pub struct SyncProcessor {
    /// Scene storage reference.
    store: Arc<SceneStore>,
    /// Conflict resolution strategy.
    conflict_strategy: ConflictStrategy,
    /// Tracks the last known modification timestamp for each element.
    element_timestamps: Arc<RwLock<HashMap<ElementId, u64>>>,
}

impl SyncProcessor {
    /// Create a new sync processor.
    ///
    /// # Arguments
    ///
    /// * `store` - The scene store to apply operations to.
    /// * `strategy` - The conflict resolution strategy to use.
    #[must_use]
    pub fn new(store: Arc<SceneStore>, strategy: ConflictStrategy) -> Self {
        Self {
            store,
            conflict_strategy: strategy,
            element_timestamps: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get the current conflict strategy.
    #[must_use]
    pub const fn conflict_strategy(&self) -> ConflictStrategy {
        self.conflict_strategy
    }

    /// Set a new conflict strategy.
    pub fn set_conflict_strategy(&mut self, strategy: ConflictStrategy) {
        self.conflict_strategy = strategy;
    }

    /// Process a batch of operations for a session.
    ///
    /// Iterates through the operations and applies them to the scene store.
    /// Returns a result indicating how many operations succeeded or failed,
    /// including details of any failures for debugging or retry decisions.
    ///
    /// # Arguments
    ///
    /// * `session_id` - The session to apply operations to.
    /// * `operations` - The operations to process.
    pub fn process_batch(
        &self,
        session_id: &str,
        operations: Vec<Operation>,
    ) -> SyncProcessorResult {
        let start_time = Instant::now();
        let mut result = SyncProcessorResult::new();

        for operation in operations {
            // Check for conflicts before applying
            if let Some(mut conflict) = self.detect_conflict(session_id, &operation) {
                let resolution = self.resolve_conflict(&conflict);
                conflict.mark_resolved();
                result.record_conflict(conflict.clone());

                match resolution {
                    ConflictResolution::KeepLocal => {
                        // Skip this operation, count as conflict - retryable since state may change
                        result.record_failure(FailedOperation::retryable(
                            operation.clone(),
                            format!("Conflict resolved: {}", conflict.reason),
                        ));
                        continue;
                    }
                    ConflictResolution::KeepRemote => {
                        // Proceed to apply the operation
                    }
                    ConflictResolution::Merge => {
                        // Merge not yet implemented - not retryable since merge will always fail
                        result.record_failure(FailedOperation::permanent(
                            operation.clone(),
                            "Merge conflict resolution not implemented".to_string(),
                        ));
                        continue;
                    }
                }
            }

            // Apply the operation
            match self.apply_operation(session_id, &operation) {
                Ok(()) => {
                    result.record_success();
                    // Update timestamp tracking
                    match &operation {
                        Operation::AddElement { element, timestamp } => {
                            self.update_timestamp(element.id, *timestamp);
                        }
                        Operation::UpdateElement { id, timestamp, .. } => {
                            self.update_timestamp(*id, *timestamp);
                        }
                        Operation::RemoveElement { id, .. } => {
                            self.remove_timestamp(id);
                        }
                        Operation::Interaction { .. } => {}
                    }
                }
                Err(e) => {
                    // Store errors are generally retryable (transient issues)
                    // except for specific permanent failures like missing resources
                    // or lock poisoning which indicate unrecoverable state
                    let retryable = !matches!(
                        &e,
                        SyncError::LockPoisoned
                            | SyncError::ElementNotFound(_)
                            | SyncError::SessionNotFound(_)
                    );
                    result.record_failure(FailedOperation::new(
                        operation.clone(),
                        e.to_string(),
                        retryable,
                    ));
                }
            }
        }

        result.finalize(start_time);
        result
    }

    /// Apply a single operation to the store.
    fn apply_operation(&self, session_id: &str, operation: &Operation) -> Result<(), SyncError> {
        match operation {
            Operation::AddElement { element, .. } => {
                tracing::debug!(
                    session_id = %session_id,
                    element_id = %element.id,
                    "apply_operation: AddElement"
                );
                self.store.add_element(session_id, element.clone())?;
                tracing::debug!(
                    session_id = %session_id,
                    element_id = %element.id,
                    "apply_operation: AddElement succeeded"
                );
            }
            Operation::UpdateElement { id, changes, .. } => {
                tracing::debug!(
                    session_id = %session_id,
                    element_id = %id,
                    changes = %changes,
                    "apply_operation: UpdateElement"
                );
                // Clone changes for the closure
                let changes_clone = changes.clone();
                self.store.update_element(session_id, *id, |element| {
                    apply_changes_to_element(element, &changes_clone);
                })?;
                tracing::debug!(
                    session_id = %session_id,
                    element_id = %id,
                    "apply_operation: UpdateElement succeeded"
                );
            }
            Operation::RemoveElement { id, .. } => {
                tracing::debug!(
                    session_id = %session_id,
                    element_id = %id,
                    "apply_operation: RemoveElement"
                );
                self.store.remove_element(session_id, *id)?;
                tracing::debug!(
                    session_id = %session_id,
                    element_id = %id,
                    "apply_operation: RemoveElement succeeded"
                );
            }
            Operation::Interaction { event, .. } => {
                // Interactions are events, not stored state changes.
                // They can be processed/broadcast but don't modify the store directly.
                tracing::debug!(
                    session_id = %session_id,
                    event_type = ?event,
                    "apply_operation: Interaction (logged only, no store modification)"
                );
            }
        }
        Ok(())
    }

    /// Detect if an operation would cause a conflict.
    ///
    /// Checks the current state to determine if the operation can be applied
    /// without conflict. Returns `Some(Conflict)` if a conflict is detected,
    /// `None` otherwise.
    ///
    /// # Arguments
    ///
    /// * `session_id` - The session to check against.
    /// * `op` - The operation to check.
    fn detect_conflict(&self, session_id: &str, op: &Operation) -> Option<Conflict> {
        let scene = self.store.get(session_id);

        match op {
            Operation::AddElement { element, timestamp } => {
                // Check if element already exists
                if let Some(ref s) = scene {
                    if s.get_element(element.id).is_some() {
                        return Some(Conflict::new(
                            op.clone(),
                            ConflictReason::ElementAlreadyExists,
                        ));
                    }
                }
                // Check for stale timestamp
                if let Ok(timestamps) = self.element_timestamps.read() {
                    if let Some(&local_ts) = timestamps.get(&element.id) {
                        if *timestamp < local_ts {
                            return Some(Conflict::new(
                                op.clone(),
                                ConflictReason::StaleTimestamp {
                                    local: local_ts,
                                    remote: *timestamp,
                                },
                            ));
                        }
                    }
                }
                None
            }
            Operation::UpdateElement { id, timestamp, .. } => {
                // Check if element exists
                match &scene {
                    Some(s) if s.get_element(*id).is_none() => {
                        return Some(Conflict::new(op.clone(), ConflictReason::ElementNotFound));
                    }
                    None => {
                        return Some(Conflict::new(op.clone(), ConflictReason::ElementNotFound));
                    }
                    _ => {}
                }
                // Check for stale timestamp
                if let Ok(timestamps) = self.element_timestamps.read() {
                    if let Some(&local_ts) = timestamps.get(id) {
                        if *timestamp < local_ts {
                            return Some(Conflict::new(
                                op.clone(),
                                ConflictReason::StaleTimestamp {
                                    local: local_ts,
                                    remote: *timestamp,
                                },
                            ));
                        }
                    }
                }
                None
            }
            Operation::RemoveElement { id, timestamp } => {
                // Check if element exists
                match &scene {
                    Some(s) if s.get_element(*id).is_none() => {
                        return Some(Conflict::new(op.clone(), ConflictReason::ElementNotFound));
                    }
                    None => {
                        return Some(Conflict::new(op.clone(), ConflictReason::ElementNotFound));
                    }
                    _ => {}
                }
                // Check for stale timestamp
                if let Ok(timestamps) = self.element_timestamps.read() {
                    if let Some(&local_ts) = timestamps.get(id) {
                        if *timestamp < local_ts {
                            return Some(Conflict::new(
                                op.clone(),
                                ConflictReason::StaleTimestamp {
                                    local: local_ts,
                                    remote: *timestamp,
                                },
                            ));
                        }
                    }
                }
                None
            }
            Operation::Interaction { .. } => {
                // Interactions do not cause conflicts
                None
            }
        }
    }

    /// Resolve a conflict using the configured strategy.
    ///
    /// Returns the resolution decision based on the conflict reason and
    /// the processor's conflict strategy.
    ///
    /// # Arguments
    ///
    /// * `conflict` - The conflict to resolve.
    #[must_use]
    pub fn resolve_conflict(&self, conflict: &Conflict) -> ConflictResolution {
        match &conflict.reason {
            ConflictReason::ElementNotFound => {
                // Element not found - cannot apply operation regardless of strategy
                tracing::debug!(
                    reason = %conflict.reason,
                    "Conflict: element not found, skipping operation"
                );
                ConflictResolution::KeepLocal
            }
            ConflictReason::ElementAlreadyExists => {
                // Element already exists - use strategy to decide
                match self.conflict_strategy {
                    ConflictStrategy::LastWriteWins | ConflictStrategy::RemoteWins => {
                        tracing::debug!(
                            strategy = ?self.conflict_strategy,
                            reason = %conflict.reason,
                            "Conflict: element exists, overwriting with remote"
                        );
                        ConflictResolution::KeepRemote
                    }
                    ConflictStrategy::LocalWins => {
                        tracing::debug!(
                            strategy = ?self.conflict_strategy,
                            reason = %conflict.reason,
                            "Conflict: element exists, keeping local"
                        );
                        ConflictResolution::KeepLocal
                    }
                }
            }
            ConflictReason::StaleTimestamp { local, remote } => {
                match self.conflict_strategy {
                    ConflictStrategy::LastWriteWins => {
                        // Already detected as stale, keep local
                        tracing::debug!(
                            local_ts = local,
                            remote_ts = remote,
                            "Conflict: stale timestamp, keeping local (LastWriteWins)"
                        );
                        ConflictResolution::KeepLocal
                    }
                    ConflictStrategy::LocalWins => {
                        tracing::debug!(
                            local_ts = local,
                            remote_ts = remote,
                            "Conflict: keeping local (LocalWins strategy)"
                        );
                        ConflictResolution::KeepLocal
                    }
                    ConflictStrategy::RemoteWins => {
                        tracing::debug!(
                            local_ts = local,
                            remote_ts = remote,
                            "Conflict: applying remote despite stale timestamp (RemoteWins)"
                        );
                        ConflictResolution::KeepRemote
                    }
                }
            }
            ConflictReason::ConcurrentModification => {
                // Use strategy to decide
                match self.conflict_strategy {
                    ConflictStrategy::LastWriteWins | ConflictStrategy::RemoteWins => {
                        tracing::debug!(
                            strategy = ?self.conflict_strategy,
                            "Conflict: concurrent modification, applying remote"
                        );
                        ConflictResolution::KeepRemote
                    }
                    ConflictStrategy::LocalWins => {
                        tracing::debug!(
                            strategy = ?self.conflict_strategy,
                            "Conflict: concurrent modification, keeping local"
                        );
                        ConflictResolution::KeepLocal
                    }
                }
            }
        }
    }

    /// Update the timestamp for an element after a successful operation.
    fn update_timestamp(&self, element_id: ElementId, timestamp: u64) {
        if let Ok(mut timestamps) = self.element_timestamps.write() {
            timestamps.insert(element_id, timestamp);
        }
    }

    /// Remove the timestamp for an element after removal.
    fn remove_timestamp(&self, element_id: &ElementId) {
        if let Ok(mut timestamps) = self.element_timestamps.write() {
            timestamps.remove(element_id);
        }
    }

    /// Process operations with automatic retry for transient failures.
    ///
    /// This method implements exponential backoff for retrying operations
    /// that fail due to transient issues. Only operations marked as retryable
    /// will be retried; permanent failures are returned immediately.
    ///
    /// # Arguments
    ///
    /// * `session_id` - The session to apply operations to.
    /// * `operations` - The operations to process.
    /// * `config` - Retry configuration controlling backoff behavior.
    ///
    /// # Returns
    ///
    /// A combined result from all retry attempts. The `synced_count` reflects
    /// total successful operations across all attempts. Failed operations
    /// include only those that exhausted all retries or were not retryable.
    pub async fn process_with_retry(
        &self,
        session_id: &str,
        operations: Vec<Operation>,
        config: &RetryConfig,
    ) -> SyncProcessorResult {
        let overall_start = Instant::now();
        let mut combined_result = SyncProcessorResult::new();
        let mut pending_ops = operations;
        let mut attempt = 0u32;

        while !pending_ops.is_empty() && attempt <= config.max_retries {
            if attempt > 0 {
                // Apply backoff delay before retry
                let delay = config.delay_for_attempt(attempt - 1);
                tracing::debug!(
                    session_id = %session_id,
                    attempt = attempt,
                    delay_ms = %delay.as_millis(),
                    pending_count = pending_ops.len(),
                    "Retrying failed operations after backoff"
                );
                tokio::time::sleep(delay).await;
            }

            let batch_result = self.process_batch(session_id, pending_ops);

            // Accumulate successful operations
            combined_result.synced_count += batch_result.synced_count;
            combined_result.conflict_count += batch_result.conflict_count;
            combined_result.conflicts.extend(batch_result.conflicts);

            // Separate retryable from permanent failures
            let mut retry_ops = Vec::new();
            for failed in batch_result.failed_operations {
                if failed.retryable && attempt < config.max_retries {
                    // Queue for retry
                    retry_ops.push(failed.operation);
                } else {
                    // Record as final failure
                    combined_result.record_failure(failed);
                }
            }

            pending_ops = retry_ops;
            attempt += 1;
        }

        // Any remaining pending ops after max retries are failures
        // (This case is handled in the loop, but added for safety)
        for op in pending_ops {
            combined_result.record_failure(FailedOperation::permanent(
                op,
                format!("Exceeded maximum retries ({})", config.max_retries),
            ));
        }

        combined_result.finalize(overall_start);
        combined_result
    }
}

/// Client connection state.
pub struct ClientConnection {
    /// This client's unique peer ID.
    peer_id: String,
    /// Subscribed session ID.
    session_id: String,
    /// Sync state reference.
    state: SyncState,
    /// Event receiver for broadcasts.
    #[allow(dead_code)]
    event_rx: broadcast::Receiver<SyncEvent>,
}

impl ClientConnection {
    /// Create a new client connection with a generated peer ID.
    #[allow(dead_code)]
    #[must_use]
    pub fn new(state: SyncState) -> Self {
        let event_rx = state.subscribe();
        Self {
            peer_id: Uuid::new_v4().to_string(),
            session_id: "default".to_string(),
            state,
            event_rx,
        }
    }

    /// Create a new client connection with a specific peer ID.
    #[must_use]
    pub fn with_peer_id(state: SyncState, peer_id: String) -> Self {
        let event_rx = state.subscribe();
        Self {
            peer_id,
            session_id: "default".to_string(),
            state,
            event_rx,
        }
    }

    /// Get this client's peer ID.
    #[must_use]
    pub fn peer_id(&self) -> &str {
        &self.peer_id
    }

    /// Create a validation error response.
    fn validation_error(err: &ValidationError, message_id: Option<String>) -> ServerMessage {
        ServerMessage::Error {
            code: "validation_error".to_string(),
            message: err.to_string(),
            message_id,
        }
    }

    /// Error returned when legacy signaling is disabled.
    fn legacy_disabled_error(message_id: Option<String>) -> ServerMessage {
        ServerMessage::Error {
            code: "legacy_signaling_disabled".to_string(),
            message: "Legacy WebRTC signaling is disabled; Communitas handles calls upstream."
                .to_string(),
            message_id,
        }
    }

    /// Handle an incoming client message.
    pub fn handle_message(&mut self, msg: ClientMessage) -> Option<ServerMessage> {
        match msg {
            ClientMessage::Subscribe { session_id } => {
                // Validate session_id
                if let Err(e) = validate_session_id(&session_id) {
                    tracing::warn!("Invalid session_id from peer {}: {}", self.peer_id, e);
                    record_validation_failure("session_id");
                    return Some(Self::validation_error(&e, None));
                }
                self.session_id = session_id.clone();
                // Send current scene state
                Some(self.state.get_scene_update(&self.session_id))
            }
            ClientMessage::AddElement {
                element,
                message_id,
            } => {
                let result = self.state.add_element(&self.session_id, &element);
                message_id.map(|mid| match result {
                    Ok(id) => ServerMessage::Ack {
                        message_id: mid,
                        success: true,
                        result: Some(serde_json::json!({ "id": id.to_string() })),
                    },
                    Err(e) => ServerMessage::Error {
                        code: "add_failed".to_string(),
                        message: e.to_string(),
                        message_id: Some(mid),
                    },
                })
            }
            ClientMessage::UpdateElement {
                id,
                changes,
                message_id,
            } => {
                // Validate element_id
                if let Err(e) = validate_element_id(&id) {
                    tracing::warn!("Invalid element_id from peer {}: {}", self.peer_id, e);
                    record_validation_failure("element_id");
                    return Some(Self::validation_error(&e, message_id));
                }
                let result = self.state.update_element(&self.session_id, &id, &changes);
                message_id.map(|mid| match result {
                    Ok(_) => ServerMessage::Ack {
                        message_id: mid,
                        success: true,
                        result: None,
                    },
                    Err(e) => ServerMessage::Error {
                        code: "update_failed".to_string(),
                        message: e.to_string(),
                        message_id: Some(mid),
                    },
                })
            }
            ClientMessage::RemoveElement { id, message_id } => {
                // Validate element_id
                if let Err(e) = validate_element_id(&id) {
                    tracing::warn!("Invalid element_id from peer {}: {}", self.peer_id, e);
                    record_validation_failure("element_id");
                    return Some(Self::validation_error(&e, message_id));
                }
                let result = self.state.remove_element(&self.session_id, &id);
                message_id.map(|mid| match result {
                    Ok(()) => ServerMessage::Ack {
                        message_id: mid,
                        success: true,
                        result: None,
                    },
                    Err(e) => ServerMessage::Error {
                        code: "remove_failed".to_string(),
                        message: e.to_string(),
                        message_id: Some(mid),
                    },
                })
            }
            ClientMessage::Ping => Some(ServerMessage::Pong {
                timestamp: current_timestamp(),
            }),
            ClientMessage::SyncQueue { operations } => {
                let result = self.state.process_queue(&self.session_id, operations);
                // Log any failed operations for debugging
                for (op, err) in &result.failed_ops {
                    tracing::debug!(
                        "Client {} sync op {} failed: {}",
                        self.peer_id,
                        op.operation_type(),
                        err
                    );
                }
                Some(result.into_server_message())
            }
            ClientMessage::GetScene => Some(self.state.get_scene_update(&self.session_id)),

            // WebRTC signaling messages - relay to target peer
            ClientMessage::StartCall {
                target_peer_id,
                session_id,
            } => {
                // Validate session_id
                if let Err(e) = validate_session_id(&session_id) {
                    tracing::warn!("Invalid session_id from peer {}: {}", self.peer_id, e);
                    record_validation_failure("session_id");
                    return Some(Self::validation_error(&e, None));
                }

                if session_id != self.session_id {
                    return Some(ServerMessage::Error {
                        code: "invalid_session".to_string(),
                        message: "Cannot start call outside active session".to_string(),
                        message_id: None,
                    });
                }

                if !self.state.legacy_signaling_enabled() {
                    tracing::info!(
                        "Peer {} joining Communitas-managed call for session {}",
                        self.peer_id,
                        self.session_id
                    );
                    self.state
                        .add_call_participant(&self.session_id, &self.peer_id);
                    return None;
                }

                // Validate peer_id when legacy signaling is enabled
                if let Err(e) = validate_peer_id(&target_peer_id) {
                    tracing::warn!("Invalid target_peer_id from peer {}: {}", self.peer_id, e);
                    record_validation_failure("peer_id");
                    return Some(Self::validation_error(&e, None));
                }

                // Verify target is in same session
                if !self
                    .state
                    .peers_in_same_session(&self.peer_id, &target_peer_id)
                {
                    tracing::warn!(
                        "Peer {} tried to call {} but not in same session",
                        self.peer_id,
                        target_peer_id
                    );
                    return Some(ServerMessage::Error {
                        code: "peer_not_found".to_string(),
                        message: "Target peer not found in session".to_string(),
                        message_id: None,
                    });
                }

                tracing::info!("Peer {} starting call to {}", self.peer_id, target_peer_id);
                let sent = self.state.send_to_peer(
                    &target_peer_id,
                    ServerMessage::IncomingCall {
                        from_peer_id: self.peer_id.clone(),
                        session_id,
                    },
                );
                if sent {
                    self.state
                        .add_call_participant(&self.session_id, &self.peer_id);
                    None
                } else {
                    Some(ServerMessage::Error {
                        code: "peer_not_found".to_string(),
                        message: "Target peer is no longer connected".to_string(),
                        message_id: None,
                    })
                }
            }

            ClientMessage::Offer {
                target_peer_id,
                sdp,
            } => {
                if !self.state.legacy_signaling_enabled() {
                    return Some(Self::legacy_disabled_error(None));
                }
                // Validate peer_id and SDP
                if let Err(e) = validate_peer_id(&target_peer_id) {
                    tracing::warn!("Invalid target_peer_id from peer {}: {}", self.peer_id, e);
                    record_validation_failure("peer_id");
                    return Some(Self::validation_error(&e, None));
                }
                if let Err(e) = validate_sdp(&sdp) {
                    tracing::warn!("Invalid SDP from peer {}: {}", self.peer_id, e);
                    record_validation_failure("sdp");
                    return Some(Self::validation_error(&e, None));
                }

                tracing::debug!("Relaying offer from {} to {}", self.peer_id, target_peer_id);
                self.state.send_to_peer(
                    &target_peer_id,
                    ServerMessage::RelayOffer {
                        from_peer_id: self.peer_id.clone(),
                        sdp,
                    },
                );
                None
            }

            ClientMessage::Answer {
                target_peer_id,
                sdp,
            } => {
                if !self.state.legacy_signaling_enabled() {
                    self.state
                        .add_call_participant(&self.session_id, &self.peer_id);
                    return Some(Self::legacy_disabled_error(None));
                }
                // Validate peer_id and SDP
                if let Err(e) = validate_peer_id(&target_peer_id) {
                    tracing::warn!("Invalid target_peer_id from peer {}: {}", self.peer_id, e);
                    record_validation_failure("peer_id");
                    return Some(Self::validation_error(&e, None));
                }
                if let Err(e) = validate_sdp(&sdp) {
                    tracing::warn!("Invalid SDP from peer {}: {}", self.peer_id, e);
                    record_validation_failure("sdp");
                    return Some(Self::validation_error(&e, None));
                }

                tracing::debug!(
                    "Relaying answer from {} to {}",
                    self.peer_id,
                    target_peer_id
                );
                self.state
                    .add_call_participant(&self.session_id, &self.peer_id);
                self.state.send_to_peer(
                    &target_peer_id,
                    ServerMessage::RelayAnswer {
                        from_peer_id: self.peer_id.clone(),
                        sdp,
                    },
                );
                None
            }

            ClientMessage::IceCandidate {
                target_peer_id,
                candidate,
                sdp_mid,
                sdp_m_line_index,
            } => {
                if !self.state.legacy_signaling_enabled() {
                    return Some(Self::legacy_disabled_error(None));
                }
                // Validate peer_id and ICE candidate
                if let Err(e) = validate_peer_id(&target_peer_id) {
                    tracing::warn!("Invalid target_peer_id from peer {}: {}", self.peer_id, e);
                    record_validation_failure("peer_id");
                    return Some(Self::validation_error(&e, None));
                }
                if let Err(e) = validate_ice_candidate(&candidate) {
                    tracing::warn!("Invalid ICE candidate from peer {}: {}", self.peer_id, e);
                    record_validation_failure("ice_candidate");
                    return Some(Self::validation_error(&e, None));
                }

                tracing::debug!(
                    "Relaying ICE candidate from {} to {}",
                    self.peer_id,
                    target_peer_id
                );
                self.state.send_to_peer(
                    &target_peer_id,
                    ServerMessage::RelayIceCandidate {
                        from_peer_id: self.peer_id.clone(),
                        candidate,
                        sdp_mid,
                        sdp_m_line_index,
                    },
                );
                None
            }

            ClientMessage::EndCall { target_peer_id } => {
                if !self.state.legacy_signaling_enabled() {
                    tracing::info!("Peer {} leaving Communitas-managed call", self.peer_id);
                    self.state
                        .remove_call_participant(&self.session_id, &self.peer_id);
                    return None;
                }
                // Validate peer_id
                if let Err(e) = validate_peer_id(&target_peer_id) {
                    tracing::warn!("Invalid target_peer_id from peer {}: {}", self.peer_id, e);
                    record_validation_failure("peer_id");
                    return Some(Self::validation_error(&e, None));
                }

                tracing::info!("Peer {} ending call with {}", self.peer_id, target_peer_id);
                self.state
                    .remove_call_participant(&self.session_id, &self.peer_id);
                self.state.send_to_peer(
                    &target_peer_id,
                    ServerMessage::CallEnded {
                        from_peer_id: self.peer_id.clone(),
                        reason: "peer_hangup".to_string(),
                    },
                );
                None
            }

            // === Communitas Call Control Messages ===
            ClientMessage::StartCommunitasCall {
                video_enabled,
                message_id,
            } => {
                if !self.state.communitas_enabled() {
                    return Some(ServerMessage::CommunitasCallResult {
                        operation: "start".to_string(),
                        success: false,
                        call_id: None,
                        error: Some("Communitas not configured; use legacy signaling".to_string()),
                        message_id,
                    });
                }

                // Spawn async task to start the call and send result
                let state = self.state.clone();
                let peer_id = self.peer_id.clone();
                let session_id = self.session_id.clone();
                tokio::spawn(async move {
                    let result = state
                        .start_communitas_call_async(&session_id, &peer_id, video_enabled)
                        .await;
                    let message = match result {
                        Ok(call_id) => ServerMessage::CommunitasCallResult {
                            operation: "start".to_string(),
                            success: true,
                            call_id: Some(call_id),
                            error: None,
                            message_id,
                        },
                        Err(e) => ServerMessage::CommunitasCallResult {
                            operation: "start".to_string(),
                            success: false,
                            call_id: None,
                            error: Some(e),
                            message_id,
                        },
                    };
                    state.send_to_peer(&peer_id, message);
                });
                None
            }

            ClientMessage::JoinCommunitasCall {
                call_id,
                message_id,
            } => {
                if !self.state.communitas_enabled() {
                    return Some(ServerMessage::CommunitasCallResult {
                        operation: "join".to_string(),
                        success: false,
                        call_id: None,
                        error: Some("Communitas not configured; use legacy signaling".to_string()),
                        message_id,
                    });
                }

                // Spawn async task to join the call and send result
                let state = self.state.clone();
                let peer_id = self.peer_id.clone();
                let session_id = self.session_id.clone();
                let call_id_clone = call_id.clone();
                tokio::spawn(async move {
                    let result = state
                        .join_communitas_call_async(&session_id, &peer_id, &call_id_clone)
                        .await;
                    let message = match result {
                        Ok(()) => ServerMessage::CommunitasCallResult {
                            operation: "join".to_string(),
                            success: true,
                            call_id: Some(call_id_clone),
                            error: None,
                            message_id,
                        },
                        Err(e) => ServerMessage::CommunitasCallResult {
                            operation: "join".to_string(),
                            success: false,
                            call_id: Some(call_id_clone),
                            error: Some(e),
                            message_id,
                        },
                    };
                    state.send_to_peer(&peer_id, message);
                });
                None
            }

            ClientMessage::LeaveCommunitasCall { message_id } => {
                if !self.state.communitas_enabled() {
                    return Some(ServerMessage::CommunitasCallResult {
                        operation: "leave".to_string(),
                        success: false,
                        call_id: None,
                        error: Some("Communitas not configured; use legacy signaling".to_string()),
                        message_id,
                    });
                }

                // Spawn async task to leave the call and send result
                let state = self.state.clone();
                let peer_id = self.peer_id.clone();
                let session_id = self.session_id.clone();
                tokio::spawn(async move {
                    let result = state
                        .leave_communitas_call_async(&session_id, &peer_id)
                        .await;
                    let message = match result {
                        Ok(()) => ServerMessage::CommunitasCallResult {
                            operation: "leave".to_string(),
                            success: true,
                            call_id: None,
                            error: None,
                            message_id,
                        },
                        Err(e) => ServerMessage::CommunitasCallResult {
                            operation: "leave".to_string(),
                            success: false,
                            call_id: None,
                            error: Some(e),
                            message_id,
                        },
                    };
                    state.send_to_peer(&peer_id, message);
                });
                None
            }

            ClientMessage::Interaction {
                interaction_type,
                element_id,
                data,
                message_id,
            } => {
                // Convert to AG-UI interaction event and broadcast
                use crate::agui::InteractionEvent;

                let interaction = match interaction_type.as_str() {
                    "touch" => {
                        let phase = data
                            .get("phase")
                            .and_then(|v| v.as_str())
                            .unwrap_or("start")
                            .to_string();
                        #[allow(clippy::cast_possible_truncation)]
                        let x = data.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                        #[allow(clippy::cast_possible_truncation)]
                        let y = data.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                        #[allow(clippy::cast_possible_truncation)]
                        let pointer_id =
                            data.get("pointer_id").and_then(|v| v.as_u64()).unwrap_or(0) as u32;

                        InteractionEvent::Touch {
                            element_id,
                            phase,
                            x,
                            y,
                            pointer_id,
                        }
                    }
                    "button_click" => {
                        let action = data
                            .get("action")
                            .and_then(|v| v.as_str())
                            .unwrap_or("click")
                            .to_string();

                        InteractionEvent::ButtonClick {
                            element_id: element_id.unwrap_or_default(),
                            action,
                        }
                    }
                    "form_input" => {
                        let field = data
                            .get("field")
                            .and_then(|v| v.as_str())
                            .unwrap_or("input")
                            .to_string();
                        let value = data
                            .get("value")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();

                        InteractionEvent::FormInput {
                            element_id: element_id.unwrap_or_default(),
                            field,
                            value,
                        }
                    }
                    "selection" => {
                        let selected = data
                            .get("selected")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(true);

                        InteractionEvent::Selection {
                            element_id: element_id.unwrap_or_default(),
                            selected,
                        }
                    }
                    "gesture" => {
                        let gesture_type = data
                            .get("gesture_type")
                            .and_then(|v| v.as_str())
                            .unwrap_or("pinch")
                            .to_string();
                        #[allow(clippy::cast_possible_truncation)]
                        let scale = data.get("scale").and_then(|v| v.as_f64()).map(|s| s as f32);
                        #[allow(clippy::cast_possible_truncation)]
                        let rotation = data
                            .get("rotation")
                            .and_then(|v| v.as_f64())
                            .map(|r| r as f32);
                        #[allow(clippy::cast_possible_truncation)]
                        let center_x =
                            data.get("center_x").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                        #[allow(clippy::cast_possible_truncation)]
                        let center_y =
                            data.get("center_y").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;

                        InteractionEvent::Gesture {
                            gesture_type,
                            scale,
                            rotation,
                            center_x,
                            center_y,
                        }
                    }
                    _ => {
                        return Some(ServerMessage::Error {
                            code: "INVALID_INTERACTION".to_string(),
                            message: format!("Unknown interaction type: {interaction_type}"),
                            message_id,
                        });
                    }
                };

                // Broadcast to AG-UI clients via the interaction channel
                self.state
                    .broadcast_interaction(&self.session_id, interaction);

                // Acknowledge if message_id was provided
                message_id.map(|id| ServerMessage::Ack {
                    message_id: id,
                    success: true,
                    result: None,
                })
            }
        }
    }

    /// Get the current session ID.
    #[allow(dead_code)]
    #[must_use]
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Try to receive a broadcast event for this client's session.
    #[allow(dead_code)]
    pub fn try_recv_event(&mut self) -> Option<ServerMessage> {
        match self.event_rx.try_recv() {
            Ok(event) if event.session_id == self.session_id => Some(event.message),
            _ => None,
        }
    }
}

/// Handle a WebSocket connection with full sync support.
pub async fn handle_sync_socket(socket: WebSocket, state: SyncState) {
    let (mut sender, mut receiver) = socket.split();

    // Generate peer ID and create client connection
    let peer_id = Uuid::new_v4().to_string();
    let mut client = ClientConnection::with_peer_id(state.clone(), peer_id.clone());

    // Create per-connection rate limiter
    let mut rate_limiter = RateLimiter::from_env();

    // Register peer with the default session initially
    let mut peer_rx = state.register_peer(&peer_id, client.session_id());

    // Send welcome message
    let welcome = ServerMessage::Welcome {
        version: env!("CARGO_PKG_VERSION").to_string(),
        session_id: client.session_id().to_string(),
        timestamp: current_timestamp(),
        legacy_signaling: Some(state.legacy_signaling_enabled()),
    };

    match serde_json::to_string(&welcome) {
        Ok(json) => {
            if sender.send(Message::Text(json.into())).await.is_err() {
                state.unregister_peer(&peer_id);
                return;
            }
        }
        Err(e) => {
            tracing::error!(peer_id = %peer_id, "Failed to serialize welcome message: {}", e);
            state.unregister_peer(&peer_id);
            return;
        }
    }

    // Send peer ID assignment
    let peer_assigned = ServerMessage::PeerAssigned {
        peer_id: peer_id.clone(),
    };

    match serde_json::to_string(&peer_assigned) {
        Ok(json) => {
            if sender.send(Message::Text(json.into())).await.is_err() {
                state.unregister_peer(&peer_id);
                return;
            }
        }
        Err(e) => {
            tracing::error!(peer_id = %peer_id, "Failed to serialize peer_assigned message: {}", e);
            state.unregister_peer(&peer_id);
            return;
        }
    }

    // Send initial scene state
    let scene_update = client.state.get_scene_update(client.session_id());
    match serde_json::to_string(&scene_update) {
        Ok(json) => {
            if sender.send(Message::Text(json.into())).await.is_err() {
                state.unregister_peer(&peer_id);
                return;
            }
        }
        Err(e) => {
            tracing::error!(peer_id = %peer_id, "Failed to serialize scene_update message: {}", e);
            state.unregister_peer(&peer_id);
            return;
        }
    }

    // Send initial call state snapshot
    let call_snapshot = state.call_snapshot(client.session_id());
    let call_message = ServerMessage::CallState {
        session_id: client.session_id().to_string(),
        call_id: call_snapshot.call_id,
        participants: call_snapshot.participants,
    };
    match serde_json::to_string(&call_message) {
        Ok(json) => {
            if sender.send(Message::Text(json.into())).await.is_err() {
                state.unregister_peer(&peer_id);
                return;
            }
        }
        Err(e) => {
            tracing::error!(peer_id = %peer_id, "Failed to serialize call_state message: {}", e);
            state.unregister_peer(&peer_id);
            return;
        }
    }

    // Subscribe to broadcast events
    let mut event_rx = state.subscribe();

    loop {
        tokio::select! {
            // Handle incoming WebSocket messages
            msg = receiver.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        // Check rate limit first
                        if !rate_limiter.try_consume() {
                            tracing::warn!("Rate limit exceeded for peer {}", peer_id);
                            record_rate_limited("websocket");
                            let retry_after = rate_limiter
                                .time_until_available()
                                .map_or(100, |d| d.as_millis().min(10000) as u64);
                            let error = ServerMessage::Error {
                                code: "rate_limited".to_string(),
                                message: format!("Rate limit exceeded. Retry after {}ms", retry_after),
                                message_id: None,
                            };
                            if let Ok(json) = serde_json::to_string(&error) {
                                if sender.send(Message::Text(json.into())).await.is_err() {
                                    break;
                                }
                            }
                            continue;
                        }

                        // Validate message size before processing
                        if let Err(e) = validate_message_size(text.len()) {
                            tracing::warn!("Message from peer {} rejected: {}", peer_id, e);
                            record_validation_failure("message_size");
                            let error = ServerMessage::Error {
                                code: "message_too_large".to_string(),
                                message: e.to_string(),
                                message_id: None,
                            };
                            if let Ok(json) = serde_json::to_string(&error) {
                                if sender.send(Message::Text(json.into())).await.is_err() {
                                    break;
                                }
                            }
                            continue;
                        }

                        tracing::debug!("Received from {}: {}", peer_id, text);

                        match serde_json::from_str::<ClientMessage>(&text) {
                            Ok(client_msg) => {
                                // Handle subscribe specially to update session and peer registry
                                if let ClientMessage::Subscribe { ref session_id } = client_msg {
                                    tracing::info!("Peer {} subscribed to session: {}", peer_id, session_id);
                                    state.update_peer_session(&peer_id, session_id);
                                }

                                if let Some(response) = client.handle_message(client_msg) {
                                    if let Ok(json) = serde_json::to_string(&response) {
                                        if sender.send(Message::Text(json.into())).await.is_err() {
                                            break;
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                let error = ServerMessage::Error {
                                    code: "parse_error".to_string(),
                                    message: e.to_string(),
                                    message_id: None,
                                };
                                if let Ok(json) = serde_json::to_string(&error) {
                                    if sender.send(Message::Text(json.into())).await.is_err() {
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) => {
                        tracing::info!("Peer {} disconnected", peer_id);
                        break;
                    }
                    Some(Err(e)) => {
                        tracing::error!("WebSocket error for peer {}: {}", peer_id, e);
                        break;
                    }
                    None => break,
                    _ => {}
                }
            }

            // Handle direct peer messages (signaling relay)
            peer_msg = peer_rx.recv() => {
                match peer_msg {
                    Some(message) => {
                        if let Ok(json) = serde_json::to_string(&message) {
                            if sender.send(Message::Text(json.into())).await.is_err() {
                                break;
                            }
                        }
                    }
                    None => {
                        // Channel closed, peer was unregistered
                        tracing::debug!("Peer {} channel closed", peer_id);
                        break;
                    }
                }
            }

            // Broadcast scene updates to client
            event = event_rx.recv() => {
                match event {
                    Ok(sync_event) if sync_event.session_id == client.session_id() => {
                        if let Ok(json) = serde_json::to_string(&sync_event.message) {
                            if sender.send(Message::Text(json.into())).await.is_err() {
                                break;
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("WebSocket client {} lagged behind by {} messages", peer_id, n);
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        tracing::info!("Broadcast channel closed");
                        break;
                    }
                    _ => {}
                }
            }
        }
    }

    // Cleanup: unregister peer
    state.unregister_peer(&peer_id);
    tracing::info!("WebSocket sync connection for peer {} closed", peer_id);
}

// Helper functions

fn default_session() -> String {
    "default".to_string()
}

/// Get the current Unix timestamp in milliseconds.
#[allow(clippy::cast_possible_truncation)]
#[must_use]
pub fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn parse_element_id(id: &str) -> Result<ElementId, SyncError> {
    Uuid::parse_str(id)
        .map(ElementId::from_uuid)
        .map_err(|_| SyncError::InvalidElementId(id.to_string()))
}

fn element_from_data(data: &ElementDocument) -> Result<Element, SyncError> {
    let mut doc = data.clone();
    if doc.id.is_empty() {
        doc.id = ElementId::new().to_string();
    }
    let id = doc.id.clone();
    doc.into_element()
        .map_err(|_| SyncError::InvalidElementId(id))
}

/// Validate a numeric value is finite and within f32 range.
///
/// Returns `Some(value as f32)` if the value is finite and within f32 range,
/// otherwise logs a warning and returns `None`.
fn validate_f64_for_f32(value: f64, field_name: &str) -> Option<f32> {
    if !value.is_finite() {
        tracing::warn!(
            field = %field_name,
            value = ?value,
            "apply_changes_to_element: ignoring non-finite value"
        );
        return None;
    }

    // Check if value is within f32 range (approximately ±3.4e38)
    const F32_MAX: f64 = f32::MAX as f64;
    if !(-F32_MAX..=F32_MAX).contains(&value) {
        tracing::warn!(
            field = %field_name,
            value = %value,
            "apply_changes_to_element: value out of f32 range, clamping"
        );
        // Clamp to f32 range instead of rejecting
        return Some(value.clamp(-F32_MAX, F32_MAX) as f32);
    }

    Some(value as f32)
}

/// Apply JSON changes to an element.
///
/// Supported fields in the changes JSON:
/// - `transform.x`, `transform.y`: Position (f32)
/// - `transform.width`, `transform.height`: Dimensions (f32)
/// - `transform.rotation`: Rotation in radians (f32)
/// - `transform.z_index`: Layer ordering (i32)
/// - `interactive`: Whether element responds to input (bool)
///
/// Unknown fields are logged at debug level and silently ignored for forward
/// compatibility (newer clients may send fields older servers don't understand).
/// Invalid values (NaN, Infinity, out-of-range) are logged and ignored.
fn apply_changes_to_element(element: &mut Element, changes: &serde_json::Value) {
    // Known top-level fields
    const KNOWN_TOP_LEVEL: &[&str] = &["transform", "interactive"];
    // Known transform fields
    const KNOWN_TRANSFORM: &[&str] = &["x", "y", "width", "height", "rotation", "z_index"];

    // Log unknown top-level fields at debug level
    if let Some(obj) = changes.as_object() {
        for key in obj.keys() {
            if !KNOWN_TOP_LEVEL.contains(&key.as_str()) {
                tracing::debug!(
                    field = %key,
                    "apply_changes_to_element: ignoring unknown top-level field"
                );
            }
        }
    }

    if let Some(transform) = changes.get("transform") {
        // Log unknown transform fields
        if let Some(obj) = transform.as_object() {
            for key in obj.keys() {
                if !KNOWN_TRANSFORM.contains(&key.as_str()) {
                    tracing::debug!(
                        field = %key,
                        "apply_changes_to_element: ignoring unknown transform field"
                    );
                }
            }
        }

        if let Some(x) = transform.get("x").and_then(|v| v.as_f64()) {
            if let Some(validated) = validate_f64_for_f32(x, "transform.x") {
                element.transform.x = validated;
            }
        }
        if let Some(y) = transform.get("y").and_then(|v| v.as_f64()) {
            if let Some(validated) = validate_f64_for_f32(y, "transform.y") {
                element.transform.y = validated;
            }
        }
        if let Some(width) = transform.get("width").and_then(|v| v.as_f64()) {
            if let Some(validated) = validate_f64_for_f32(width, "transform.width") {
                element.transform.width = validated;
            }
        }
        if let Some(height) = transform.get("height").and_then(|v| v.as_f64()) {
            if let Some(validated) = validate_f64_for_f32(height, "transform.height") {
                element.transform.height = validated;
            }
        }
        if let Some(rotation) = transform.get("rotation").and_then(|v| v.as_f64()) {
            if let Some(validated) = validate_f64_for_f32(rotation, "transform.rotation") {
                element.transform.rotation = validated;
            }
        }
        if let Some(z_index) = transform.get("z_index").and_then(|v| v.as_i64()) {
            // i64 to i32 conversion - clamp to i32 range
            #[allow(clippy::cast_possible_truncation)]
            let clamped = z_index.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
            if clamped != z_index as i32 {
                tracing::warn!(
                    original = %z_index,
                    clamped = %clamped,
                    "apply_changes_to_element: z_index out of i32 range, clamped"
                );
            }
            element.transform.z_index = clamped;
        }
    }

    if let Some(interactive) = changes.get("interactive").and_then(|v| v.as_bool()) {
        element.interactive = interactive;
    }
}

/// Convert an Element to serializable ElementDocument.
#[must_use]
pub fn element_to_data(element: &Element) -> ElementDocument {
    ElementDocument::from(element)
}

#[cfg(test)]
mod tests {
    use super::*;
    use canvas_core::{ElementKind, Transform, ViewportDocument};

    #[test]
    fn test_client_message_parse_subscribe() {
        let json = r#"{"type":"subscribe","session_id":"test-session"}"#;
        let msg: ClientMessage = serde_json::from_str(json).expect("should parse");
        match msg {
            ClientMessage::Subscribe { session_id } => assert_eq!(session_id, "test-session"),
            _ => panic!("Expected Subscribe"),
        }
    }

    #[test]
    fn test_client_message_parse_subscribe_default() {
        let json = r#"{"type":"subscribe"}"#;
        let msg: ClientMessage = serde_json::from_str(json).expect("should parse");
        match msg {
            ClientMessage::Subscribe { session_id } => assert_eq!(session_id, "default"),
            _ => panic!("Expected Subscribe"),
        }
    }

    #[test]
    fn test_client_message_parse_add_element() {
        let json = r##"{"type":"add_element","element":{"id":"","kind":{"type":"Text","data":{"content":"Hello","font_size":24.0,"color":"#ff0000"}},"transform":{"x":100,"y":200,"width":300,"height":50,"rotation":0,"z_index":0}},"message_id":"msg-123"}"##;
        let msg: ClientMessage = serde_json::from_str(json).expect("should parse");
        match msg {
            ClientMessage::AddElement {
                element,
                message_id,
            } => {
                assert_eq!(message_id, Some("msg-123".to_string()));
                match element.kind {
                    ElementKind::Text { content, .. } => assert_eq!(content, "Hello"),
                    _ => panic!("Expected Text element"),
                }
            }
            _ => panic!("Expected AddElement"),
        }
    }

    #[test]
    fn test_client_message_parse_remove_element() {
        let json = r#"{"type":"remove_element","id":"some-uuid"}"#;
        let msg: ClientMessage = serde_json::from_str(json).expect("should parse");
        match msg {
            ClientMessage::RemoveElement { id, .. } => assert_eq!(id, "some-uuid"),
            _ => panic!("Expected RemoveElement"),
        }
    }

    #[test]
    fn test_client_message_parse_ping() {
        let json = r#"{"type":"ping"}"#;
        let msg: ClientMessage = serde_json::from_str(json).expect("should parse");
        assert!(matches!(msg, ClientMessage::Ping));
    }

    #[test]
    fn test_server_message_serialize_welcome() {
        let msg = ServerMessage::Welcome {
            version: "1.0.0".to_string(),
            session_id: "default".to_string(),
            timestamp: 12345,
            legacy_signaling: Some(true),
        };
        let json = serde_json::to_string(&msg).expect("should serialize");
        assert!(json.contains("welcome"));
        assert!(json.contains("1.0.0"));
        assert!(json.contains("legacy_signaling"));
    }

    #[test]
    fn test_server_message_serialize_scene_update() {
        let msg = ServerMessage::SceneUpdate {
            scene: SceneDocument {
                session_id: "default".to_string(),
                viewport: ViewportDocument {
                    width: 800.0,
                    height: 600.0,
                    zoom: 1.0,
                    pan_x: 0.0,
                    pan_y: 0.0,
                },
                elements: vec![ElementDocument {
                    id: "elem-1".to_string(),
                    kind: ElementKind::Text {
                        content: "Test".to_string(),
                        font_size: 16.0,
                        color: "#000000".to_string(),
                    },
                    transform: Transform::default(),
                    interactive: true,
                    selected: false,
                }],
                timestamp: 12345,
            },
        };
        let json = serde_json::to_string(&msg).expect("should serialize");
        assert!(json.contains("scene_update"));
        assert!(json.contains("elem-1"));
        assert!(json.contains("Test"));
    }

    #[test]
    fn test_server_message_serialize_element_added() {
        let msg = ServerMessage::ElementAdded {
            element: ElementDocument {
                id: "new-elem".to_string(),
                kind: ElementKind::Text {
                    content: "New".to_string(),
                    font_size: 16.0,
                    color: "#000000".to_string(),
                },
                transform: Transform::default(),
                interactive: true,
                selected: false,
            },
            timestamp: 12345,
        };
        let json = serde_json::to_string(&msg).expect("should serialize");
        assert!(json.contains("element_added"));
        assert!(json.contains("new-elem"));
    }

    #[test]
    fn test_server_message_serialize_element_removed() {
        let msg = ServerMessage::ElementRemoved {
            id: "removed-elem".to_string(),
            timestamp: 12345,
        };
        let json = serde_json::to_string(&msg).expect("should serialize");
        assert!(json.contains("element_removed"));
        assert!(json.contains("removed-elem"));
    }

    #[test]
    fn test_server_message_serialize_error() {
        let msg = ServerMessage::Error {
            code: "not_found".to_string(),
            message: "Element not found".to_string(),
            message_id: Some("msg-123".to_string()),
        };
        let json = serde_json::to_string(&msg).expect("should serialize");
        assert!(json.contains("error"));
        assert!(json.contains("not_found"));
    }

    #[test]
    fn test_sync_state_create() {
        let state = SyncState::new();
        let scene = state.get_scene("default");
        assert!(scene.is_some());
    }

    #[test]
    fn test_sync_state_store_accessor() {
        let state = SyncState::new();
        let store = state.store();
        // The store should have the default session
        assert!(store.get("default").is_some());
    }

    #[test]
    fn test_sync_state_add_element() {
        let state = SyncState::new();
        let element = ElementDocument {
            id: String::new(),
            kind: ElementKind::Text {
                content: "Hello".to_string(),
                font_size: 16.0,
                color: "#000000".to_string(),
            },
            transform: Transform::default(),
            interactive: true,
            selected: false,
        };

        let result = state.add_element("default", &element);
        assert!(result.is_ok());

        let scene = state.get_scene("default").expect("should have scene");
        assert_eq!(scene.element_count(), 1);
    }

    #[test]
    fn test_sync_state_remove_element() {
        let state = SyncState::new();
        let element = ElementDocument {
            id: String::new(),
            kind: ElementKind::Text {
                content: "Hello".to_string(),
                font_size: 16.0,
                color: "#000000".to_string(),
            },
            transform: Transform::default(),
            interactive: true,
            selected: false,
        };

        let id = state.add_element("default", &element).expect("should add");
        let result = state.remove_element("default", &id.to_string());
        assert!(result.is_ok());

        let scene = state.get_scene("default").expect("should have scene");
        assert_eq!(scene.element_count(), 0);
    }

    #[test]
    fn test_sync_state_update_element() {
        let state = SyncState::new();
        let element = ElementDocument {
            id: String::new(),
            kind: ElementKind::Text {
                content: "Hello".to_string(),
                font_size: 16.0,
                color: "#000000".to_string(),
            },
            transform: Transform {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 100.0,
                rotation: 0.0,
                z_index: 0,
            },
            interactive: true,
            selected: false,
        };

        let id = state.add_element("default", &element).expect("should add");
        let changes = serde_json::json!({"transform": {"x": 50.0, "y": 75.0}});
        let result = state.update_element("default", &id.to_string(), &changes);
        assert!(result.is_ok());

        let scene = state.get_scene("default").expect("should have scene");
        let updated = scene.get_element(id).expect("should have element");
        assert!((updated.transform.x - 50.0).abs() < f32::EPSILON);
        assert!((updated.transform.y - 75.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_sync_state_get_scene_update() {
        let state = SyncState::new();
        let element = ElementDocument {
            id: String::new(),
            kind: ElementKind::Text {
                content: "Hello".to_string(),
                font_size: 16.0,
                color: "#000000".to_string(),
            },
            transform: Transform::default(),
            interactive: true,
            selected: false,
        };

        let _ = state.add_element("default", &element);
        let update = state.get_scene_update("default");

        match update {
            ServerMessage::SceneUpdate { scene } => {
                assert_eq!(scene.elements.len(), 1);
            }
            _ => panic!("Expected SceneUpdate"),
        }
    }

    #[test]
    fn test_sync_state_process_queue() {
        let state = SyncState::new();
        let operations = vec![
            QueuedOperation::Add {
                element: ElementDocument {
                    id: String::new(),
                    kind: ElementKind::Text {
                        content: "Queued 1".to_string(),
                        font_size: 16.0,
                        color: "#000000".to_string(),
                    },
                    transform: Transform::default(),
                    interactive: true,
                    selected: false,
                },
                timestamp: 100,
            },
            QueuedOperation::Add {
                element: ElementDocument {
                    id: String::new(),
                    kind: ElementKind::Text {
                        content: "Queued 2".to_string(),
                        font_size: 16.0,
                        color: "#000000".to_string(),
                    },
                    transform: Transform::default(),
                    interactive: true,
                    selected: false,
                },
                timestamp: 200,
            },
        ];

        let result = state.process_queue("default", operations);
        assert_eq!(result.processed_count, 2);
        assert_eq!(result.failed_count, 0);

        let scene = state.get_scene("default").expect("should have scene");
        assert_eq!(scene.element_count(), 2);
    }

    #[test]
    fn test_sync_state_multiple_sessions() {
        let state = SyncState::new();

        let element1 = ElementDocument {
            id: String::new(),
            kind: ElementKind::Text {
                content: "Session 1".to_string(),
                font_size: 16.0,
                color: "#000000".to_string(),
            },
            transform: Transform::default(),
            interactive: true,
            selected: false,
        };

        let element2 = ElementDocument {
            id: String::new(),
            kind: ElementKind::Text {
                content: "Session 2".to_string(),
                font_size: 16.0,
                color: "#000000".to_string(),
            },
            transform: Transform::default(),
            interactive: true,
            selected: false,
        };

        let _ = state.add_element("session-1", &element1);
        let _ = state.add_element("session-2", &element2);

        let scene1 = state.get_scene("session-1").expect("should have scene 1");
        let scene2 = state.get_scene("session-2").expect("should have scene 2");

        assert_eq!(scene1.element_count(), 1);
        assert_eq!(scene2.element_count(), 1);
    }

    #[test]
    fn test_client_connection_handle_ping() {
        let state = SyncState::new();
        let mut client = ClientConnection::new(state);

        let response = client.handle_message(ClientMessage::Ping);
        assert!(response.is_some());
        assert!(matches!(response.unwrap(), ServerMessage::Pong { .. }));
    }

    #[test]
    fn test_client_connection_handle_subscribe() {
        let state = SyncState::new();
        let mut client = ClientConnection::new(state);

        let response = client.handle_message(ClientMessage::Subscribe {
            session_id: "test-session".to_string(),
        });

        assert!(response.is_some());
        assert!(matches!(
            response.unwrap(),
            ServerMessage::SceneUpdate { .. }
        ));
        assert_eq!(client.session_id(), "test-session");
    }

    #[test]
    fn test_client_connection_handle_get_scene() {
        let state = SyncState::new();
        let mut client = ClientConnection::new(state);

        let response = client.handle_message(ClientMessage::GetScene);
        assert!(response.is_some());
        assert!(matches!(
            response.unwrap(),
            ServerMessage::SceneUpdate { .. }
        ));
    }

    #[test]
    fn test_element_data_default_id() {
        let json = r##"{"id":"","kind":{"type":"Text","data":{"content":"Test","font_size":16.0,"color":"#000000"}}}"##;
        let element_doc: ElementDocument = serde_json::from_str(json).expect("should parse");
        let element = element_from_data(&element_doc).expect("should convert");
        assert!(!element.id.to_string().is_empty());
    }

    #[test]
    fn test_element_kind_data_text_defaults() {
        let json =
            r##"{"type":"Text","data":{"content":"Test","font_size":16.0,"color":"#000000"}}"##;
        let kind: ElementKind = serde_json::from_str(json).expect("should parse");
        match kind {
            ElementKind::Text {
                font_size, color, ..
            } => {
                assert!((font_size - 16.0).abs() < f32::EPSILON);
                assert_eq!(color, "#000000");
            }
            _ => panic!("Expected Text"),
        }
    }

    #[test]
    fn test_transform_data_defaults() {
        let json = r##"{"id":"","kind":{"type":"Text","data":{"content":"Example","font_size":16.0,"color":"#000000"}}}"##;
        let doc: ElementDocument = serde_json::from_str(json).expect("should parse");
        assert!((doc.transform.width - 100.0).abs() < f32::EPSILON);
        assert!((doc.transform.height - 100.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_queued_operation_add() {
        let json = r##"{"op":"add","element":{"id":"","kind":{"type":"Text","data":{"content":"Queued","font_size":16.0,"color":"#000000"}}},"timestamp":12345}"##;
        let op: QueuedOperation = serde_json::from_str(json).expect("should parse");
        match op {
            QueuedOperation::Add { timestamp, .. } => assert_eq!(timestamp, 12345),
            _ => panic!("Expected Add"),
        }
    }

    #[test]
    fn test_queued_operation_remove() {
        let json = r#"{"op":"remove","id":"some-id","timestamp":54321}"#;
        let op: QueuedOperation = serde_json::from_str(json).expect("should parse");
        match op {
            QueuedOperation::Remove { id, timestamp } => {
                assert_eq!(id, "some-id");
                assert_eq!(timestamp, 54321);
            }
            _ => panic!("Expected Remove"),
        }
    }

    #[test]
    fn test_broadcast_event_subscription() {
        let state = SyncState::new();
        let mut rx = state.subscribe();

        let element = ElementDocument {
            id: String::new(),
            kind: ElementKind::Text {
                content: "Broadcast Test".to_string(),
                font_size: 16.0,
                color: "#000000".to_string(),
            },
            transform: Transform::default(),
            interactive: true,
            selected: false,
        };

        // This should trigger a broadcast
        let _ = state.add_element("default", &element);

        // Should receive the broadcast
        let event = rx.try_recv();
        assert!(event.is_ok());
        let sync_event = event.unwrap();
        assert_eq!(sync_event.session_id, "default");
        assert!(
            matches!(sync_event.message, ServerMessage::ElementAdded { .. })
                || matches!(sync_event.message, ServerMessage::SceneUpdate { .. })
        );
    }

    #[test]
    fn test_sync_error_display() {
        let err = SyncError::ElementNotFound("test-id".to_string());
        assert!(err.to_string().contains("test-id"));

        let err = SyncError::SessionNotFound("test-session".to_string());
        assert!(err.to_string().contains("test-session"));

        let err = SyncError::InvalidElementId("bad-id".to_string());
        assert!(err.to_string().contains("bad-id"));
    }

    #[test]
    fn test_store_error_conversion() {
        let store_err = StoreError::SessionNotFound("test".to_string());
        let sync_err: SyncError = store_err.into();
        assert!(matches!(sync_err, SyncError::SessionNotFound(_)));

        let store_err = StoreError::ElementNotFound("elem".to_string());
        let sync_err: SyncError = store_err.into();
        assert!(matches!(sync_err, SyncError::ElementNotFound(_)));

        let store_err = StoreError::LockPoisoned;
        let sync_err: SyncError = store_err.into();
        assert!(matches!(sync_err, SyncError::LockPoisoned));

        let store_err = StoreError::SceneError("scene error".to_string());
        let sync_err: SyncError = store_err.into();
        assert!(matches!(sync_err, SyncError::InvalidMessage(_)));
    }

    // === WebRTC Signaling Message Tests ===

    #[test]
    fn test_client_message_parse_start_call() {
        let json =
            r#"{"type":"start_call","target_peer_id":"peer-123","session_id":"test-session"}"#;
        let msg: ClientMessage = serde_json::from_str(json).expect("should parse");
        match msg {
            ClientMessage::StartCall {
                target_peer_id,
                session_id,
            } => {
                assert_eq!(target_peer_id, "peer-123");
                assert_eq!(session_id, "test-session");
            }
            _ => panic!("Expected StartCall"),
        }
    }

    #[test]
    fn test_client_message_parse_offer() {
        let json = r#"{"type":"offer","target_peer_id":"peer-456","sdp":"v=0\r\no=- 123 456 IN IP4 127.0.0.1"}"#;
        let msg: ClientMessage = serde_json::from_str(json).expect("should parse");
        match msg {
            ClientMessage::Offer {
                target_peer_id,
                sdp,
            } => {
                assert_eq!(target_peer_id, "peer-456");
                assert!(sdp.contains("v=0"));
            }
            _ => panic!("Expected Offer"),
        }
    }

    #[test]
    fn test_client_message_parse_answer() {
        let json = r#"{"type":"answer","target_peer_id":"peer-789","sdp":"v=0\r\no=- 789 012 IN IP4 127.0.0.1"}"#;
        let msg: ClientMessage = serde_json::from_str(json).expect("should parse");
        match msg {
            ClientMessage::Answer {
                target_peer_id,
                sdp,
            } => {
                assert_eq!(target_peer_id, "peer-789");
                assert!(sdp.contains("v=0"));
            }
            _ => panic!("Expected Answer"),
        }
    }

    #[test]
    fn test_client_message_parse_ice_candidate() {
        let json = r#"{"type":"ice_candidate","target_peer_id":"peer-abc","candidate":"candidate:1 1 UDP 2130706431 192.168.1.1 12345 typ host","sdp_mid":"0","sdp_m_line_index":0}"#;
        let msg: ClientMessage = serde_json::from_str(json).expect("should parse");
        match msg {
            ClientMessage::IceCandidate {
                target_peer_id,
                candidate,
                sdp_mid,
                sdp_m_line_index,
            } => {
                assert_eq!(target_peer_id, "peer-abc");
                assert!(candidate.contains("candidate:1"));
                assert_eq!(sdp_mid, Some("0".to_string()));
                assert_eq!(sdp_m_line_index, Some(0));
            }
            _ => panic!("Expected IceCandidate"),
        }
    }

    #[test]
    fn test_client_message_parse_ice_candidate_minimal() {
        let json = r#"{"type":"ice_candidate","target_peer_id":"peer-xyz","candidate":"candidate:2 1 UDP"}"#;
        let msg: ClientMessage = serde_json::from_str(json).expect("should parse");
        match msg {
            ClientMessage::IceCandidate {
                sdp_mid,
                sdp_m_line_index,
                ..
            } => {
                assert!(sdp_mid.is_none());
                assert!(sdp_m_line_index.is_none());
            }
            _ => panic!("Expected IceCandidate"),
        }
    }

    #[test]
    fn test_client_message_parse_end_call() {
        let json = r#"{"type":"end_call","target_peer_id":"peer-end"}"#;
        let msg: ClientMessage = serde_json::from_str(json).expect("should parse");
        match msg {
            ClientMessage::EndCall { target_peer_id } => {
                assert_eq!(target_peer_id, "peer-end");
            }
            _ => panic!("Expected EndCall"),
        }
    }

    #[test]
    fn test_server_message_serialize_incoming_call() {
        let msg = ServerMessage::IncomingCall {
            from_peer_id: "caller-123".to_string(),
            session_id: "test-session".to_string(),
        };
        let json = serde_json::to_string(&msg).expect("should serialize");
        assert!(json.contains("incoming_call"));
        assert!(json.contains("caller-123"));
        assert!(json.contains("test-session"));
    }

    #[test]
    fn test_server_message_serialize_relay_offer() {
        let msg = ServerMessage::RelayOffer {
            from_peer_id: "sender-456".to_string(),
            sdp: "v=0\r\no=- test".to_string(),
        };
        let json = serde_json::to_string(&msg).expect("should serialize");
        assert!(json.contains("relay_offer"));
        assert!(json.contains("sender-456"));
        assert!(json.contains("v=0"));
    }

    #[test]
    fn test_server_message_serialize_relay_answer() {
        let msg = ServerMessage::RelayAnswer {
            from_peer_id: "responder-789".to_string(),
            sdp: "v=0\r\no=- answer".to_string(),
        };
        let json = serde_json::to_string(&msg).expect("should serialize");
        assert!(json.contains("relay_answer"));
        assert!(json.contains("responder-789"));
    }

    #[test]
    fn test_server_message_serialize_relay_ice_candidate() {
        let msg = ServerMessage::RelayIceCandidate {
            from_peer_id: "ice-sender".to_string(),
            candidate: "candidate:1 1 UDP 2130706431".to_string(),
            sdp_mid: Some("audio".to_string()),
            sdp_m_line_index: Some(1),
        };
        let json = serde_json::to_string(&msg).expect("should serialize");
        assert!(json.contains("relay_ice_candidate"));
        assert!(json.contains("ice-sender"));
        assert!(json.contains("candidate:1"));
        assert!(json.contains("audio"));
    }

    #[test]
    fn test_server_message_serialize_relay_ice_candidate_minimal() {
        let msg = ServerMessage::RelayIceCandidate {
            from_peer_id: "ice-min".to_string(),
            candidate: "candidate:2".to_string(),
            sdp_mid: None,
            sdp_m_line_index: None,
        };
        let json = serde_json::to_string(&msg).expect("should serialize");
        // Should not contain sdp_mid or sdp_m_line_index when None
        assert!(!json.contains("sdp_mid"));
        assert!(!json.contains("sdp_m_line_index"));
    }

    #[test]
    fn test_server_message_serialize_call_state() {
        let msg = ServerMessage::CallState {
            session_id: "default".to_string(),
            call_id: Some("call-xyz".to_string()),
            participants: vec!["peer-a".to_string(), "peer-b".to_string()],
        };
        let json = serde_json::to_string(&msg).expect("should serialize");
        assert!(json.contains("call_state"));
        assert!(json.contains("call-xyz"));
        assert!(json.contains("peer-a"));
    }

    #[test]
    fn test_call_snapshot_tracks_participants() {
        let state = SyncState::new();
        state.add_call_participant("default", "peer-a");
        state.add_call_participant("default", "peer-b");
        state.set_call_metadata("default", "call-123".to_string(), "default".to_string());

        let snapshot = state.call_snapshot("default");
        assert_eq!(snapshot.call_id, Some("call-123".to_string()));
        assert_eq!(snapshot.participants.len(), 2);
        assert!(snapshot.participants.contains(&"peer-a".to_string()));
        assert!(snapshot.participants.contains(&"peer-b".to_string()));
    }

    #[test]
    fn test_server_message_serialize_call_ended() {
        let msg = ServerMessage::CallEnded {
            from_peer_id: "ender-123".to_string(),
            reason: "user_hangup".to_string(),
        };
        let json = serde_json::to_string(&msg).expect("should serialize");
        assert!(json.contains("call_ended"));
        assert!(json.contains("ender-123"));
        assert!(json.contains("user_hangup"));
    }

    #[test]
    fn test_server_message_serialize_peer_assigned() {
        let msg = ServerMessage::PeerAssigned {
            peer_id: "assigned-peer-id".to_string(),
        };
        let json = serde_json::to_string(&msg).expect("should serialize");
        assert!(json.contains("peer_assigned"));
        assert!(json.contains("assigned-peer-id"));
    }

    #[test]
    fn test_client_connection_handle_signaling_start_call_no_peer() {
        let state = SyncState::new();
        let mut client = ClientConnection::new(state);

        // StartCall returns error when target peer not in same session
        let response = client.handle_message(ClientMessage::StartCall {
            target_peer_id: "peer-1".to_string(),
            session_id: "default".to_string(),
        });
        assert!(response.is_some());
        match response.unwrap() {
            ServerMessage::Error { code, .. } => {
                assert_eq!(code, "peer_not_found");
            }
            _ => panic!("Expected Error response"),
        }
    }

    #[test]
    fn test_client_connection_handle_signaling_relay_returns_none() {
        let state = SyncState::new();
        let mut client = ClientConnection::new(state);

        // Other signaling messages return None (relay silently fails for missing peers)
        let response = client.handle_message(ClientMessage::Offer {
            target_peer_id: "peer-2".to_string(),
            sdp: "test".to_string(),
        });
        assert!(response.is_none());

        let response = client.handle_message(ClientMessage::EndCall {
            target_peer_id: "peer-3".to_string(),
        });
        assert!(response.is_none());
    }

    // === Rate Limiter Tests ===

    #[test]
    fn test_rate_limiter_allows_burst() {
        let mut limiter = RateLimiter::new(10, 1);
        // Should allow burst up to capacity
        for _ in 0..10 {
            assert!(limiter.try_consume());
        }
        // 11th should be rejected
        assert!(!limiter.try_consume());
    }

    #[test]
    fn test_rate_limiter_refills_over_time() {
        let mut limiter = RateLimiter::new(2, 10);
        // Consume all tokens
        assert!(limiter.try_consume());
        assert!(limiter.try_consume());
        assert!(!limiter.try_consume());

        // Simulate time passing (manually set last_refill)
        limiter.last_refill = Instant::now() - Duration::from_millis(200);
        limiter.refill();
        // Should have ~2 tokens now (200ms * 10/s = 2)
        assert!(limiter.tokens >= 1.0);
    }

    #[test]
    fn test_rate_limiter_time_until_available() {
        let mut limiter = RateLimiter::new(1, 10);
        assert!(limiter.try_consume());
        // No tokens left
        let wait_time = limiter.time_until_available();
        assert!(wait_time.is_some());
        // Should need ~100ms for 1 token at 10/s rate
        let wait_ms = wait_time.unwrap().as_millis();
        assert!(wait_ms > 0 && wait_ms <= 100);
    }

    #[test]
    fn test_rate_limiter_from_env_defaults() {
        // Without env vars, should use defaults
        let limiter = RateLimiter::from_env();
        assert!((limiter.capacity - 100.0).abs() < f64::EPSILON);
        assert!((limiter.refill_rate - 10.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_rate_limiter_capacity_capped() {
        let mut limiter = RateLimiter::new(5, 10);
        // Tokens should never exceed capacity
        limiter.last_refill = Instant::now() - Duration::from_secs(100);
        limiter.refill();
        assert!((limiter.tokens - 5.0).abs() < f64::EPSILON);
    }

    // Tests for OperationType, FailedOperationInfo, and ProcessQueueResult

    #[test]
    fn test_operation_type_display() {
        assert_eq!(OperationType::Add.to_string(), "add");
        assert_eq!(OperationType::Update.to_string(), "update");
        assert_eq!(OperationType::Remove.to_string(), "remove");
    }

    #[test]
    fn test_operation_type_serialization() {
        let json = serde_json::to_string(&OperationType::Add).expect("should serialize");
        assert_eq!(json, "\"add\"");

        let json = serde_json::to_string(&OperationType::Update).expect("should serialize");
        assert_eq!(json, "\"update\"");

        let json = serde_json::to_string(&OperationType::Remove).expect("should serialize");
        assert_eq!(json, "\"remove\"");
    }

    #[test]
    fn test_queued_operation_type() {
        let add_op = QueuedOperation::Add {
            element: ElementDocument {
                id: "test".to_string(),
                kind: ElementKind::Text {
                    content: "test".to_string(),
                    font_size: 16.0,
                    color: "#000".to_string(),
                },
                transform: Transform::default(),
                interactive: true,
                selected: false,
            },
            timestamp: 100,
        };
        assert_eq!(add_op.operation_type(), OperationType::Add);

        let update_op = QueuedOperation::Update {
            id: "test".to_string(),
            changes: serde_json::json!({}),
            timestamp: 100,
        };
        assert_eq!(update_op.operation_type(), OperationType::Update);

        let remove_op = QueuedOperation::Remove {
            id: "test".to_string(),
            timestamp: 100,
        };
        assert_eq!(remove_op.operation_type(), OperationType::Remove);
    }

    #[test]
    fn test_failed_operation_info_from_add_op() {
        let op = QueuedOperation::Add {
            element: ElementDocument {
                id: "elem-123".to_string(),
                kind: ElementKind::Text {
                    content: "test".to_string(),
                    font_size: 16.0,
                    color: "#000".to_string(),
                },
                transform: Transform::default(),
                interactive: true,
                selected: false,
            },
            timestamp: 100,
        };

        let info = FailedOperationInfo::from_failed_op(&op, "element already exists");
        assert_eq!(info.operation, OperationType::Add);
        assert_eq!(info.element_id, Some("elem-123".to_string()));
        assert_eq!(info.error, "element already exists");
    }

    #[test]
    fn test_failed_operation_info_from_remove_op() {
        let op = QueuedOperation::Remove {
            id: "elem-456".to_string(),
            timestamp: 200,
        };

        let info = FailedOperationInfo::from_failed_op(&op, "element not found");
        assert_eq!(info.operation, OperationType::Remove);
        assert_eq!(info.element_id, Some("elem-456".to_string()));
        assert_eq!(info.error, "element not found");
    }

    #[test]
    fn test_failed_operation_info_serialization() {
        let info = FailedOperationInfo {
            operation: OperationType::Update,
            element_id: Some("id-789".to_string()),
            error: "conflict detected".to_string(),
        };

        let json = serde_json::to_string(&info).expect("should serialize");
        assert!(json.contains("\"operation\":\"update\""));
        assert!(json.contains("\"element_id\":\"id-789\""));
        assert!(json.contains("\"error\":\"conflict detected\""));
    }

    #[test]
    fn test_failed_operation_info_skips_none_element_id() {
        let info = FailedOperationInfo {
            operation: OperationType::Add,
            element_id: None,
            error: "unknown error".to_string(),
        };

        let json = serde_json::to_string(&info).expect("should serialize");
        assert!(!json.contains("element_id"));
    }

    #[test]
    fn test_process_queue_result_into_server_message() {
        let op1 = QueuedOperation::Remove {
            id: "e1".to_string(),
            timestamp: 1,
        };
        let op2 = QueuedOperation::Update {
            id: "e2".to_string(),
            changes: serde_json::json!({}),
            timestamp: 2,
        };

        let result = ProcessQueueResult {
            processed_count: 5,
            failed_count: 2,
            failed_ops: vec![
                (op1, "not found".to_string()),
                (op2, "conflict".to_string()),
            ],
            timestamp: 12345,
        };

        let msg = result.into_server_message();
        match msg {
            ServerMessage::SyncResult {
                synced_count,
                conflict_count,
                timestamp,
                failed_operations,
            } => {
                assert_eq!(synced_count, 5);
                assert_eq!(conflict_count, 2);
                assert_eq!(timestamp, 12345);
                assert_eq!(failed_operations.len(), 2);
                assert_eq!(failed_operations[0].operation, OperationType::Remove);
                assert_eq!(failed_operations[1].operation, OperationType::Update);
            }
            _ => panic!("Expected SyncResult"),
        }
    }

    #[test]
    fn test_process_queue_result_truncates_to_max_failures() {
        let failed_ops: Vec<_> = (0..15)
            .map(|i| {
                (
                    QueuedOperation::Remove {
                        id: format!("e{}", i),
                        timestamp: i,
                    },
                    "error".to_string(),
                )
            })
            .collect();

        let result = ProcessQueueResult {
            processed_count: 0,
            failed_count: 15,
            failed_ops,
            timestamp: 12345,
        };

        let msg = result.into_server_message();
        match msg {
            ServerMessage::SyncResult {
                failed_operations, ..
            } => {
                assert_eq!(
                    failed_operations.len(),
                    FailedOperationInfo::MAX_FAILURES_IN_RESPONSE
                );
                assert_eq!(failed_operations.len(), 10);
            }
            _ => panic!("Expected SyncResult"),
        }
    }

    #[test]
    fn test_process_queue_with_failed_remove() {
        let state = SyncState::new();

        // Try to remove a non-existent element
        let operations = vec![QueuedOperation::Remove {
            id: "nonexistent-id".to_string(),
            timestamp: 100,
        }];

        let result = state.process_queue("default", operations);
        // The remove should fail since element doesn't exist
        assert_eq!(result.processed_count, 0);
        assert_eq!(result.failed_count, 1);
        assert_eq!(result.failed_ops.len(), 1);
        assert_eq!(
            result.failed_ops[0].0.operation_type(),
            OperationType::Remove
        );
    }

    #[test]
    fn test_sync_processor_new() {
        let store = Arc::new(SceneStore::new());
        let processor = SyncProcessor::new(store, ConflictStrategy::LastWriteWins);
        assert_eq!(
            processor.conflict_strategy(),
            ConflictStrategy::LastWriteWins
        );
    }

    #[test]
    fn test_sync_processor_set_conflict_strategy() {
        let store = Arc::new(SceneStore::new());
        let mut processor = SyncProcessor::new(store, ConflictStrategy::LastWriteWins);
        processor.set_conflict_strategy(ConflictStrategy::LocalWins);
        assert_eq!(processor.conflict_strategy(), ConflictStrategy::LocalWins);
    }

    #[test]
    fn test_sync_processor_process_empty_batch() {
        let store = Arc::new(SceneStore::new());
        let processor = SyncProcessor::new(store, ConflictStrategy::LastWriteWins);
        let result = processor.process_batch("default", vec![]);
        assert_eq!(result.synced_count, 0);
        assert_eq!(result.failed_count, 0);
        assert!(result.success);
    }

    #[test]
    fn test_sync_processor_process_add_element() {
        let store = Arc::new(SceneStore::new());
        let processor = SyncProcessor::new(store.clone(), ConflictStrategy::LastWriteWins);

        let element = Element::new(ElementKind::Text {
            content: "Test".to_string(),
            font_size: 16.0,
            color: "#000000".to_string(),
        });
        let operations = vec![Operation::AddElement {
            element,
            timestamp: Operation::now(),
        }];

        let result = processor.process_batch("default", operations);
        assert_eq!(result.synced_count, 1);
        assert_eq!(result.failed_count, 0);
        assert!(result.success);

        // Verify element was added
        let scene = store.get("default").expect("session should exist");
        assert_eq!(scene.element_count(), 1);
    }

    #[test]
    fn test_sync_processor_tracks_failed_operations() {
        let store = Arc::new(SceneStore::new());
        let processor = SyncProcessor::new(store, ConflictStrategy::LastWriteWins);

        // Try to remove an element that doesn't exist (should fail)
        let fake_id = ElementId::new();
        let operations = vec![Operation::RemoveElement {
            id: fake_id,
            timestamp: Operation::now(),
        }];

        let result = processor.process_batch("default", operations);
        assert_eq!(result.synced_count, 0);
        assert_eq!(result.failed_count, 1);
        assert!(!result.success);

        // Verify the failed operation was tracked with error details
        assert_eq!(result.failed_operations.len(), 1);
        assert!(result.failed_operations[0].error.contains("not found"));
    }

    #[test]
    fn test_sync_processor_process_update_element() {
        let store = Arc::new(SceneStore::new());
        let processor = SyncProcessor::new(store.clone(), ConflictStrategy::LastWriteWins);

        // First, add an element
        let element = Element::new(ElementKind::Text {
            content: "Hello".to_string(),
            font_size: 24.0,
            color: "#000000".to_string(),
        });
        let element_id = element.id;
        let add_op = Operation::AddElement {
            element: element.clone(),
            timestamp: Operation::now(),
        };
        let result = processor.process_batch("default", vec![add_op]);
        assert_eq!(result.synced_count, 1);
        assert!(result.success);

        // Now update it
        let changes = serde_json::json!({
            "transform": {
                "x": 100.0,
                "y": 200.0
            }
        });
        let update_op = Operation::UpdateElement {
            id: element_id,
            changes,
            timestamp: Operation::now(),
        };
        let result = processor.process_batch("default", vec![update_op]);
        assert_eq!(result.synced_count, 1);
        assert!(result.success);

        // Verify the element was updated
        let scene = store.get("default").expect("scene should exist");
        let updated = scene.get_element(element_id).expect("element should exist");
        assert!((updated.transform.x - 100.0).abs() < f32::EPSILON);
        assert!((updated.transform.y - 200.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_sync_processor_process_remove_element() {
        let store = Arc::new(SceneStore::new());
        let processor = SyncProcessor::new(store.clone(), ConflictStrategy::LastWriteWins);

        // First, add an element
        let element = Element::new(ElementKind::Text {
            content: "ToRemove".to_string(),
            font_size: 16.0,
            color: "#ff0000".to_string(),
        });
        let element_id = element.id;
        let add_op = Operation::AddElement {
            element,
            timestamp: Operation::now(),
        };
        let result = processor.process_batch("default", vec![add_op]);
        assert!(result.success);

        // Verify element exists
        let scene = store.get("default").expect("scene should exist");
        assert!(scene.get_element(element_id).is_some());

        // Now remove it
        let remove_op = Operation::RemoveElement {
            id: element_id,
            timestamp: Operation::now(),
        };
        let result = processor.process_batch("default", vec![remove_op]);
        assert_eq!(result.synced_count, 1);
        assert!(result.success);

        // Verify element was removed
        let scene = store.get("default").expect("scene should exist");
        assert!(scene.get_element(element_id).is_none());
    }

    #[test]
    fn test_sync_processor_process_interaction() {
        use canvas_core::event::{InputEvent, TouchEvent, TouchPhase, TouchPoint};

        let store = Arc::new(SceneStore::new());
        let processor = SyncProcessor::new(store.clone(), ConflictStrategy::LastWriteWins);

        // Create an interaction operation
        let touch_point = TouchPoint {
            id: 1,
            x: 100.0,
            y: 200.0,
            pressure: None,
            radius: None,
        };
        let touch_event = TouchEvent::new(TouchPhase::Start, vec![touch_point], 0);
        let event = InputEvent::Touch(touch_event);
        let interaction_op = Operation::Interaction {
            event,
            timestamp: Operation::now(),
        };

        // Process the interaction - it should succeed but not modify the store
        let result = processor.process_batch("default", vec![interaction_op]);
        assert_eq!(result.synced_count, 1);
        assert!(result.success);

        // The scene should still be empty (interactions do not add elements)
        let document = store.scene_document("default");
        assert!(document.elements.is_empty());
    }

    #[test]
    fn test_sync_processor_update_nonexistent_element() {
        let store = Arc::new(SceneStore::new());
        let processor = SyncProcessor::new(store.clone(), ConflictStrategy::LastWriteWins);

        // Try to update an element that doesn't exist
        let fake_id = canvas_core::ElementId::new();
        let changes = serde_json::json!({
            "transform": { "x": 100.0 }
        });
        let update_op = Operation::UpdateElement {
            id: fake_id,
            changes,
            timestamp: Operation::now(),
        };

        let result = processor.process_batch("default", vec![update_op]);

        // Should fail
        assert!(!result.success);
        assert_eq!(result.failed_count, 1);
        assert_eq!(result.failed_operations.len(), 1);
        // Error should mention element not found
        assert!(result.failed_operations[0].error.contains("not found"));
    }

    #[test]
    fn test_apply_changes_ignores_nan_and_infinity() {
        let mut element = Element::new(ElementKind::Text {
            content: "Test".to_string(),
            font_size: 16.0,
            color: "#000000".to_string(),
        });
        let original_x = element.transform.x;
        let original_y = element.transform.y;

        // Try to set NaN value - should be ignored
        let changes = serde_json::json!({
            "transform": {
                "x": f64::NAN,
                "y": f64::INFINITY
            }
        });
        apply_changes_to_element(&mut element, &changes);

        // Values should remain unchanged
        assert!((element.transform.x - original_x).abs() < f32::EPSILON);
        assert!((element.transform.y - original_y).abs() < f32::EPSILON);
    }

    #[test]
    fn test_apply_changes_clamps_large_values() {
        let mut element = Element::new(ElementKind::Text {
            content: "Test".to_string(),
            font_size: 16.0,
            color: "#000000".to_string(),
        });

        // Try to set a value larger than f32::MAX
        let huge_value = f64::from(f32::MAX) * 2.0;
        let changes = serde_json::json!({
            "transform": {
                "x": huge_value
            }
        });
        apply_changes_to_element(&mut element, &changes);

        // Value should be clamped to f32::MAX
        assert!((element.transform.x - f32::MAX).abs() < 1e30);
    }

    #[test]
    fn test_apply_changes_ignores_unknown_fields() {
        let mut element = Element::new(ElementKind::Text {
            content: "Test".to_string(),
            font_size: 16.0,
            color: "#000000".to_string(),
        });

        // Include unknown fields alongside known ones
        let changes = serde_json::json!({
            "unknown_field": "should be ignored",
            "another_unknown": 42,
            "transform": {
                "x": 100.0,
                "unknown_transform_field": "also ignored"
            }
        });
        apply_changes_to_element(&mut element, &changes);

        // Known field should be applied
        assert!((element.transform.x - 100.0).abs() < f32::EPSILON);
        // Element should still be valid (no panic from unknown fields)
        assert_eq!(element.transform.x, 100.0);
    }

    #[test]
    fn test_apply_changes_partial_transform() {
        let mut element = Element::new(ElementKind::Text {
            content: "Test".to_string(),
            font_size: 16.0,
            color: "#000000".to_string(),
        });
        // Set initial transform values
        element.transform.x = 10.0;
        element.transform.y = 20.0;
        element.transform.width = 100.0;
        element.transform.height = 50.0;

        // Only update x, leave other fields unchanged
        let changes = serde_json::json!({
            "transform": {
                "x": 500.0
            }
        });
        apply_changes_to_element(&mut element, &changes);

        // x should be updated
        assert!((element.transform.x - 500.0).abs() < f32::EPSILON);
        // Other fields should remain unchanged
        assert!((element.transform.y - 20.0).abs() < f32::EPSILON);
        assert!((element.transform.width - 100.0).abs() < f32::EPSILON);
        assert!((element.transform.height - 50.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_apply_changes_all_transform_fields() {
        let mut element = Element::new(ElementKind::Text {
            content: "Test".to_string(),
            font_size: 16.0,
            color: "#000000".to_string(),
        });

        let changes = serde_json::json!({
            "transform": {
                "x": 100.0,
                "y": 200.0,
                "width": 300.0,
                "height": 400.0,
                "rotation": 1.57,
                "z_index": 5
            },
            "interactive": true
        });
        apply_changes_to_element(&mut element, &changes);

        assert!((element.transform.x - 100.0).abs() < f32::EPSILON);
        assert!((element.transform.y - 200.0).abs() < f32::EPSILON);
        assert!((element.transform.width - 300.0).abs() < f32::EPSILON);
        assert!((element.transform.height - 400.0).abs() < f32::EPSILON);
        assert!((element.transform.rotation - 1.57).abs() < f32::EPSILON);
        assert_eq!(element.transform.z_index, 5);
        assert!(element.interactive);
    }

    #[test]
    fn test_apply_changes_z_index_clamping() {
        let mut element = Element::new(ElementKind::Text {
            content: "Test".to_string(),
            font_size: 16.0,
            color: "#000000".to_string(),
        });

        // Try to set z_index larger than i32::MAX
        let huge_z = i64::from(i32::MAX) + 1000;
        let changes = serde_json::json!({
            "transform": {
                "z_index": huge_z
            }
        });
        apply_changes_to_element(&mut element, &changes);

        // Should be clamped to i32::MAX
        assert_eq!(element.transform.z_index, i32::MAX);
    }

    #[test]
    fn test_sync_processor_mixed_success_and_failure_batch() {
        let store = Arc::new(SceneStore::new());
        let processor = SyncProcessor::new(store.clone(), ConflictStrategy::LastWriteWins);

        // Create a valid element
        let element = Element::new(ElementKind::Text {
            content: "Valid".to_string(),
            font_size: 16.0,
            color: "#000000".to_string(),
        });
        let valid_id = element.id;

        // Create a batch with: add (success), update non-existent (fail), add another (success)
        let fake_id = canvas_core::ElementId::new();
        let operations = vec![
            Operation::AddElement {
                element,
                timestamp: Operation::now(),
            },
            Operation::UpdateElement {
                id: fake_id,
                changes: serde_json::json!({"transform": {"x": 100.0}}),
                timestamp: Operation::now(),
            },
            Operation::RemoveElement {
                id: valid_id,
                timestamp: Operation::now(),
            },
        ];

        let result = processor.process_batch("default", operations);

        // 2 successes, 1 failure
        assert_eq!(result.synced_count, 2);
        assert_eq!(result.failed_count, 1);
        assert!(!result.success);
        assert_eq!(result.failed_operations.len(), 1);
    }

    // ========================
    // Conflict Detection Tests
    // ========================

    #[test]
    fn test_conflict_reason_display() {
        let not_found = ConflictReason::ElementNotFound;
        assert_eq!(not_found.to_string(), "element not found");

        let exists = ConflictReason::ElementAlreadyExists;
        assert_eq!(exists.to_string(), "element already exists");

        let stale = ConflictReason::StaleTimestamp {
            local: 200,
            remote: 100,
        };
        assert_eq!(stale.to_string(), "stale timestamp (local=200, remote=100)");

        let concurrent = ConflictReason::ConcurrentModification;
        assert_eq!(concurrent.to_string(), "concurrent modification");
    }

    #[test]
    fn test_conflict_new_and_mark_resolved() {
        let op = Operation::RemoveElement {
            id: canvas_core::ElementId::new(),
            timestamp: 100,
        };
        let mut conflict = Conflict::new(op.clone(), ConflictReason::ElementNotFound);

        assert!(!conflict.resolved);
        assert!(matches!(conflict.reason, ConflictReason::ElementNotFound));

        conflict.mark_resolved();
        assert!(conflict.resolved);
    }

    #[test]
    fn test_detect_conflict_element_not_found_on_update() {
        let store = Arc::new(SceneStore::new());
        let processor = SyncProcessor::new(store.clone(), ConflictStrategy::LastWriteWins);

        let fake_id = canvas_core::ElementId::new();
        let op = Operation::UpdateElement {
            id: fake_id,
            changes: serde_json::json!({"transform": {"x": 100.0}}),
            timestamp: 100,
        };

        let conflict = processor.detect_conflict("default", &op);
        assert!(conflict.is_some());
        let c = conflict.expect("should have conflict");
        assert!(matches!(c.reason, ConflictReason::ElementNotFound));
    }

    #[test]
    fn test_detect_conflict_element_not_found_on_remove() {
        let store = Arc::new(SceneStore::new());
        let processor = SyncProcessor::new(store.clone(), ConflictStrategy::LastWriteWins);

        let fake_id = canvas_core::ElementId::new();
        let op = Operation::RemoveElement {
            id: fake_id,
            timestamp: 100,
        };

        let conflict = processor.detect_conflict("default", &op);
        assert!(conflict.is_some());
        let c = conflict.expect("should have conflict");
        assert!(matches!(c.reason, ConflictReason::ElementNotFound));
    }

    #[test]
    fn test_detect_conflict_element_already_exists() {
        let store = Arc::new(SceneStore::new());
        let processor = SyncProcessor::new(store.clone(), ConflictStrategy::LastWriteWins);

        let element = Element::new(ElementKind::Text {
            content: "Test".to_string(),
            font_size: 16.0,
            color: "#000000".to_string(),
        });
        let _element_id = element.id;
        let _ = store.add_element("default", element.clone());

        let op = Operation::AddElement {
            element: element.clone(),
            timestamp: 100,
        };

        let conflict = processor.detect_conflict("default", &op);
        assert!(conflict.is_some());
        let c = conflict.expect("should have conflict");
        assert!(matches!(c.reason, ConflictReason::ElementAlreadyExists));
    }

    #[test]
    fn test_detect_conflict_stale_timestamp() {
        let store = Arc::new(SceneStore::new());
        let processor = SyncProcessor::new(store.clone(), ConflictStrategy::LastWriteWins);

        let element = Element::new(ElementKind::Text {
            content: "Test".to_string(),
            font_size: 16.0,
            color: "#000000".to_string(),
        });
        let element_id = element.id;

        processor.update_timestamp(element_id, 200);

        let op = Operation::UpdateElement {
            id: element_id,
            changes: serde_json::json!({"transform": {"x": 100.0}}),
            timestamp: 100,
        };

        let _conflict = processor.detect_conflict("default", &op);
        let _ = store.add_element("default", element);

        let conflict2 = processor.detect_conflict("default", &op);
        assert!(conflict2.is_some());
        if let Some(c) = conflict2 {
            match c.reason {
                ConflictReason::StaleTimestamp { local, remote } => {
                    assert_eq!(local, 200);
                    assert_eq!(remote, 100);
                }
                _ => panic!("Expected StaleTimestamp conflict"),
            }
        }
    }

    #[test]
    fn test_detect_no_conflict_for_interaction() {
        let store = Arc::new(SceneStore::new());
        let processor = SyncProcessor::new(store.clone(), ConflictStrategy::LastWriteWins);

        let op = Operation::Interaction {
            event: canvas_core::InputEvent::Touch(canvas_core::TouchEvent::new(
                canvas_core::TouchPhase::Start,
                vec![],
                100,
            )),
            timestamp: 100,
        };

        let conflict = processor.detect_conflict("default", &op);
        assert!(conflict.is_none());
    }

    #[test]
    fn test_resolve_conflict_element_not_found_keeps_local() {
        let store = Arc::new(SceneStore::new());
        let processor = SyncProcessor::new(store, ConflictStrategy::RemoteWins);

        let op = Operation::RemoveElement {
            id: canvas_core::ElementId::new(),
            timestamp: 100,
        };
        let conflict = Conflict::new(op, ConflictReason::ElementNotFound);

        let resolution = processor.resolve_conflict(&conflict);
        assert_eq!(resolution, ConflictResolution::KeepLocal);
    }

    #[test]
    fn test_resolve_conflict_already_exists_last_write_wins() {
        let store = Arc::new(SceneStore::new());
        let processor = SyncProcessor::new(store, ConflictStrategy::LastWriteWins);

        let element = Element::new(ElementKind::Text {
            content: "Test".to_string(),
            font_size: 16.0,
            color: "#000000".to_string(),
        });
        let op = Operation::AddElement {
            element,
            timestamp: 100,
        };
        let conflict = Conflict::new(op, ConflictReason::ElementAlreadyExists);

        let resolution = processor.resolve_conflict(&conflict);
        assert_eq!(resolution, ConflictResolution::KeepRemote);
    }

    #[test]
    fn test_resolve_conflict_already_exists_local_wins() {
        let store = Arc::new(SceneStore::new());
        let processor = SyncProcessor::new(store, ConflictStrategy::LocalWins);

        let element = Element::new(ElementKind::Text {
            content: "Test".to_string(),
            font_size: 16.0,
            color: "#000000".to_string(),
        });
        let op = Operation::AddElement {
            element,
            timestamp: 100,
        };
        let conflict = Conflict::new(op, ConflictReason::ElementAlreadyExists);

        let resolution = processor.resolve_conflict(&conflict);
        assert_eq!(resolution, ConflictResolution::KeepLocal);
    }

    #[test]
    fn test_resolve_conflict_stale_timestamp_last_write_wins() {
        let store = Arc::new(SceneStore::new());
        let processor = SyncProcessor::new(store, ConflictStrategy::LastWriteWins);

        let op = Operation::UpdateElement {
            id: canvas_core::ElementId::new(),
            changes: serde_json::json!({}),
            timestamp: 100,
        };
        let conflict = Conflict::new(
            op,
            ConflictReason::StaleTimestamp {
                local: 200,
                remote: 100,
            },
        );

        let resolution = processor.resolve_conflict(&conflict);
        assert_eq!(resolution, ConflictResolution::KeepLocal);
    }

    #[test]
    fn test_resolve_conflict_stale_timestamp_remote_wins() {
        let store = Arc::new(SceneStore::new());
        let processor = SyncProcessor::new(store, ConflictStrategy::RemoteWins);

        let op = Operation::UpdateElement {
            id: canvas_core::ElementId::new(),
            changes: serde_json::json!({}),
            timestamp: 100,
        };
        let conflict = Conflict::new(
            op,
            ConflictReason::StaleTimestamp {
                local: 200,
                remote: 100,
            },
        );

        let resolution = processor.resolve_conflict(&conflict);
        assert_eq!(resolution, ConflictResolution::KeepRemote);
    }

    #[test]
    fn test_resolve_conflict_concurrent_modification() {
        let store = Arc::new(SceneStore::new());
        let processor = SyncProcessor::new(store, ConflictStrategy::LocalWins);

        let op = Operation::UpdateElement {
            id: canvas_core::ElementId::new(),
            changes: serde_json::json!({}),
            timestamp: 100,
        };
        let conflict = Conflict::new(op, ConflictReason::ConcurrentModification);

        let resolution = processor.resolve_conflict(&conflict);
        assert_eq!(resolution, ConflictResolution::KeepLocal);
    }

    #[test]
    fn test_process_batch_tracks_conflicts() {
        let store = Arc::new(SceneStore::new());
        let processor = SyncProcessor::new(store.clone(), ConflictStrategy::LastWriteWins);

        let element = Element::new(ElementKind::Text {
            content: "Test".to_string(),
            font_size: 16.0,
            color: "#000000".to_string(),
        });
        let _element_id = element.id;
        let _ = store.add_element("default", element.clone());

        let operations = vec![Operation::AddElement {
            element: element.clone(),
            timestamp: 100,
        }];

        let result = processor.process_batch("default", operations);

        assert_eq!(result.conflicts.len(), 1);
        assert!(result.conflicts[0].resolved);
        assert!(matches!(
            result.conflicts[0].reason,
            ConflictReason::ElementAlreadyExists
        ));
    }

    #[test]
    fn test_sync_processor_result_has_conflicts_field() {
        let result = SyncProcessorResult::default();
        assert!(result.conflicts.is_empty());

        let mut result2 = SyncProcessorResult::default();
        let op = Operation::RemoveElement {
            id: canvas_core::ElementId::new(),
            timestamp: 100,
        };
        result2
            .conflicts
            .push(Conflict::new(op, ConflictReason::ElementNotFound));
        assert_eq!(result2.conflicts.len(), 1);
    }

    #[test]
    fn test_sync_processor_result_metrics() {
        let mut result = SyncProcessorResult::new();
        assert!(result.success);
        assert_eq!(result.synced_count, 0);
        assert_eq!(result.failed_count, 0);
        assert_eq!(result.conflict_count, 0);
        assert!(result.timestamp > 0);

        result.record_success();
        assert_eq!(result.synced_count, 1);
        assert!(result.success);

        let op = Operation::RemoveElement {
            id: canvas_core::ElementId::new(),
            timestamp: 100,
        };
        result.record_failure(FailedOperation::retryable(
            op.clone(),
            "element not found".to_string(),
        ));
        assert_eq!(result.failed_count, 1);
        assert!(!result.success);

        let conflict = Conflict::new(op, ConflictReason::ElementNotFound);
        result.record_conflict(conflict);
        assert_eq!(result.conflict_count, 1);
    }

    #[test]
    fn test_sync_processor_result_timing() {
        let start = std::time::Instant::now();
        let mut result = SyncProcessorResult::new();

        // Simulate some work
        std::thread::sleep(std::time::Duration::from_millis(10));

        result.finalize(start);

        // Duration should be at least 10ms
        assert!(result.duration_ms >= 10);
        // Timestamp should be recent (within last minute)
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        assert!(result.timestamp > now_ms - 60_000);
        assert!(result.timestamp <= now_ms);
    }

    #[test]
    fn test_failed_operation_retryable_flag() {
        let op = Operation::AddElement {
            element: Element::new(ElementKind::Text {
                content: "Test".to_string(),
                font_size: 16.0,
                color: "#000000".to_string(),
            }),
            timestamp: 100,
        };

        let retryable_failure =
            FailedOperation::retryable(op.clone(), "temporary error".to_string());
        assert!(retryable_failure.retryable);
        assert_eq!(retryable_failure.error, "temporary error");

        let permanent_failure = FailedOperation::permanent(op.clone(), "invalid data".to_string());
        assert!(!permanent_failure.retryable);
        assert_eq!(permanent_failure.error, "invalid data");
    }

    #[test]
    fn test_sync_processor_result_retryable_counts() {
        let mut result = SyncProcessorResult::new();

        let op1 = Operation::RemoveElement {
            id: canvas_core::ElementId::new(),
            timestamp: 100,
        };
        let op2 = Operation::RemoveElement {
            id: canvas_core::ElementId::new(),
            timestamp: 101,
        };
        let op3 = Operation::RemoveElement {
            id: canvas_core::ElementId::new(),
            timestamp: 102,
        };

        result.record_failure(FailedOperation::retryable(op1, "error1".to_string()));
        result.record_failure(FailedOperation::permanent(op2, "error2".to_string()));
        result.record_failure(FailedOperation::retryable(op3, "error3".to_string()));

        assert_eq!(result.failed_count, 3);
        assert_eq!(result.retryable_count(), 2);
        assert_eq!(result.permanent_count(), 1);
    }

    #[test]
    fn test_sync_processor_result_serialization() {
        let mut result = SyncProcessorResult::new();
        result.record_success();

        let op = Operation::RemoveElement {
            id: canvas_core::ElementId::new(),
            timestamp: 100,
        };
        result.record_failure(FailedOperation::retryable(op, "test error".to_string()));

        // Should serialize without panic
        let json = serde_json::to_string(&result).expect("should serialize");
        assert!(json.contains(r#""success":false"#));
        assert!(json.contains(r#""synced_count":1"#));
        assert!(json.contains(r#""failed_count":1"#));
        assert!(json.contains(r#""retryable":true"#));
    }

    // Tests for RetryConfig
    #[test]
    fn test_retry_config_default() {
        let config = RetryConfig::default();
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.initial_delay_ms, 100);
        assert_eq!(config.max_delay_ms, 5000);
        assert!((config.backoff_multiplier - 2.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_retry_config_new() {
        let config = RetryConfig::new(5, 200, 10000, 1.5);
        assert_eq!(config.max_retries, 5);
        assert_eq!(config.initial_delay_ms, 200);
        assert_eq!(config.max_delay_ms, 10000);
        assert!((config.backoff_multiplier - 1.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_retry_config_delay_for_attempt_exponential() {
        let config = RetryConfig::new(3, 100, 5000, 2.0);

        // attempt 0: 100 * 2^0 = 100ms
        assert_eq!(config.delay_for_attempt(0).as_millis(), 100);
        // attempt 1: 100 * 2^1 = 200ms
        assert_eq!(config.delay_for_attempt(1).as_millis(), 200);
        // attempt 2: 100 * 2^2 = 400ms
        assert_eq!(config.delay_for_attempt(2).as_millis(), 400);
        // attempt 3: 100 * 2^3 = 800ms
        assert_eq!(config.delay_for_attempt(3).as_millis(), 800);
    }

    #[test]
    fn test_retry_config_delay_capped_at_max() {
        let config = RetryConfig::new(10, 100, 500, 2.0);

        // attempt 3: 100 * 2^3 = 800ms, capped at 500ms
        assert_eq!(config.delay_for_attempt(3).as_millis(), 500);
        // attempt 10: would be huge, but capped at 500ms
        assert_eq!(config.delay_for_attempt(10).as_millis(), 500);
    }

    #[test]
    fn test_retry_config_serialization() {
        let config = RetryConfig::default();
        let json = serde_json::to_string(&config).expect("should serialize");
        assert!(json.contains(r#""max_retries":3"#));
        assert!(json.contains(r#""initial_delay_ms":100"#));
        assert!(json.contains(r#""max_delay_ms":5000"#));
        assert!(json.contains(r#""backoff_multiplier":2.0"#));

        // Deserialize back
        let parsed: RetryConfig = serde_json::from_str(&json).expect("should deserialize");
        assert_eq!(parsed.max_retries, 3);
    }

    #[tokio::test]
    async fn test_process_with_retry_no_failures() {
        let store = Arc::new(SceneStore::new());
        let processor = SyncProcessor::new(store.clone(), ConflictStrategy::LastWriteWins);

        let element = Element::new(ElementKind::Text {
            content: "test text".to_string(),
            font_size: 16.0,
            color: "#ffffff".to_string(),
        });
        let operations = vec![Operation::AddElement {
            element: element.clone(),
            timestamp: 1000,
        }];

        let config = RetryConfig::default();
        let result = processor
            .process_with_retry("test-session", operations, &config)
            .await;

        assert!(result.success);
        assert_eq!(result.synced_count, 1);
        assert_eq!(result.failed_count, 0);
    }

    #[tokio::test]
    async fn test_process_with_retry_permanent_failure_no_retry() {
        let store = Arc::new(SceneStore::new());
        let processor = SyncProcessor::new(store.clone(), ConflictStrategy::LastWriteWins);

        // Try to update a non-existent element (permanent failure - element not found)
        let operations = vec![Operation::UpdateElement {
            id: canvas_core::ElementId::new(),
            changes: serde_json::json!({"z_index": 5}),
            timestamp: 1000,
        }];

        let config = RetryConfig::new(3, 10, 100, 2.0); // Short delays for testing
        let result = processor
            .process_with_retry("test-session", operations, &config)
            .await;

        assert!(!result.success);
        assert_eq!(result.synced_count, 0);
        assert_eq!(result.failed_count, 1);
        // Should not be retried because element not found is permanent
        assert_eq!(result.retryable_count(), 1);
    }

    #[tokio::test]
    async fn test_process_with_retry_zero_retries_config() {
        let store = Arc::new(SceneStore::new());
        let processor = SyncProcessor::new(store.clone(), ConflictStrategy::LastWriteWins);

        let element = Element::new(ElementKind::Text {
            content: "test".to_string(),
            font_size: 16.0,
            color: "#ffffff".to_string(),
        });
        let operations = vec![Operation::AddElement {
            element,
            timestamp: 1000,
        }];

        let config = RetryConfig::new(0, 100, 5000, 2.0);
        let result = processor
            .process_with_retry("test-session", operations, &config)
            .await;

        // Should still process successfully with zero retries
        assert!(result.success);
        assert_eq!(result.synced_count, 1);
    }

    // ============================================================
    // Additional SyncProcessor Tests - Comprehensive Coverage
    // ============================================================

    #[test]
    fn test_process_batch_mixed_operations() {
        let store = Arc::new(SceneStore::new());
        let processor = SyncProcessor::new(store.clone(), ConflictStrategy::LastWriteWins);

        let element1 = Element::new(ElementKind::Text {
            content: "First".to_string(),
            font_size: 16.0,
            color: "#000000".to_string(),
        });
        let element2 = Element::new(ElementKind::Text {
            content: "Second".to_string(),
            font_size: 18.0,
            color: "#ffffff".to_string(),
        });
        let element1_id = element1.id;
        let element2_id = element2.id;

        let operations = vec![
            Operation::AddElement {
                element: element1,
                timestamp: 1000,
            },
            Operation::AddElement {
                element: element2,
                timestamp: 1001,
            },
            Operation::UpdateElement {
                id: element1_id,
                changes: serde_json::json!({"transform": {"x": 50.0}}),
                timestamp: 1002,
            },
            Operation::RemoveElement {
                id: element2_id,
                timestamp: 1003,
            },
        ];

        let result = processor.process_batch("default", operations);

        assert!(result.success);
        assert_eq!(result.synced_count, 4);
        assert_eq!(result.failed_count, 0);

        let scene = store.get("default").expect("scene should exist");
        assert_eq!(scene.element_count(), 1);
        let updated_element = scene
            .get_element(element1_id)
            .expect("element1 should exist");
        assert!((updated_element.transform.x - 50.0).abs() < f32::EPSILON);
        assert!(scene.get_element(element2_id).is_none());
    }

    #[test]
    fn test_no_conflict_valid_operations_sequence() {
        let store = Arc::new(SceneStore::new());
        let processor = SyncProcessor::new(store.clone(), ConflictStrategy::LastWriteWins);

        let element = Element::new(ElementKind::Text {
            content: "Test".to_string(),
            font_size: 16.0,
            color: "#000000".to_string(),
        });
        let element_id = element.id;
        let add_op = Operation::AddElement {
            element,
            timestamp: 1000,
        };
        let result = processor.process_batch("default", vec![add_op]);
        assert!(result.success);
        assert_eq!(result.conflict_count, 0);

        let update_op = Operation::UpdateElement {
            id: element_id,
            changes: serde_json::json!({"transform": {"x": 100.0}}),
            timestamp: 2000,
        };
        let result = processor.process_batch("default", vec![update_op]);
        assert!(result.success);
        assert_eq!(result.conflict_count, 0);

        let remove_op = Operation::RemoveElement {
            id: element_id,
            timestamp: 3000,
        };
        let result = processor.process_batch("default", vec![remove_op]);
        assert!(result.success);
        assert_eq!(result.conflict_count, 0);
    }

    #[test]
    fn test_resolve_conflict_last_write_wins_comprehensive() {
        let store = Arc::new(SceneStore::new());
        let processor = SyncProcessor::new(store.clone(), ConflictStrategy::LastWriteWins);

        // Test ElementAlreadyExists - should keep remote (the incoming add)
        let conflict = Conflict::new(
            Operation::AddElement {
                element: Element::new(ElementKind::Text {
                    content: "Test".to_string(),
                    font_size: 16.0,
                    color: "#000000".to_string(),
                }),
                timestamp: 1000,
            },
            ConflictReason::ElementAlreadyExists,
        );
        let resolution = processor.resolve_conflict(&conflict);
        assert_eq!(resolution, ConflictResolution::KeepRemote);

        // Test StaleTimestamp - should keep local (newer wins)
        let conflict = Conflict::new(
            Operation::UpdateElement {
                id: canvas_core::ElementId::new(),
                changes: serde_json::json!({}),
                timestamp: 1000,
            },
            ConflictReason::StaleTimestamp {
                local: 2000,
                remote: 1000,
            },
        );
        let resolution = processor.resolve_conflict(&conflict);
        assert_eq!(resolution, ConflictResolution::KeepLocal);

        // Test ElementNotFound - always keep local
        let conflict = Conflict::new(
            Operation::RemoveElement {
                id: canvas_core::ElementId::new(),
                timestamp: 1000,
            },
            ConflictReason::ElementNotFound,
        );
        let resolution = processor.resolve_conflict(&conflict);
        assert_eq!(resolution, ConflictResolution::KeepLocal);
    }

    #[test]
    fn test_resolve_conflict_local_wins_comprehensive() {
        let store = Arc::new(SceneStore::new());
        let processor = SyncProcessor::new(store.clone(), ConflictStrategy::LocalWins);

        // Test ElementAlreadyExists - should keep local
        let conflict = Conflict::new(
            Operation::AddElement {
                element: Element::new(ElementKind::Text {
                    content: "Test".to_string(),
                    font_size: 16.0,
                    color: "#000000".to_string(),
                }),
                timestamp: 1000,
            },
            ConflictReason::ElementAlreadyExists,
        );
        let resolution = processor.resolve_conflict(&conflict);
        assert_eq!(resolution, ConflictResolution::KeepLocal);

        // Test StaleTimestamp - should keep local
        let conflict = Conflict::new(
            Operation::UpdateElement {
                id: canvas_core::ElementId::new(),
                changes: serde_json::json!({}),
                timestamp: 1000,
            },
            ConflictReason::StaleTimestamp {
                local: 500,
                remote: 1000,
            },
        );
        let resolution = processor.resolve_conflict(&conflict);
        assert_eq!(resolution, ConflictResolution::KeepLocal);

        // Test ConcurrentModification - should keep local
        let conflict = Conflict::new(
            Operation::UpdateElement {
                id: canvas_core::ElementId::new(),
                changes: serde_json::json!({}),
                timestamp: 1000,
            },
            ConflictReason::ConcurrentModification,
        );
        let resolution = processor.resolve_conflict(&conflict);
        assert_eq!(resolution, ConflictResolution::KeepLocal);
    }

    #[test]
    fn test_resolve_conflict_remote_wins_comprehensive() {
        let store = Arc::new(SceneStore::new());
        let processor = SyncProcessor::new(store.clone(), ConflictStrategy::RemoteWins);

        // Test ElementAlreadyExists - should keep remote
        let conflict = Conflict::new(
            Operation::AddElement {
                element: Element::new(ElementKind::Text {
                    content: "Test".to_string(),
                    font_size: 16.0,
                    color: "#000000".to_string(),
                }),
                timestamp: 1000,
            },
            ConflictReason::ElementAlreadyExists,
        );
        let resolution = processor.resolve_conflict(&conflict);
        assert_eq!(resolution, ConflictResolution::KeepRemote);

        // Test StaleTimestamp - should keep remote (even though older)
        let conflict = Conflict::new(
            Operation::UpdateElement {
                id: canvas_core::ElementId::new(),
                changes: serde_json::json!({}),
                timestamp: 1000,
            },
            ConflictReason::StaleTimestamp {
                local: 2000,
                remote: 1000,
            },
        );
        let resolution = processor.resolve_conflict(&conflict);
        assert_eq!(resolution, ConflictResolution::KeepRemote);

        // Test ConcurrentModification - should keep remote
        let conflict = Conflict::new(
            Operation::UpdateElement {
                id: canvas_core::ElementId::new(),
                changes: serde_json::json!({}),
                timestamp: 1000,
            },
            ConflictReason::ConcurrentModification,
        );
        let resolution = processor.resolve_conflict(&conflict);
        assert_eq!(resolution, ConflictResolution::KeepRemote);
    }

    #[test]
    fn test_retry_config_default_values_check() {
        let config = RetryConfig::default();
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.initial_delay_ms, 100);
        assert_eq!(config.max_delay_ms, 5000);
        assert!((config.backoff_multiplier - 2.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_retry_config_exponential_backoff_sequence() {
        let config = RetryConfig::new(5, 100, 10000, 2.0);

        assert_eq!(config.delay_for_attempt(0).as_millis(), 100);
        assert_eq!(config.delay_for_attempt(1).as_millis(), 200);
        assert_eq!(config.delay_for_attempt(2).as_millis(), 400);
        assert_eq!(config.delay_for_attempt(3).as_millis(), 800);
        assert_eq!(config.delay_for_attempt(4).as_millis(), 1600);
    }

    #[tokio::test]
    async fn test_process_with_retry_success_on_first_attempt() {
        let store = Arc::new(SceneStore::new());
        let processor = SyncProcessor::new(store.clone(), ConflictStrategy::LastWriteWins);

        let element = Element::new(ElementKind::Text {
            content: "Success".to_string(),
            font_size: 16.0,
            color: "#000000".to_string(),
        });
        let operations = vec![Operation::AddElement {
            element,
            timestamp: 1000,
        }];

        let config = RetryConfig::new(3, 10, 100, 2.0);
        let result = processor
            .process_with_retry("test-session", operations, &config)
            .await;

        assert!(result.success);
        assert_eq!(result.synced_count, 1);
        assert_eq!(result.failed_count, 0);
    }

    #[tokio::test]
    async fn test_process_with_retry_exhausts_all_retries() {
        let store = Arc::new(SceneStore::new());
        let processor = SyncProcessor::new(store.clone(), ConflictStrategy::LastWriteWins);

        let element = Element::new(ElementKind::Text {
            content: "Exists".to_string(),
            font_size: 16.0,
            color: "#000000".to_string(),
        });
        let element_id = element.id;
        let add_op = Operation::AddElement {
            element: element.clone(),
            timestamp: 1000,
        };
        let result = processor.process_batch("test-session", vec![add_op]);
        assert!(result.success);

        let processor = SyncProcessor::new(store.clone(), ConflictStrategy::LocalWins);
        let operations = vec![Operation::AddElement {
            element,
            timestamp: 2000,
        }];

        let config = RetryConfig::new(2, 5, 20, 2.0);
        let result = processor
            .process_with_retry("test-session", operations, &config)
            .await;

        assert!(!result.success);
        assert_eq!(result.synced_count, 0);

        let scene = store.get("test-session").expect("scene should exist");
        assert!(scene.get_element(element_id).is_some());
    }

    #[test]
    fn test_conflict_element_not_found_marked_retryable() {
        // When an element is not found, the conflict is detected and resolved
        // with KeepLocal. The failure is marked as RETRYABLE because the state
        // may change (another client might add the element).
        let store = Arc::new(SceneStore::new());
        let processor = SyncProcessor::new(store.clone(), ConflictStrategy::LastWriteWins);

        let fake_id = canvas_core::ElementId::new();
        let operations = vec![Operation::UpdateElement {
            id: fake_id,
            changes: serde_json::json!({"transform": {"x": 100.0}}),
            timestamp: 1000,
        }];

        let result = processor.process_batch("default", operations);

        assert!(!result.success);
        assert_eq!(result.failed_count, 1);
        assert_eq!(result.failed_operations.len(), 1);

        // ElementNotFound through conflict path is retryable (state may change)
        assert!(
            result.failed_operations[0].retryable,
            "ElementNotFound conflict should be retryable (state may change)"
        );
    }

    #[test]
    fn test_sync_processor_result_retryable_and_permanent_counts() {
        let mut result = SyncProcessorResult::new();

        result.record_failure(FailedOperation::retryable(
            Operation::AddElement {
                element: Element::new(ElementKind::Text {
                    content: "T".to_string(),
                    font_size: 16.0,
                    color: "#000".to_string(),
                }),
                timestamp: 1000,
            },
            "transient error".to_string(),
        ));
        result.record_failure(FailedOperation::permanent(
            Operation::RemoveElement {
                id: canvas_core::ElementId::new(),
                timestamp: 1001,
            },
            "permanent error".to_string(),
        ));
        result.record_failure(FailedOperation::retryable(
            Operation::UpdateElement {
                id: canvas_core::ElementId::new(),
                changes: serde_json::json!({}),
                timestamp: 1002,
            },
            "another transient".to_string(),
        ));

        assert_eq!(result.retryable_count(), 2);
        assert_eq!(result.permanent_count(), 1);
        assert_eq!(result.failed_count, 3);
    }

    #[test]
    fn test_sync_processor_result_finalize_success_flag() {
        use std::time::Instant;

        let mut result = SyncProcessorResult::new();
        result.record_success();
        result.record_success();
        result.finalize(Instant::now());

        assert!(result.success);
        assert_eq!(result.synced_count, 2);

        let mut result = SyncProcessorResult::new();
        result.record_success();
        result.record_failure(FailedOperation::permanent(
            Operation::RemoveElement {
                id: canvas_core::ElementId::new(),
                timestamp: 1000,
            },
            "error".to_string(),
        ));
        result.finalize(Instant::now());

        assert!(!result.success);
        assert_eq!(result.synced_count, 1);
        assert_eq!(result.failed_count, 1);
    }

    #[test]
    fn test_conflict_mark_resolved_state() {
        let mut conflict = Conflict::new(
            Operation::AddElement {
                element: Element::new(ElementKind::Text {
                    content: "T".to_string(),
                    font_size: 16.0,
                    color: "#000".to_string(),
                }),
                timestamp: 1000,
            },
            ConflictReason::ElementAlreadyExists,
        );

        assert!(!conflict.resolved);
        conflict.mark_resolved();
        assert!(conflict.resolved);
    }

    #[test]
    fn test_failed_operation_constructors_retryable_vs_permanent() {
        let op = Operation::RemoveElement {
            id: canvas_core::ElementId::new(),
            timestamp: 1000,
        };

        let retryable = FailedOperation::retryable(op.clone(), "temp error".to_string());
        assert!(retryable.retryable);
        assert_eq!(retryable.error, "temp error");

        let permanent = FailedOperation::permanent(op, "permanent error".to_string());
        assert!(!permanent.retryable);
        assert_eq!(permanent.error, "permanent error");
    }

    mod proptest_tests {
        use super::*;
        use proptest::prelude::*;

        fn arb_element_kind() -> impl Strategy<Value = ElementKind> {
            (any::<String>(), 1.0f32..100.0f32, any::<String>()).prop_map(
                |(content, font_size, color)| ElementKind::Text {
                    content: content.chars().take(100).collect(),
                    font_size,
                    color: format!("#{:06x}", color.len() % 0xFFFFFF),
                },
            )
        }

        fn arb_operation() -> impl Strategy<Value = Operation> {
            prop_oneof![
                (arb_element_kind(), 0u64..u64::MAX).prop_map(|(kind, timestamp)| {
                    Operation::AddElement {
                        element: Element::new(kind),
                        timestamp,
                    }
                }),
                (any::<u64>(), 0u64..u64::MAX).prop_map(|(_, timestamp)| {
                    Operation::RemoveElement {
                        id: canvas_core::ElementId::new(),
                        timestamp,
                    }
                }),
            ]
        }

        proptest! {
            #[test]
            fn prop_batch_processing_maintains_invariants(
                ops in prop::collection::vec(arb_operation(), 0..10)
            ) {
                let store = Arc::new(SceneStore::new());
                let processor = SyncProcessor::new(store.clone(), ConflictStrategy::LastWriteWins);

                let result = processor.process_batch("prop-test", ops.clone());

                let total = ops.len();
                prop_assert!(
                    result.synced_count + result.failed_count <= total,
                    "synced_count ({}) + failed_count ({}) should not exceed total ({})",
                    result.synced_count, result.failed_count, total
                );

                prop_assert_eq!(
                    result.success,
                    result.failed_count == 0,
                    "success flag should match failed_count == 0"
                );

                prop_assert_eq!(
                    result.failed_operations.len(),
                    result.failed_count,
                    "failed_operations length should match failed_count"
                );

                prop_assert_eq!(
                    result.conflicts.len(),
                    result.conflict_count,
                    "conflicts length should match conflict_count"
                );
            }

            #[test]
            fn prop_retry_delay_never_exceeds_max(
                initial_ms in 1u64..1000u64,
                max_ms in 100u64..10000u64,
                multiplier in 1.0f64..5.0f64,
                attempt in 0u32..20u32
            ) {
                let max_delay = initial_ms.max(max_ms);
                let config = RetryConfig::new(10, initial_ms, max_delay, multiplier);

                let delay = config.delay_for_attempt(attempt);

                prop_assert!(
                    delay.as_millis() <= max_delay as u128,
                    "delay {} should not exceed max_delay {}",
                    delay.as_millis(),
                    max_delay
                );
            }

            #[test]
            fn prop_empty_batch_always_succeeds(_seed in any::<u64>()) {
                let store = Arc::new(SceneStore::new());
                let processor = SyncProcessor::new(store, ConflictStrategy::LastWriteWins);

                let result = processor.process_batch("empty-test", vec![]);

                prop_assert!(result.success);
                prop_assert_eq!(result.synced_count, 0);
                prop_assert_eq!(result.failed_count, 0);
                prop_assert_eq!(result.conflict_count, 0);
            }
        }
    }
}
