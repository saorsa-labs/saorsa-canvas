# Phase 4.1: Observability

> Goal: Add structured tracing and metrics for production monitoring and debugging.

## Prerequisites

- [x] M3 complete (WebRTC & Media Integration)
- [x] canvas-server running with WebSocket and HTTP endpoints
- [x] Tracing crate already in use for logging

## Overview

This phase adds production-grade observability:

1. **Structured Tracing** - Replace println! with spans, add request tracing
2. **Metrics Collection** - Prometheus-compatible metrics endpoint
3. **Health Checks** - Liveness and readiness probes

Architecture:
```
                    ┌─────────────────────┐
                    │   canvas-server     │
                    │                     │
 HTTP Request ──────┼──► TraceLayer       │
                    │    (spans, timing)  │
                    │                     │
 WebSocket ─────────┼──► Connection spans │
                    │    (session, peer)  │
                    │                     │
 MCP Calls ─────────┼──► Tool invocation  │
                    │    spans            │
                    │                     │
                    └─────────┬───────────┘
                              │
              ┌───────────────┼───────────────┐
              ▼               ▼               ▼
         /metrics        /health         JSON logs
         (Prometheus)    (k8s probes)    (stdout)
```

---

<task type="auto" priority="p1">
  <n>Add structured tracing with tower-http</n>
  <files>
    canvas-server/Cargo.toml,
    canvas-server/src/main.rs,
    canvas-server/src/routes.rs
  </files>
  <action>
    Add request tracing middleware and structured logging:

    1. Add dependencies to canvas-server/Cargo.toml:
       ```toml
       tower-http = { version = "0.6", features = ["trace", "request-id", "cors"] }
       tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
       uuid = { workspace = true }
       ```

    2. Update main.rs to configure structured logging:
       ```rust
       use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

       fn init_tracing() {
           let filter = EnvFilter::try_from_default_env()
               .unwrap_or_else(|_| EnvFilter::new("info,canvas_server=debug,tower_http=debug"));

           let fmt_layer = tracing_subscriber::fmt::layer()
               .with_target(true)
               .with_thread_ids(false)
               .with_file(true)
               .with_line_number(true);

           // Use JSON format in production (RUST_LOG_FORMAT=json)
           if std::env::var("RUST_LOG_FORMAT").as_deref() == Ok("json") {
               tracing_subscriber::registry()
                   .with(filter)
                   .with(fmt_layer.json())
                   .init();
           } else {
               tracing_subscriber::registry()
                   .with(filter)
                   .with(fmt_layer)
                   .init();
           }
       }
       ```

    3. Add TraceLayer to router in routes.rs:
       ```rust
       use tower_http::trace::{TraceLayer, DefaultMakeSpan, DefaultOnRequest, DefaultOnResponse};
       use tracing::Level;

       pub fn create_router(state: AppState) -> Router {
           Router::new()
               // ... existing routes ...
               .layer(
                   TraceLayer::new_for_http()
                       .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
                       .on_request(DefaultOnRequest::new().level(Level::INFO))
                       .on_response(DefaultOnResponse::new().level(Level::INFO))
               )
               .with_state(state)
       }
       ```

    4. Add request ID middleware for correlation:
       ```rust
       use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
       use http::header::HeaderName;

       const REQUEST_ID_HEADER: HeaderName = HeaderName::from_static("x-request-id");

       // Add to router layers:
       .layer(SetRequestIdLayer::new(REQUEST_ID_HEADER.clone(), MakeRequestUuid))
       .layer(PropagateRequestIdLayer::new(REQUEST_ID_HEADER))
       ```

    5. Add spans to WebSocket handler:
       ```rust
       #[tracing::instrument(skip(ws, state), fields(session_id = %session_id))]
       async fn handle_websocket(
           ws: WebSocketUpgrade,
           Path(session_id): Path<String>,
           State(state): State<AppState>,
       ) -> impl IntoResponse {
           // ...
       }
       ```

    6. Add spans to key operations:
       - Scene mutations: `#[tracing::instrument(skip(scene), fields(element_count))]`
       - MCP tool calls: `#[tracing::instrument(skip(state), fields(tool_name))]`
       - Signaling messages: `#[tracing::instrument(fields(msg_type, peer_id))]`
  </action>
  <verify>
    cargo fmt --all -- --check
    cargo clippy -p canvas-server --all-features -- -D warnings
    cargo test -p canvas-server
    # Manual: Run server and verify structured logs appear
    RUST_LOG=debug cargo run -p canvas-server
  </verify>
  <done>
    - TraceLayer adds request spans with timing
    - Request IDs propagate through headers
    - WebSocket connections have session spans
    - JSON log format available via RUST_LOG_FORMAT=json
    - All handlers have appropriate tracing::instrument
  </done>
