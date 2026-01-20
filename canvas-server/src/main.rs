//! # Saorsa Canvas Server
//!
//! Local embedded server for the Saorsa Canvas PWA.
//! Binds to localhost only for security.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::{
    extract::{ws::WebSocketUpgrade, State},
    http::{header, HeaderValue, Method, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use canvas_core::SceneDocument;
use canvas_mcp::{CanvasMcpServer, JsonRpcRequest, JsonRpcResponse};
use tower_http::{
    cors::CorsLayer,
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    services::ServeDir,
    trace::{DefaultMakeSpan, DefaultOnRequest, DefaultOnResponse, TraceLayer},
};
use tracing::Level;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use canvas_server::agui;
use canvas_server::communitas::{
    self, spawn_network_retry_task, ClientDescriptor, CommunitasMcpClient, NetworkRetryConfig,
    RetryConfig,
};
use canvas_server::health;
use canvas_server::metrics;
use canvas_server::routes;
use canvas_server::sync::{self, current_timestamp, handle_sync_socket, SyncOrigin, SyncState};
use canvas_server::AppState;
use metrics_exporter_prometheus::PrometheusHandle;

/// Default port for the canvas server.
const DEFAULT_PORT: u16 = 9473; // "SAOR" on phone keypad

/// Build a CORS layer that only allows localhost origins.
///
/// This is a security measure to ensure the server only accepts requests from
/// the local machine. The server is designed to run on localhost only.
fn build_cors_layer(port: u16) -> CorsLayer {
    // Allowed localhost origins with the configured port
    let localhost_origins = [
        format!("http://localhost:{port}"),
        format!("http://127.0.0.1:{port}"),
        // Also allow common development ports for dev servers
        "http://localhost:3000".to_string(),
        "http://localhost:5173".to_string(), // Vite
        "http://localhost:8080".to_string(),
        "http://127.0.0.1:3000".to_string(),
        "http://127.0.0.1:5173".to_string(),
        "http://127.0.0.1:8080".to_string(),
    ];

    let origins: Vec<HeaderValue> = localhost_origins
        .iter()
        .filter_map(|o| o.parse().ok())
        .collect();

    CorsLayer::new()
        .allow_origin(origins)
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION, header::ACCEPT])
        .allow_credentials(true)
}

