#![allow(dead_code)]
//! Client for interacting with the Communitas MCP HTTP endpoint.
//!
//! This adapter speaks JSON-RPC 2.0 to the Communitas MCP server (`/mcp`)
//! and exposes high-level helpers for canvas-specific operations.

use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, RwLock,
};
use std::time::Instant;

use crate::sync::{ServerMessage, SyncOrigin, SyncState};
use canvas_core::SceneDocument;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use thiserror::Error;
use tokio::task::JoinHandle;
use tracing::warn;
use url::Url;

const JSONRPC_VERSION: &str = "2.0";

/// Errors that can occur when talking to the Communitas MCP server.
#[derive(Debug, Error)]
pub enum CommunitasError {
    /// The MCP base URL provided by configuration is invalid.
    #[error("invalid Communitas MCP URL: {0}")]
    InvalidUrl(String),
    /// HTTP layer failed (connection, timeout, etc.).
    #[error("Communitas MCP HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    /// JSON parsing failed unexpectedly.
    #[error("failed to parse Communitas MCP payload: {0}")]
    Json(#[from] serde_json::Error),
    /// The server returned an RPC error.
    #[error("Communitas MCP RPC error {code}: {message}")]
    Rpc {
        /// Error code defined by MCP.
        code: i32,
        /// Human readable error message.
        message: String,
        /// Optional additional data payload.
        data: Option<Value>,
    },
    /// The RPC response did not match the expected structure.
    #[error("unexpected Communitas MCP response: {0}")]
    UnexpectedResponse(String),
}

impl CommunitasError {
    /// Returns true if this error is retryable (transient HTTP failures).
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::Http(_))
    }
}

/// Configuration for retry with exponential backoff.
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts.
    pub max_attempts: u32,
    /// Initial delay between retries in milliseconds.
    pub initial_delay_ms: u64,
    /// Maximum delay between retries in milliseconds.
    pub max_delay_ms: u64,
    /// Multiplier for exponential backoff.
    pub multiplier: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 5,
            initial_delay_ms: 100,
            max_delay_ms: 10_000,
            multiplier: 2.0,
        }
    }
}

impl RetryConfig {
    /// Create a new retry configuration with custom values.
    #[must_use]
    pub fn new(
        max_attempts: u32,
        initial_delay_ms: u64,
        max_delay_ms: u64,
        multiplier: f64,
    ) -> Self {
        Self {
            max_attempts,
            initial_delay_ms,
            max_delay_ms,
            multiplier,
        }
    }

    /// Calculate delay for a given attempt number (0-indexed).
    #[must_use]
    pub fn delay_for_attempt(&self, attempt: u32) -> u64 {
        let base_delay = self.initial_delay_ms as f64 * self.multiplier.powi(attempt as i32);
        let capped_delay = base_delay.min(self.max_delay_ms as f64) as u64;
        // Add jitter: random value between 0 and 25% of delay
        let jitter = (capped_delay / 4).max(1);
        capped_delay.saturating_add(jitter / 2)
    }
}

/// Configuration for periodic scene pulling from Communitas.
#[derive(Debug, Clone)]
pub struct PullConfig {
    /// Interval between pull attempts in seconds.
    pub interval_secs: u64,
    /// Whether periodic pulling is enabled.
    pub enabled: bool,
}

impl Default for PullConfig {
    fn default() -> Self {
        Self {
            interval_secs: 30,
            enabled: true,
        }
    }
}

impl PullConfig {
    /// Create a new pull configuration.
    #[must_use]
    pub fn new(interval_secs: u64, enabled: bool) -> Self {
        Self {
            interval_secs,
            enabled,
        }
    }

    /// Create a disabled pull configuration.
    #[must_use]
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            ..Self::default()
        }
    }
}

/// Minimal MCP client info descriptor.
#[derive(Clone, Debug)]
pub struct ClientDescriptor {
    /// Client name presented to the server.
    pub name: String,
    /// Client version string.
    pub version: String,
}

impl ClientDescriptor {
    fn into_protocol_info(self) -> ClientInfo {
        ClientInfo {
            name: self.name,
            version: self.version,
        }
    }
}

/// Asynchronous Communitas MCP client.
#[derive(Clone)]
pub struct CommunitasMcpClient {
    inner: Arc<InnerClient>,
}