</task>

---

<task type="auto" priority="p1">
  <n>Add Prometheus metrics endpoint</n>
  <files>
    canvas-server/Cargo.toml,
    canvas-server/src/main.rs,
    canvas-server/src/routes.rs,
    canvas-server/src/metrics.rs
  </files>
  <action>
    Add metrics collection and Prometheus endpoint:

    1. Add dependencies to canvas-server/Cargo.toml:
       ```toml
       metrics = "0.24"
       metrics-exporter-prometheus = "0.16"
       ```

    2. Create canvas-server/src/metrics.rs:
       ```rust
       //! Prometheus metrics for canvas-server.

       use metrics::{counter, gauge, histogram};
       use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};

       /// Initialize metrics and return the Prometheus handle.
       pub fn init_metrics() -> PrometheusHandle {
           PrometheusBuilder::new()
               .install_recorder()
               .expect("Failed to install Prometheus recorder")
       }

       // Metric names as constants
       pub const HTTP_REQUESTS_TOTAL: &str = "canvas_http_requests_total";
       pub const HTTP_REQUEST_DURATION: &str = "canvas_http_request_duration_seconds";
       pub const WS_CONNECTIONS_ACTIVE: &str = "canvas_ws_connections_active";
       pub const WS_MESSAGES_TOTAL: &str = "canvas_ws_messages_total";
       pub const SCENE_ELEMENTS_TOTAL: &str = "canvas_scene_elements_total";
       pub const MCP_TOOL_CALLS_TOTAL: &str = "canvas_mcp_tool_calls_total";
       pub const SIGNALING_MESSAGES_TOTAL: &str = "canvas_signaling_messages_total";

       /// Record an HTTP request.
       pub fn record_http_request(method: &str, path: &str, status: u16, duration_secs: f64) {
           counter!(HTTP_REQUESTS_TOTAL, "method" => method.to_string(), "path" => path.to_string(), "status" => status.to_string()).increment(1);
           histogram!(HTTP_REQUEST_DURATION, "method" => method.to_string(), "path" => path.to_string()).record(duration_secs);
       }

       /// Update active WebSocket connection count.
       pub fn set_ws_connections(count: usize) {
           gauge!(WS_CONNECTIONS_ACTIVE).set(count as f64);
       }

       /// Record a WebSocket message.
       pub fn record_ws_message(direction: &str, msg_type: &str) {
           counter!(WS_MESSAGES_TOTAL, "direction" => direction.to_string(), "type" => msg_type.to_string()).increment(1);
       }

       /// Update scene element count.
       pub fn set_scene_elements(count: usize) {
           gauge!(SCENE_ELEMENTS_TOTAL).set(count as f64);
       }

       /// Record an MCP tool call.
       pub fn record_mcp_tool_call(tool_name: &str, success: bool) {
           counter!(MCP_TOOL_CALLS_TOTAL, "tool" => tool_name.to_string(), "success" => success.to_string()).increment(1);
       }

       /// Record a signaling message.
       pub fn record_signaling_message(msg_type: &str) {
           counter!(SIGNALING_MESSAGES_TOTAL, "type" => msg_type.to_string()).increment(1);
       }
       ```

    3. Add metrics endpoint to routes.rs:
       ```rust
       use metrics_exporter_prometheus::PrometheusHandle;

       async fn metrics_handler(State(handle): State<PrometheusHandle>) -> String {
           handle.render()
       }

       // Add to router:
       .route("/metrics", get(metrics_handler))
       ```

    4. Initialize metrics in main.rs and pass handle to router

    5. Instrument key code paths:
       - WebSocket connect/disconnect: update WS_CONNECTIONS_ACTIVE
       - WebSocket message receive: record_ws_message("inbound", msg_type)
       - Scene mutations: set_scene_elements(count)
       - MCP tool calls: record_mcp_tool_call(tool, success)
       - Signaling: record_signaling_message(type)

    6. Add metrics middleware for HTTP requests (or use tower-http metrics layer)
  </action>
  <verify>
    cargo fmt --all -- --check
    cargo clippy -p canvas-server --all-features -- -D warnings
    cargo test -p canvas-server
    # Manual: curl http://localhost:9473/metrics
  </verify>
  <done>
    - /metrics endpoint returns Prometheus format
    - HTTP request count and latency tracked
    - WebSocket connection gauge
    - Scene element gauge
    - MCP tool call counter
    - Signaling message counter
  </done>
</task>

---