/// Initialize structured tracing with optional JSON format.
///
/// Set `RUST_LOG` to control log levels (default: info,canvas_server=debug,tower_http=debug).
/// Set `RUST_LOG_FORMAT=json` for JSON output (recommended for production).
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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing with optional JSON format
    init_tracing();

    // Initialize Prometheus metrics
    let metrics_handle = metrics::init_metrics()
        .map_err(|e| anyhow::anyhow!("Failed to initialize Prometheus metrics: {}", e))?;
    tracing::info!("Prometheus metrics initialized");

    let port = std::env::var("CANVAS_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(DEFAULT_PORT);

    // Determine the paths for static files
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let web_dir = manifest_dir.join("../web");
    let pkg_dir = manifest_dir.join("../web/pkg");

    tracing::info!("Serving web files from: {:?}", web_dir);
    tracing::info!("Serving WASM package from: {:?}", pkg_dir);

    // Build static file services
    let web_service = ServeDir::new(&web_dir);
    let pkg_service = ServeDir::new(&pkg_dir);

    // Create sync state for WebSocket scene synchronization
    let sync_state = SyncState::new();
    let communitas_client = init_communitas_client(&sync_state).await;

    // Create MCP server with change notification callback
    let scene_tx = sync_state.sender();
    let mut mcp = CanvasMcpServer::new(sync_state.store());
    mcp.set_on_change(move |session_id, scene| {
        let document = SceneDocument::from_scene(session_id, scene, current_timestamp());
        let event = sync::SyncEvent {
            session_id: session_id.to_string(),
            message: sync::ServerMessage::SceneUpdate { scene: document },
            origin: sync::SyncOrigin::Local,
        };
        // Ignore send errors (no receivers is okay)
        let _ = scene_tx.send(event);
    });

    // Create AG-UI state
    let agui_state = agui::AgUiState::new(sync_state.clone());

    // Create shared state with MCP server and sync
    let state = AppState {
        mcp: Arc::new(mcp),
        sync: sync_state.clone(),
        communitas: communitas_client,
    };

    // Build AG-UI router
    let agui_router = Router::new()
        .route("/stream", get(agui::stream_handler))
        .route("/render", post(agui::render_handler))
        .with_state(agui_state);

    // Build metrics router with PrometheusHandle
    let metrics_router = Router::new()
        .route("/metrics", get(metrics_handler))
        .with_state(metrics_handle);

    // Build the router
    let app = Router::new()
        // Metrics endpoint (separate state)
        .merge(metrics_router)
        // Health check endpoints (Kubernetes probes)
        .route("/health/live", get(health::liveness))
        .route("/health/ready", get(health::readiness))
        .route("/health", get(health::readiness)) // Backward compatible
        .route("/ws", get(websocket_handler))
        .route("/ws/sync", get(sync_websocket_handler))
        .route("/mcp", post(mcp_handler))
        .route(
            "/api/scene",
            get(routes::get_scene_handler).post(routes::update_scene_handler),
        )
        .route("/api/scene/{session_id}", get(routes::get_session_scene))
        // AG-UI endpoints
        .nest("/ag-ui", agui_router)
        // Serve WASM package at /pkg
        .nest_service("/pkg", pkg_service)
        // Serve manifest.json and sw.js from web directory
        .route("/manifest.json", get(manifest_handler))
        .route("/sw.js", get(sw_handler))
        // Fallback to index.html for SPA
        .fallback_service(web_service)
        // Request ID for distributed tracing correlation
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
        // CORS configuration - restricted to localhost only for security
        .layer(build_cors_layer(port))
        // Structured request tracing with timing
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
                .on_request(DefaultOnRequest::new().level(Level::INFO))
                .on_response(DefaultOnResponse::new().level(Level::INFO)),
        )
        .with_state(state);

    // Bind to localhost ONLY (security requirement)
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;

    tracing::info!("Saorsa Canvas server starting on http://{}", addr);
    tracing::info!("Open http://localhost:{} in your browser", port);

    axum::serve(listener, app).await?;

    Ok(())
}

/// Serve the manifest.json file.
async fn manifest_handler() -> impl IntoResponse {
    (
        StatusCode::OK,
        [("Content-Type", "application/manifest+json")],
        include_str!("../../web/manifest.json"),
    )
}

/// Serve the service worker file.
async fn sw_handler() -> impl IntoResponse {
    (
        StatusCode::OK,
        [("Content-Type", "application/javascript")],
        include_str!("../../web/sw.js"),
    )
}

/// Prometheus metrics endpoint.
#[tracing::instrument(name = "metrics", skip(handle))]
async fn metrics_handler(State(handle): State<PrometheusHandle>) -> impl IntoResponse {
    handle.render()
}

/// MCP JSON-RPC endpoint.
#[tracing::instrument(name = "mcp_handler", skip(state, request), fields(method = %request.method))]
async fn mcp_handler(
    State(state): State<AppState>,
    Json(request): Json<JsonRpcRequest>,
) -> Json<JsonRpcResponse> {
    tracing::debug!("Processing MCP request");
    let response = state.mcp.handle_request(request).await;
    Json(response)
}

/// Legacy WebSocket handler (backwards compatible).
#[tracing::instrument(name = "websocket_connect", skip(ws, state))]
async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    tracing::info!("WebSocket connection upgrade requested");
    ws.on_upgrade(move |socket| handle_sync_socket(socket, state.sync))
}

/// Sync WebSocket handler for real-time scene synchronization.
#[tracing::instrument(name = "sync_websocket_connect", skip(ws, state))]
async fn sync_websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    tracing::info!("Sync WebSocket connection upgrade requested");
    ws.on_upgrade(move |socket| handle_sync_socket(socket, state.sync))
}