struct InnerClient {
    http: Client,
    endpoint: Url,
    client_info: ClientInfo,
    request_id: AtomicU64,
    retry_config: RetryConfig,
}

impl CommunitasMcpClient {
    /// Create a new Communitas MCP client with default retry configuration.
    ///
    /// `base_url` may be either the MCP endpoint itself (`https://host:3040/mcp`)
    /// or just the host (in which case `/mcp` is appended automatically).
    pub fn new(
        base_url: impl AsRef<str>,
        descriptor: ClientDescriptor,
    ) -> Result<Self, CommunitasError> {
        Self::with_retry_config(base_url, descriptor, RetryConfig::default())
    }

    /// Create a new Communitas MCP client with custom retry configuration.
    ///
    /// # Errors
    ///
    /// Returns [`CommunitasError::InvalidUrl`] if the URL is malformed.
    /// Returns [`CommunitasError::Http`] if the HTTP client fails to build.
    pub fn with_retry_config(
        base_url: impl AsRef<str>,
        descriptor: ClientDescriptor,
        retry_config: RetryConfig,
    ) -> Result<Self, CommunitasError> {
        let mut url = Url::parse(base_url.as_ref())
            .map_err(|e| CommunitasError::InvalidUrl(e.to_string()))?;

        if url.path().is_empty() || url.path() == "/" {
            url.set_path("/mcp");
        }

        let http = Client::builder()
            .user_agent(format!("{} (saorsa-canvas)", descriptor.name.as_str()))
            .build()?;

        Ok(Self {
            inner: Arc::new(InnerClient {
                http,
                endpoint: url,
                client_info: descriptor.into_protocol_info(),
                request_id: AtomicU64::new(1),
                retry_config,
            }),
        })
    }

    /// Perform MCP initialize handshake.
    pub async fn initialize(&self) -> Result<InitializeResult, CommunitasError> {
        let params = InitializeParams {
            protocol_version: "2024-11-05".to_string(),
            capabilities: ClientCapabilities::default(),
            client_info: self.inner.client_info.clone(),
        };

        self.send_rpc("initialize", Some(serde_json::to_value(params)?))
            .await
    }

    /// Authenticate using a delegate token issued by Communitas.
    pub async fn authenticate_with_token(&self, token: &str) -> Result<(), CommunitasError> {
        self.call_tool("authenticate_token", Some(json!({ "token": token })))
            .await?;
        Ok(())
    }

    /// List available tools after initialization/authentication.
    pub async fn tools_list(&self) -> Result<ToolListResult, CommunitasError> {
        self.send_rpc("tools/list", None).await
    }

    /// Call an MCP tool with optional arguments.
    pub async fn call_tool(
        &self,
        name: &str,
        arguments: Option<Value>,
    ) -> Result<Value, CommunitasError> {
        let params = json!({
            "name": name,
            "arguments": arguments.unwrap_or_else(|| json!({}))
        });
        self.send_rpc::<Value>("tools/call", Some(params)).await
    }

    /// Fetch the canonical scene document for a session via Communitas.
    pub async fn fetch_scene(&self, session_id: &str) -> Result<SceneDocument, CommunitasError> {
        let response = self
            .call_tool(
                "canvas_get_scene",
                Some(json!({ "session_id": session_id })),
            )
            .await?;
        Self::deserialize_scene(&response)
    }

    /// Push the latest scene document upstream.
    pub async fn push_scene(&self, scene: &SceneDocument) -> Result<(), CommunitasError> {
        let response = self
            .call_tool(
                "canvas_update_scene",
                Some(json!({
                    "session_id": scene.session_id,
                    "scene": scene
                })),
            )
            .await?;

        Self::require_success_flag(&response, "push_scene")
    }

    /// Check that a response contains `"success": true`, returning an error otherwise.
    fn require_success_flag(response: &Value, operation: &str) -> Result<(), CommunitasError> {
        if response.get("success").and_then(Value::as_bool) == Some(true) {
            Ok(())
        } else {
            Err(CommunitasError::UnexpectedResponse(format!(
                "{operation} missing success flag: {response}"
            )))
        }
    }

    fn deserialize_scene(value: &Value) -> Result<SceneDocument, CommunitasError> {
        if let Some(scene) = value.get("scene") {
            return Ok(serde_json::from_value(scene.clone())?);
        }

        if let Some(content) = value.get("content").and_then(Value::as_array) {
            if let Some(first) = content.first() {
                if let Some(text) = first.get("text").and_then(Value::as_str) {
                    return serde_json::from_str(text).map_err(CommunitasError::from);
                }
            }
        }

        Err(CommunitasError::UnexpectedResponse(
            "response did not contain a scene document".to_string(),
        ))
    }

