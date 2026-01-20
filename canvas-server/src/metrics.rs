//! Prometheus metrics for canvas-server.
//!
//! Provides metrics collection and a Prometheus-compatible `/metrics` endpoint.

use metrics::{counter, gauge, histogram};
use metrics_exporter_prometheus::{BuildError, PrometheusBuilder, PrometheusHandle};

// Metric names as constants for consistency
const HTTP_REQUESTS_TOTAL: &str = "canvas_http_requests_total";
const HTTP_REQUEST_DURATION: &str = "canvas_http_request_duration_seconds";
const WS_CONNECTIONS_ACTIVE: &str = "canvas_ws_connections_active";
const WS_MESSAGES_TOTAL: &str = "canvas_ws_messages_total";
const SCENE_ELEMENTS_TOTAL: &str = "canvas_scene_elements_total";
const MCP_TOOL_CALLS_TOTAL: &str = "canvas_mcp_tool_calls_total";
const SIGNALING_MESSAGES_TOTAL: &str = "canvas_signaling_messages_total";
const VALIDATION_FAILURES_TOTAL: &str = "canvas_validation_failures_total";
const RATE_LIMITED_TOTAL: &str = "canvas_rate_limited_total";
const COMMUNITAS_NETWORK_STATE: &str = "canvas_communitas_network_state";
const COMMUNITAS_RETRY_ATTEMPTS: &str = "canvas_communitas_retry_attempts_total";

/// Initialize metrics and return the Prometheus handle.
///
/// # Errors
///
/// Returns an error if the Prometheus recorder cannot be installed
/// (e.g., if another recorder is already installed).
pub fn init_metrics() -> Result<PrometheusHandle, BuildError> {
    PrometheusBuilder::new().install_recorder()
}

/// Record an HTTP request.
///
/// # Arguments
///
/// * `method` - HTTP method (GET, POST, etc.)
/// * `path` - Request path
/// * `status` - HTTP status code
/// * `duration_secs` - Request duration in seconds
pub fn record_http_request(method: &str, path: &str, status: u16, duration_secs: f64) {
    counter!(
        HTTP_REQUESTS_TOTAL,
        "method" => method.to_string(),
        "path" => path.to_string(),
        "status" => status.to_string()
    )
    .increment(1);
    histogram!(
        HTTP_REQUEST_DURATION,
        "method" => method.to_string(),
        "path" => path.to_string()
    )
    .record(duration_secs);
}

/// Update active WebSocket connection count.
pub fn set_ws_connections(count: usize) {
    gauge!(WS_CONNECTIONS_ACTIVE).set(count as f64);
}

/// Increment active WebSocket connections.
pub fn inc_ws_connections() {
    gauge!(WS_CONNECTIONS_ACTIVE).increment(1.0);
}

/// Decrement active WebSocket connections.
pub fn dec_ws_connections() {
    gauge!(WS_CONNECTIONS_ACTIVE).decrement(1.0);
}

/// Record a WebSocket message.
///
/// # Arguments
///
/// * `direction` - "inbound" or "outbound"
/// * `msg_type` - Message type (e.g., "subscribe", "ping", "scene_update")
pub fn record_ws_message(direction: &str, msg_type: &str) {
    counter!(
        WS_MESSAGES_TOTAL,
        "direction" => direction.to_string(),
        "type" => msg_type.to_string()
    )
    .increment(1);
}

/// Update scene element count.
pub fn set_scene_elements(count: usize) {
    gauge!(SCENE_ELEMENTS_TOTAL).set(count as f64);
}

/// Record an MCP tool call.
///
/// # Arguments
///
/// * `tool_name` - Name of the tool called
/// * `success` - Whether the call succeeded
pub fn record_mcp_tool_call(tool_name: &str, success: bool) {
    counter!(
        MCP_TOOL_CALLS_TOTAL,
        "tool" => tool_name.to_string(),
        "success" => success.to_string()
    )
    .increment(1);
}

/// Record a signaling message (WebRTC).
///
/// # Arguments
///
/// * `msg_type` - Signaling message type (offer, answer, ice_candidate, etc.)
pub fn record_signaling_message(msg_type: &str) {
    counter!(
        SIGNALING_MESSAGES_TOTAL,
        "type" => msg_type.to_string()
    )
    .increment(1);
}

/// Record an input validation failure.
///
/// # Arguments
///
/// * `validation_type` - Type of validation that failed (session_id, element_id, peer_id, sdp, etc.)
pub fn record_validation_failure(validation_type: &str) {
    counter!(
        VALIDATION_FAILURES_TOTAL,
        "type" => validation_type.to_string()
    )
    .increment(1);
}

/// Record a rate-limited request.
///
/// # Arguments
///
/// * `source` - Source of the rate-limited request (websocket, http, etc.)
pub fn record_rate_limited(source: &str) {
    counter!(
        RATE_LIMITED_TOTAL,
        "source" => source.to_string()
    )
    .increment(1);
}

/// Update Communitas network connection state.
///
/// # Arguments
///
/// * `state` - Connection state: "connected", "disconnected", "retrying"
pub fn set_communitas_network_state(state: &str) {
    // Use gauge with different labels for each state
    // Reset all states to 0, then set current to 1
    gauge!(COMMUNITAS_NETWORK_STATE, "state" => "connected").set(0.0);
    gauge!(COMMUNITAS_NETWORK_STATE, "state" => "disconnected").set(0.0);
    gauge!(COMMUNITAS_NETWORK_STATE, "state" => "retrying").set(0.0);
    gauge!(COMMUNITAS_NETWORK_STATE, "state" => state.to_string()).set(1.0);
}

/// Record a Communitas network retry attempt.
///
/// # Arguments
///
/// * `outcome` - "success" or "failure"
pub fn record_communitas_retry(outcome: &str) {
    counter!(
        COMMUNITAS_RETRY_ATTEMPTS,
        "outcome" => outcome.to_string()
    )
    .increment(1);
}

#[cfg(test)]
mod tests {
    // Note: Testing actual metrics values requires a test recorder.
    // These tests are placeholders for documentation purposes.
    // The metrics functions themselves don't panic when no recorder is set.

    #[test]
    fn test_metrics_module_exists() {
        // Verify the metrics module compiles correctly
        assert!(true);
    }
}