/// Initialize Communitas MCP client for upstream scene synchronization.
#[tracing::instrument(name = "init_communitas", skip(sync_state))]
async fn init_communitas_client(sync_state: &SyncState) -> Option<CommunitasMcpClient> {
    let url = match std::env::var("COMMUNITAS_MCP_URL") {
        Ok(url) => url,
        Err(_) => return None,
    };

    let descriptor = ClientDescriptor {
        name: "saorsa-canvas-server".into(),
        version: env!("CARGO_PKG_VERSION").into(),
    };

    let client = match CommunitasMcpClient::new(url.as_str(), descriptor) {
        Ok(client) => client,
        Err(err) => {
            tracing::error!("Failed to configure Communitas MCP client: {}", err);
            return None;
        }
    };

    if let Err(err) = client.initialize().await {
        tracing::error!("Communitas MCP initialize failed: {}", err);
        return None;
    }

    if let Ok(token) = std::env::var("COMMUNITAS_MCP_TOKEN") {
        if let Err(err) = client.authenticate_with_token(token.trim()).await {
            tracing::error!("Communitas delegate authentication failed: {}", err);
            return None;
        }
    }

    let preferred_port = std::env::var("COMMUNITAS_NETWORK_PORT")
        .ok()
        .and_then(|p| p.parse().ok());

    // Retry network_start with exponential backoff for transient failures
    let retry_config = RetryConfig::new(5, 500, 8000, 2.0);
    let mut network_ok = false;
    let mut last_error: Option<String> = None;

    for attempt in 0..retry_config.max_attempts {
        match client.network_start(preferred_port).await {
            Ok(()) => {
                tracing::info!(
                    "Communitas networking (saorsa-webrtc over ant-quic) started (attempt {})",
                    attempt + 1
                );
                network_ok = true;
                break;
            }
            Err(err) => {
                last_error = Some(err.to_string());
                if !err.is_retryable() {
                    tracing::warn!(
                        "Communitas network_start failed with non-retryable error: {}",
                        err
                    );
                    break;
                }

                if attempt + 1 < retry_config.max_attempts {
                    let delay = retry_config.delay_for_attempt(attempt);
                    tracing::warn!(
                        "Communitas network_start failed (attempt {}/{}), retrying in {}ms: {}",
                        attempt + 1,
                        retry_config.max_attempts,
                        delay,
                        err
                    );
                    tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
                } else {
                    tracing::warn!(
                        "Communitas network_start failed after {} attempts: {}",
                        retry_config.max_attempts,
                        err
                    );
                }
            }
        }
    }

    if !network_ok {
        if let Some(err) = last_error {
            tracing::warn!(
                "Communitas networking unavailable: {}; legacy signaling remains enabled",
                err
            );
        }
    }
    // Always fetch scene to sync state (works even without networking)
    match client.fetch_scene("default").await {
        Ok(document) => match document.clone().into_scene() {
            Ok(scene) => {
                if let Err(err) = sync_state.replace_scene("default", scene, SyncOrigin::Remote) {
                    tracing::warn!("Failed to cache Communitas scene: {}", err);
                }
            }
            Err(err) => tracing::warn!("Failed to convert Communitas scene: {}", err),
        },
        Err(err) => tracing::warn!("Communitas fetch_scene failed: {}", err),
    }

    // Only set client (disabling legacy signaling) if networking succeeded
    if network_ok {
        sync_state.set_communitas_client(client.clone());
        communitas::spawn_scene_bridge(sync_state.clone(), client.clone());
        tracing::info!(
            "Communitas MCP client connected at {} (legacy signaling disabled)",
            url
        );
    } else {
        // Spawn scene bridge for data sync (even without networking)
        communitas::spawn_scene_bridge(sync_state.clone(), client.clone());

        // Spawn background retry task for persistent network recovery
        let retry_config = NetworkRetryConfig::default();
        let _retry_handle = spawn_network_retry_task(
            client.clone(),
            sync_state.clone(),
            preferred_port,
            retry_config,
        );
        tracing::info!(
            "Communitas MCP client connected at {} (legacy signaling enabled, background retry active)",
            url
        );
    }

    Some(client)
}