    /// Start the Communitas networking stack (saorsa-gossip over ant-quic).
    pub async fn network_start(&self, preferred_port: Option<u16>) -> Result<(), CommunitasError> {
        let mut args = serde_json::Map::new();
        if let Some(port) = preferred_port {
            args.insert("preferred_port".to_string(), Value::from(port));
        }

        let response = self
            .call_tool("network_start", Some(Value::Object(args)))
            .await?;

        Self::require_success_flag(&response, "network_start")
    }

    /// Start a WebRTC call associated with an entity/channel.
    pub async fn start_voice_call(
        &self,
        entity_id: &str,
        video_enabled: bool,
    ) -> Result<StartCallResult, CommunitasError> {
        let response = self
            .call_tool(
                "start_voice_call",
                Some(json!({
                    "entity_id": entity_id,
                    "video_enabled": video_enabled,
                })),
            )
            .await?;
        serde_json::from_value(response).map_err(CommunitasError::from)
    }

    /// Join an existing WebRTC call.
    pub async fn join_call(&self, call_id: &str) -> Result<CallAcknowledgeResult, CommunitasError> {
        let response = self
            .call_tool("join_call", Some(json!({ "call_id": call_id })))
            .await?;
        serde_json::from_value(response).map_err(CommunitasError::from)
    }

    /// End/leave a call.
    pub async fn end_call(&self, call_id: &str) -> Result<CallAcknowledgeResult, CommunitasError> {
        let response = self
            .call_tool("end_call", Some(json!({ "call_id": call_id })))
            .await?;
        serde_json::from_value(response).map_err(CommunitasError::from)
    }

    /// Toggle mute state.
    pub async fn toggle_mute(
        &self,
        call_id: &str,
        muted: bool,
    ) -> Result<ToggleMuteResult, CommunitasError> {
        let response = self
            .call_tool(
                "toggle_mute",
                Some(json!({
                    "call_id": call_id,
                    "muted": muted,
                })),
            )
            .await?;
        serde_json::from_value(response).map_err(CommunitasError::from)
    }

    /// Toggle outbound video.
    pub async fn toggle_video(
        &self,
        call_id: &str,
        enabled: bool,
    ) -> Result<ToggleVideoResult, CommunitasError> {
        let response = self
            .call_tool(
                "toggle_video",
                Some(json!({
                    "call_id": call_id,
                    "enabled": enabled,
                })),
            )
            .await?;
        serde_json::from_value(response).map_err(CommunitasError::from)
    }

    /// Start or stop screen sharing.
    pub async fn share_screen(
        &self,
        call_id: &str,
        enabled: bool,
    ) -> Result<ShareScreenResult, CommunitasError> {
        let response = self
            .call_tool(
                "share_screen",
                Some(json!({
                    "call_id": call_id,
                    "enabled": enabled,
                })),
            )
            .await?;
        serde_json::from_value(response).map_err(CommunitasError::from)
    }

    /// Fetch current call status snapshot.
    pub async fn get_call_status(
        &self,
        call_id: &str,
    ) -> Result<CallStatusResult, CommunitasError> {
        let response = self
            .call_tool("get_call_status", Some(json!({ "call_id": call_id })))
            .await?;
        serde_json::from_value(response).map_err(CommunitasError::from)
    }

    /// List current participants in a call.
    pub async fn get_call_participants(
        &self,
        call_id: &str,
    ) -> Result<CallParticipantsResult, CommunitasError> {
        let response = self
            .call_tool("get_call_participants", Some(json!({ "call_id": call_id })))
            .await?;
        serde_json::from_value(response).map_err(CommunitasError::from)
    }

