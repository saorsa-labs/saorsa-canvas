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
    Element, ElementDocument, ElementId, OfflineQueue, Scene, SceneDocument, SceneStore, StoreError,
};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tokio::sync::mpsc;
use uuid::Uuid;

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
    /// Get the type of this operation as a string.
    #[must_use]
    pub fn operation_type(&self) -> &'static str {
        match self {
            Self::Add { .. } => "add",
            Self::Update { .. } => "update",
            Self::Remove { .. } => "remove",
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

/// Information about a failed sync operation sent to clients.
///
/// This struct provides minimal but useful details about operations
/// that failed during queue processing, enabling client-side reconciliation.
#[derive(Debug, Clone, Serialize)]
pub struct FailedOperationInfo {
    /// Type of operation that failed (add, update, remove).
    pub operation: String,
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
            operation: op.operation_type().to_string(),
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
        Self {
            store: SceneStore::new(),
            event_tx,
            offline_queue: Arc::new(RwLock::new(OfflineQueue::new())),
            peers: Arc::new(RwLock::new(HashMap::new())),
            active_calls: Arc::new(RwLock::new(HashMap::new())),
            communitas: Arc::new(RwLock::new(None)),
            conflict_count: Arc::new(AtomicU64::new(0)),
        }
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
        if let Ok(mut calls) = self.active_calls.write() {
            if let Some(call) = calls.get_mut(session_id) {
                call.participants.insert(peer_id.to_string());
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
        if let Ok(mut calls) = self.active_calls.write() {
            let entry = calls
                .entry(session_id.to_string())
                .or_insert_with(|| ActiveCall::new(session_id));
            entry.call_id = Some(call_id.to_string());
            entry.participants.insert(peer_id.to_string());
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
        if let Ok(mut calls) = self.active_calls.write() {
            if let Some(call) = calls.get_mut(session_id) {
                call.participants.remove(peer_id);
                if call.participants.is_empty() {
                    should_end = true;
                    calls.remove(session_id);
                }
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
        // Ignore send errors (no receivers is okay)
        let _ = self.event_tx.send(event);
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
        }
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

    if let Ok(json) = serde_json::to_string(&welcome) {
        if sender.send(Message::Text(json.into())).await.is_err() {
            state.unregister_peer(&peer_id);
            return;
        }
    }

    // Send peer ID assignment
    let peer_assigned = ServerMessage::PeerAssigned {
        peer_id: peer_id.clone(),
    };

    if let Ok(json) = serde_json::to_string(&peer_assigned) {
        if sender.send(Message::Text(json.into())).await.is_err() {
            state.unregister_peer(&peer_id);
            return;
        }
    }

    // Send initial scene state
    let scene_update = client.state.get_scene_update(client.session_id());
    if let Ok(json) = serde_json::to_string(&scene_update) {
        if sender.send(Message::Text(json.into())).await.is_err() {
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
    if let Ok(json) = serde_json::to_string(&call_message) {
        if sender.send(Message::Text(json.into())).await.is_err() {
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

/// Apply JSON changes to an element.
fn apply_changes_to_element(element: &mut Element, changes: &serde_json::Value) {
    if let Some(transform) = changes.get("transform") {
        if let Some(x) = transform.get("x").and_then(|v| v.as_f64()) {
            element.transform.x = x as f32;
        }
        if let Some(y) = transform.get("y").and_then(|v| v.as_f64()) {
            element.transform.y = y as f32;
        }
        if let Some(width) = transform.get("width").and_then(|v| v.as_f64()) {
            element.transform.width = width as f32;
        }
        if let Some(height) = transform.get("height").and_then(|v| v.as_f64()) {
            element.transform.height = height as f32;
        }
        if let Some(rotation) = transform.get("rotation").and_then(|v| v.as_f64()) {
            element.transform.rotation = rotation as f32;
        }
        if let Some(z_index) = transform.get("z_index").and_then(|v| v.as_i64()) {
            element.transform.z_index = z_index as i32;
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
}