<task type="auto" priority="p1">
  <n>Add health check endpoints</n>
  <files>
    canvas-server/src/routes.rs,
    canvas-server/src/health.rs
  </files>
  <action>
    Add Kubernetes-compatible health probes:

    1. Create canvas-server/src/health.rs:
       ```rust
       //! Health check endpoints for Kubernetes probes.

       use axum::{extract::State, http::StatusCode, Json};
       use serde::Serialize;

       use crate::AppState;

       /// Health status response.
       #[derive(Serialize)]
       pub struct HealthStatus {
           /// Overall status: "healthy" or "unhealthy"
           pub status: &'static str,
           /// Server version
           pub version: &'static str,
           /// Individual component checks
           pub checks: HealthChecks,
       }

       /// Individual health checks.
       #[derive(Serialize)]
       pub struct HealthChecks {
           /// Scene store accessible
           pub scene_store: bool,
           /// WebSocket handler ready
           pub websocket: bool,
       }

       /// Liveness probe - is the server running?
       /// Returns 200 if the process is alive.
       pub async fn liveness() -> StatusCode {
           StatusCode::OK
       }

       /// Readiness probe - is the server ready to accept traffic?
       /// Checks that all dependencies are available.
       pub async fn readiness(State(state): State<AppState>) -> (StatusCode, Json<HealthStatus>) {
           let scene_ok = state.scene_store.read().is_ok();
           let ws_ok = true; // WebSocket is always ready if server is up

           let all_ok = scene_ok && ws_ok;

           let status = HealthStatus {
               status: if all_ok { "healthy" } else { "unhealthy" },
               version: env!("CARGO_PKG_VERSION"),
               checks: HealthChecks {
                   scene_store: scene_ok,
                   websocket: ws_ok,
               },
           };

           let code = if all_ok {
               StatusCode::OK
           } else {
               StatusCode::SERVICE_UNAVAILABLE
           };

           (code, Json(status))
       }
       ```

    2. Add routes in routes.rs:
       ```rust
       mod health;

       // Add to router:
       .route("/health/live", get(health::liveness))
       .route("/health/ready", get(health::readiness))
       // Also add legacy /health for backward compatibility
       .route("/health", get(health::readiness))
       ```

    3. Document health endpoints in code comments:
       - /health/live - Kubernetes liveness probe (restart if fails)
       - /health/ready - Kubernetes readiness probe (remove from LB if fails)
       - /health - Backward compatible combined check
  </action>
  <verify>
    cargo fmt --all -- --check
    cargo clippy -p canvas-server --all-features -- -D warnings
    cargo test -p canvas-server
    # Manual tests:
    curl http://localhost:9473/health/live   # Should return 200
    curl http://localhost:9473/health/ready  # Should return JSON with status
    curl http://localhost:9473/health        # Should return JSON with status
  </verify>
  <done>
    - /health/live returns 200 for liveness
    - /health/ready returns JSON with component checks
    - /health for backward compatibility
    - Returns 503 if any check fails
    - Version included in response
  </done>
</task>

---

## Verification

```bash
# Build and lint
cargo fmt --all -- --check
cargo clippy --workspace --all-features -- -D warnings
cargo test --workspace

# Manual verification
cargo run -p canvas-server &

# Test structured logging
RUST_LOG=debug curl http://localhost:9473/scene
# Verify request span in logs with timing

# Test JSON logging
RUST_LOG_FORMAT=json RUST_LOG=info cargo run -p canvas-server
# Verify JSON output

# Test metrics
curl http://localhost:9473/metrics
# Should see canvas_* metrics

# Test health endpoints
curl -s http://localhost:9473/health/live
# 200 OK

curl -s http://localhost:9473/health/ready | jq
# {"status":"healthy","version":"0.1.0","checks":{...}}

# Load test to generate metrics
for i in {1..100}; do curl -s http://localhost:9473/scene > /dev/null; done
curl http://localhost:9473/metrics | grep canvas_http_requests_total
```

## Risks

- **Low**: Metrics overhead is negligible for typical workloads
- **Low**: Tracing may increase log volume in debug mode
- **Medium**: PrometheusHandle must be shared thread-safely

## Notes

- JSON logging recommended for production (easier to parse)
- Metrics follow Prometheus naming conventions (snake_case, _total suffix for counters)
- Health checks designed for Kubernetes deployment
- Request IDs enable distributed tracing correlation

## Exit Criteria

- [x] TraceLayer adds spans to all HTTP requests
- [x] Request IDs propagate via x-request-id header
- [x] WebSocket handlers have session spans
- [x] JSON log format available via RUST_LOG_FORMAT=json
- [x] /metrics returns Prometheus format
- [x] HTTP requests, WebSocket, MCP, signaling metrics tracked
- [x] /health/live returns 200
- [x] /health/ready returns component status JSON
- [x] All clippy warnings resolved
- [x] ROADMAP.md updated with Phase 4.1 progress