    async fn send_rpc<T>(&self, method: &str, params: Option<Value>) -> Result<T, CommunitasError>
    where
        for<'de> T: Deserialize<'de>,
    {
        let id = self.inner.request_id.fetch_add(1, Ordering::Relaxed);
        let request = JsonRpcRequest {
            jsonrpc: JSONRPC_VERSION,
            id,
            method,
            params,
        };

        let config = &self.inner.retry_config;
        let mut last_error: Option<CommunitasError> = None;

        for attempt in 0..config.max_attempts {
            // Perform HTTP request
            let http_result = self
                .inner
                .http
                .post(self.inner.endpoint.clone())
                .json(&request)
                .send()
                .await;

            let response = match http_result {
                Ok(resp) => resp,
                Err(e) => {
                    let error = CommunitasError::Http(e);
                    if attempt + 1 < config.max_attempts {
                        let delay = config.delay_for_attempt(attempt);
                        warn!(
                            "Communitas RPC {} failed (attempt {}/{}), retrying in {}ms: {}",
                            method,
                            attempt + 1,
                            config.max_attempts,
                            delay,
                            error
                        );
                        tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
                        last_error = Some(error);
                        continue;
                    }
                    return Err(error);
                }
            };

            // Parse response - JSON errors are not retryable
            let rpc: JsonRpcResponse = match response.json().await {
                Ok(r) => r,
                Err(e) => {
                    let error = CommunitasError::Http(e);
                    if error.is_retryable() && attempt + 1 < config.max_attempts {
                        let delay = config.delay_for_attempt(attempt);
                        warn!(
                            "Communitas RPC {} response parse failed (attempt {}/{}), retrying in {}ms: {}",
                            method,
                            attempt + 1,
                            config.max_attempts,
                            delay,
                            error
                        );
                        tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
                        last_error = Some(error);
                        continue;
                    }
                    return Err(error);
                }
            };

            // RPC errors are not retryable
            if let Some(error) = rpc.error {
                return Err(CommunitasError::Rpc {
                    code: error.code,
                    message: error.message,
                    data: error.data,
                });
            }

            let result = rpc
                .result
                .ok_or_else(|| CommunitasError::UnexpectedResponse("missing result".into()))?;

            return Ok(serde_json::from_value(result)?);
        }

        // Should not reach here, but return last error if we do
        Err(last_error.unwrap_or_else(|| {
            CommunitasError::UnexpectedResponse("retry loop exited without result".into())
        }))
    }
}

#[derive(Debug, Clone, Serialize)]
struct JsonRpcRequest<'a> {
    jsonrpc: &'a str,
    id: u64,
    method: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct JsonRpcResponse {
    #[allow(dead_code)]
    jsonrpc: String,
    #[serde(default)]
    result: Option<Value>,
    #[serde(default)]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Deserialize)]
struct JsonRpcError {
    code: i32,
    message: String,
    #[serde(default)]
    data: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct InitializeParams {
    protocol_version: String,
    capabilities: ClientCapabilities,
    client_info: ClientInfo,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ClientCapabilities {}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ClientInfo {
    name: String,
    version: String,
}

/// Initialize result returned by Communitas MCP.
#[derive(Debug, Clone, Deserialize)]
pub struct InitializeResult {
    pub protocol_version: String,
    pub capabilities: ServerCapabilities,
    pub server_info: ServerInfo,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerCapabilities {
    #[serde(default)]
    pub tools: Option<ToolsCapability>,
    #[serde(default)]
    pub resources: Option<ResourcesCapability>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ToolsCapability {
    #[serde(default)]
    pub list_changed: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResourcesCapability {
    #[serde(default)]
    pub subscribe: bool,
    #[serde(default)]
    pub list_changed: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerInfo {
    pub name: String,
    pub version: String,
}

/// Tool list result wrapper.
#[derive(Debug, Clone, Deserialize)]
pub struct ToolListResult {
    pub tools: Vec<ToolInfo>,
}

/// Basic tool metadata.
#[derive(Debug, Clone, Deserialize)]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub input_schema: Value,
}

/// Result payload for `start_voice_call`.
#[derive(Debug, Clone, Deserialize)]
pub struct StartCallResult {
    /// Unique call identifier assigned by Communitas.
    pub call_id: String,
    /// Entity/channel associated with the call.
    pub entity_id: String,
    /// Indicates success (Communitas sets this true on success).
    #[serde(default)]
    pub success: bool,
}

/// Result payload for `join_call` / `end_call`.
#[derive(Debug, Clone, Deserialize)]
pub struct CallAcknowledgeResult {
    /// Target call identifier.
    pub call_id: String,
    /// Whether the operation succeeded.
    #[serde(default)]
    pub success: bool,
}

/// Result payload for toggle mute operations.
#[derive(Debug, Clone, Deserialize)]
pub struct ToggleMuteResult {
    /// Call identifier.
    pub call_id: String,
    /// Whether the participant is muted after the operation.
    pub muted: bool,
}

/// Result payload for toggle video operations.
#[derive(Debug, Clone, Deserialize)]
pub struct ToggleVideoResult {
    /// Call identifier.
    pub call_id: String,
    /// Whether video is enabled after the toggle.
    #[serde(rename = "video_enabled")]
    pub video_enabled: bool,
}

/// Result payload for screen sharing toggles.
#[derive(Debug, Clone, Deserialize)]
pub struct ShareScreenResult {
    /// Call identifier.
    pub call_id: String,
    /// Whether the toggle succeeded.
    #[serde(default)]
    pub success: bool,
    /// Human-readable description ("started"/"stopped").
    #[serde(default)]
    pub screen_share: Option<String>,
}

/// Snapshot of call state returned by `get_call_status`.
#[derive(Debug, Clone, Deserialize)]
pub struct CallStatusResult {
    /// Call identifier.
    pub call_id: String,
    /// Entity/channel identifier.
    pub entity_id: String,
    /// Number of participants currently in the call.
    pub participant_count: usize,
    /// Unix timestamp when the call started (seconds).
    pub started_at: i64,
    /// Whether the local participant is muted.
    pub is_muted: bool,
    /// Whether video is enabled.
    pub is_video_enabled: bool,
    /// Whether screen sharing is active.
    pub is_screen_sharing: bool,
}

/// Participant list result from `get_call_participants`.
#[derive(Debug, Clone, Deserialize)]
pub struct CallParticipantsResult {
    /// Participant identities (four-word addresses).
    pub participants: Vec<String>,
}

/// Connection health state for the Communitas bridge.
#[derive(Debug, Clone, Default)]
pub enum ConnectionState {
    /// Connected and healthy.
    #[default]
    Connected,
    /// Disconnected from Communitas.
    Disconnected {
        /// When the disconnection was detected.
        since: Instant,
        /// Reason for disconnection.
        reason: String,
    },
    /// Attempting to reconnect.
    Reconnecting {
        /// Current reconnection attempt number.
        attempt: u32,
    },
}

/// Handle to a running Communitas bridge with health tracking.
pub struct BridgeHandle {
    push_handle: JoinHandle<()>,
    pull_handle: Option<JoinHandle<()>>,
    state: Arc<RwLock<ConnectionState>>,
    push_shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
    pull_shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

impl BridgeHandle {
    /// Get the current connection state.
    ///
    /// # Panics
    ///
    /// Panics if the state lock is poisoned.
    #[must_use]
    pub fn state(&self) -> ConnectionState {
        self.state
            .read()
            .map(|guard| guard.clone())
            .unwrap_or_else(|_| ConnectionState::Disconnected {
                since: Instant::now(),
                reason: "lock poisoned".into(),
            })
    }

    /// Check if the bridge is connected.
    #[must_use]
    pub fn is_connected(&self) -> bool {
        matches!(self.state(), ConnectionState::Connected)
    }

    /// Gracefully shut down the bridge (both push and pull tasks).
    pub fn shutdown(mut self) {
        // Signal push task to stop
        if let Some(tx) = self.push_shutdown_tx.take() {
            let _ = tx.send(());
        }
        // Signal pull task to stop
        if let Some(tx) = self.pull_shutdown_tx.take() {
            let _ = tx.send(());
        }
        // Abort handles as backup
        self.push_handle.abort();
        if let Some(handle) = self.pull_handle {
            handle.abort();
        }
    }

    /// Abort the bridge without graceful shutdown.
    pub fn abort(self) {
        self.push_handle.abort();
        if let Some(handle) = self.pull_handle {
            handle.abort();
        }
    }
}

/// Spawn a task that watches local sync events and mirrors them to Communitas.
///
/// Returns a [`BridgeHandle`] for monitoring connection health and shutdown.
pub fn spawn_scene_bridge(sync: SyncState, client: CommunitasMcpClient) -> BridgeHandle {
    let state = Arc::new(RwLock::new(ConnectionState::Connected));
    let state_clone = Arc::clone(&state);
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel();
    let retry_config = client.inner.retry_config.clone();
    let mut rx = sync.subscribe();

    let push_handle = tokio::spawn(async move {
        let mut consecutive_failures: u32 = 0;

        loop {
            tokio::select! {
                // Check for shutdown signal
                _ = &mut shutdown_rx => {
                    tracing::info!("Communitas bridge received shutdown signal");
                    break;
                }

                // Process sync events
                event_result = rx.recv() => {
                    let event = match event_result {
                        Ok(e) => e,
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            tracing::info!("Communitas bridge: sync channel closed");
                            break;
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            warn!("Communitas bridge lagged by {} messages", n);
                            continue;
                        }
                    };

                    if event.origin != SyncOrigin::Local {
                        continue;
                    }

                    if let ServerMessage::SceneUpdate { scene } = event.message {
                        // Update state to reconnecting if we had failures
                        if consecutive_failures > 0 {
                            if let Ok(mut guard) = state_clone.write() {
                                *guard = ConnectionState::Reconnecting {
                                    attempt: consecutive_failures,
                                };
                            }
                        }

                        match client.push_scene(&scene).await {
                            Ok(()) => {
                                // Success - reset failure count and mark connected
                                if consecutive_failures > 0 {
                                    tracing::info!(
                                        "Communitas bridge reconnected after {} failures",
                                        consecutive_failures
                                    );
                                    consecutive_failures = 0;
                                    if let Ok(mut guard) = state_clone.write() {
                                        *guard = ConnectionState::Connected;
                                    }
                                }
                            }
                            Err(err) => {
                                consecutive_failures = consecutive_failures.saturating_add(1);

                                // Mark as disconnected
                                if let Ok(mut guard) = state_clone.write() {
                                    *guard = ConnectionState::Disconnected {
                                        since: Instant::now(),
                                        reason: err.to_string(),
                                    };
                                }

                                warn!(
                                    "Failed to push scene to Communitas (failure #{}): {}",
                                    consecutive_failures, err
                                );

                                // Apply backoff before next attempt
                                if consecutive_failures < retry_config.max_attempts {
                                    let delay = retry_config.delay_for_attempt(consecutive_failures - 1);
                                    tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
                                }
                            }
                        }
                    }
                }
            }
        }
    });

    BridgeHandle {
        push_handle,
        pull_handle: None,
        state,
        push_shutdown_tx: Some(shutdown_tx),
        pull_shutdown_tx: None,
    }
}

/// Spawn a task that periodically fetches scenes from Communitas.
///
/// This task polls the remote server at the configured interval and applies
/// any changes that have a newer timestamp than the local version.
///
/// # Arguments
///
/// * `sync` - The sync state to update with remote changes
/// * `client` - The Communitas MCP client
/// * `config` - Pull configuration (interval, enabled)
/// * `session_ids` - List of session IDs to fetch
///
/// Returns a tuple of (JoinHandle, shutdown sender).
fn spawn_scene_pull(
    sync: SyncState,
    client: CommunitasMcpClient,
    config: PullConfig,
    session_ids: Vec<String>,
) -> (JoinHandle<()>, tokio::sync::oneshot::Sender<()>) {
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel();

    let handle = tokio::spawn(async move {
        if !config.enabled {
            tracing::debug!("Communitas pull disabled, task exiting");
            return;
        }

        let interval = tokio::time::Duration::from_secs(config.interval_secs);
        let mut ticker = tokio::time::interval(interval);
        // Don't fire immediately on start
        ticker.tick().await;

        loop {
            tokio::select! {
                _ = &mut shutdown_rx => {
                    tracing::info!("Communitas pull received shutdown signal");
                    break;
                }

                _ = ticker.tick() => {
                    for session_id in &session_ids {
                        match client.fetch_scene(session_id).await {
                            Ok(remote_doc) => {
                                // Get local timestamp for comparison
                                let local_doc = sync.scene_document(session_id);

                                if remote_doc.timestamp > local_doc.timestamp {
                                    tracing::debug!(
                                        session_id = %session_id,
                                        remote_ts = remote_doc.timestamp,
                                        local_ts = local_doc.timestamp,
                                        "Applying newer remote scene"
                                    );

                                    match remote_doc.into_scene() {
                                        Ok(scene) => {
                                            if let Err(e) = sync.replace_scene(
                                                session_id,
                                                scene,
                                                SyncOrigin::Remote,
                                            ) {
                                                warn!(
                                                    session_id = %session_id,
                                                    error = %e,
                                                    "Failed to apply remote scene"
                                                );
                                            }
                                        }
                                        Err(e) => {
                                            warn!(
                                                session_id = %session_id,
                                                error = %e,
                                                "Failed to parse remote scene"
                                            );
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                // Log but continue - don't break the loop on fetch errors
                                warn!(
                                    session_id = %session_id,
                                    error = %e,
                                    "Failed to fetch scene from Communitas"
                                );
                            }
                        }
                    }
                }
            }
        }
    });

    (handle, shutdown_tx)
}

/// Spawn a full bidirectional Communitas bridge with push and pull.
///
/// This creates both:
/// - A push task that watches local changes and mirrors them upstream
/// - A pull task that periodically fetches remote changes
///
/// # Arguments
///
/// * `sync` - The sync state for bidirectional updates
/// * `client` - The Communitas MCP client
/// * `pull_config` - Configuration for periodic pulling
/// * `session_ids` - Session IDs to sync (for pulling)
pub fn spawn_full_bridge(
    sync: SyncState,
    client: CommunitasMcpClient,
    pull_config: PullConfig,
    session_ids: Vec<String>,
) -> BridgeHandle {
    // Create push bridge first
    let mut handle = spawn_scene_bridge(sync.clone(), client.clone());

    // Add pull task if enabled
    if pull_config.enabled {
        let (pull_handle, pull_shutdown_tx) =
            spawn_scene_pull(sync, client, pull_config, session_ids);
        handle.pull_handle = Some(pull_handle);
        handle.pull_shutdown_tx = Some(pull_shutdown_tx);
    }

    handle
}

#[cfg(test)]
mod tests {
    use super::*;
    use canvas_core::{ElementDocument, ElementKind, Scene, Transform, ViewportDocument};
    use tokio::time::{sleep, Duration};
    use wiremock::matchers::{body_json, body_string_contains, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    // =========================================================================
    // Unit tests that don't require network/wiremock

    #[test]
    fn test_communitas_error_is_retryable() {
        // RPC errors should not be retryable
        let rpc_err = CommunitasError::Rpc {
            code: -32000,
            message: "test error".into(),
            data: None,
        };
        assert!(!rpc_err.is_retryable());

        let url_err = CommunitasError::InvalidUrl("bad url".into());
        assert!(!url_err.is_retryable());

        let response_err = CommunitasError::UnexpectedResponse("bad response".into());
        assert!(!response_err.is_retryable());
    }

    // =========================================================================

    #[test]
    fn test_retry_config_defaults() {
        let config = RetryConfig::default();
        assert_eq!(config.max_attempts, 5);
        assert_eq!(config.initial_delay_ms, 100);
        assert_eq!(config.max_delay_ms, 10_000);
    }

    #[test]
    fn test_client_descriptor_creation() {
        let desc = ClientDescriptor {
            name: "test".into(),
            version: "1.0.0".into(),
        };
        assert_eq!(desc.name, "test");
        assert_eq!(desc.version, "1.0.0");
    }

    #[test]
    fn test_invalid_url_error() {
        let result = CommunitasMcpClient::new(
            "not-a-valid-url",
            ClientDescriptor {
                name: "test".into(),
                version: "1.0".into(),
            },
        );
        assert!(result.is_err());
        let err = result.err().expect("expected error");
        match err {
            CommunitasError::InvalidUrl(_) => {}
            other => panic!("Expected InvalidUrl error, got: {:?}", other),
        }
    }

    #[test]
    fn test_scene_document_sample() {
        let doc = sample_scene();
        assert_eq!(doc.session_id, "default");
        assert_eq!(doc.elements.len(), 1);
        assert_eq!(doc.viewport.width, 800.0);
    }

    fn sample_scene() -> SceneDocument {
        SceneDocument {
            session_id: "default".to_string(),
            viewport: ViewportDocument {
                width: 800.0,
                height: 600.0,
                zoom: 1.0,
                pan_x: 0.0,
                pan_y: 0.0,
            },
            elements: vec![ElementDocument {
                id: "abc".to_string(),
                kind: ElementKind::Text {
                    content: "Hello".into(),
                    font_size: 18.0,
                    color: "#000000".into(),
                },
                transform: Transform::default(),
                interactive: true,
                selected: false,
            }],
            timestamp: 42,
        }
    }

    async fn client_with_mock(server: &MockServer) -> CommunitasMcpClient {
        CommunitasMcpClient::new(
            server.uri(),
            ClientDescriptor {
                name: "test-client".into(),
                version: "0.0.1".into(),
            },
        )
        .expect("client")
    }

    #[tokio::test]
    #[cfg_attr(
        target_os = "macos",
        ignore = "wiremock/reqwest system-configuration issue on macOS"
    )]
    async fn initialize_success() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/mcp"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": {
                    "protocol_version": "2024-11-05",
                    "capabilities": {
                        "tools": { "list_changed": false },
                        "resources": { "list_changed": false, "subscribe": false }
                    },
                    "server_info": { "name": "mock", "version": "1.2.3" }
                }
            })))
            .mount(&server)
            .await;

        let client = client_with_mock(&server).await;
        let result = client.initialize().await.expect("result");
        assert_eq!(result.server_info.name, "mock");
    }

    #[tokio::test]
    #[cfg_attr(
        target_os = "macos",
        ignore = "wiremock/reqwest system-configuration issue on macOS"
    )]
    async fn call_tool_propagates_error() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/mcp"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "jsonrpc": "2.0",
                "id": 2,
                "error": {
                    "code": -32000,
                    "message": "boom"
                }
            })))
            .mount(&server)
            .await;

        let client = client_with_mock(&server).await;
        let err = client
            .call_tool("canvas_get_scene", None)
            .await
            .unwrap_err();
        match err {
            CommunitasError::Rpc { code, .. } => assert_eq!(code, -32000),
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[tokio::test]
    #[cfg_attr(
        target_os = "macos",
        ignore = "wiremock/reqwest system-configuration issue on macOS"
    )]
    async fn fetch_scene_parses_scene_document() {
        let server = MockServer::start().await;
        let scene = sample_scene();

        Mock::given(method("POST"))
            .and(path("/mcp"))
            .and(body_json(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/call",
                "params": {
                    "name": "canvas_get_scene",
                    "arguments": {"session_id": "default"}
                }
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": {
                    "scene": scene
                }
            })))
            .mount(&server)
            .await;

        let client = client_with_mock(&server).await;
        let fetched = client.fetch_scene("default").await.expect("scene");
        assert_eq!(fetched.session_id, scene.session_id);
        assert_eq!(fetched.elements.len(), 1);
    }

    #[tokio::test]
    #[cfg_attr(
        target_os = "macos",
        ignore = "wiremock/reqwest system-configuration issue on macOS"
    )]
    async fn authenticate_with_token_sends_request() {
        let server = MockServer::start().await;

        let _mock = Mock::given(method("POST"))
            .and(path("/mcp"))
            .and(body_string_contains("authenticate_token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": { "success": true }
            })))
            .expect(1)
            .mount_as_scoped(&server)
            .await;

        let client = client_with_mock(&server).await;
        client
            .authenticate_with_token("delegate-token")
            .await
            .expect("auth");
    }

    #[tokio::test]
    #[cfg_attr(
        target_os = "macos",
        ignore = "wiremock/reqwest system-configuration issue on macOS"
    )]
    async fn bridge_pushes_local_updates() {
        let server = MockServer::start().await;

        let client = client_with_mock(&server).await;
        let sync = SyncState::new();
        let handle = spawn_scene_bridge(sync.clone(), client);

        sync.replace_scene("default", Scene::new(800.0, 600.0), SyncOrigin::Local)
            .expect("replace scene");

        sleep(Duration::from_millis(300)).await;
        let requests = server.received_requests().await.expect("requests");
        assert_eq!(requests.len(), 1);
        let body_text = std::str::from_utf8(&requests[0].body).unwrap_or_default();
        assert!(body_text.contains("canvas_update_scene"));
        handle.abort();
    }

    #[tokio::test]
    #[cfg_attr(
        target_os = "macos",
        ignore = "wiremock/reqwest system-configuration issue on macOS"
    )]
    async fn bridge_ignores_remote_events() {
        let server = MockServer::start().await;
        let client = client_with_mock(&server).await;
        let sync = SyncState::new();
        let handle = spawn_scene_bridge(sync.clone(), client);

        sync.replace_scene("default", Scene::new(800.0, 600.0), SyncOrigin::Remote)
            .expect("replace scene");

        sleep(Duration::from_millis(50)).await;
        let requests = server.received_requests().await.expect("requests");
        assert!(requests.is_empty());
        handle.abort();
    }
}
