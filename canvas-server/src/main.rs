//! # Saorsa Canvas Server
//!
//! Local embedded server for the Saorsa Canvas PWA.
//! Binds to localhost only for security.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use canvas_core::Scene;
use canvas_mcp::{CanvasMcpServer, JsonRpcRequest, JsonRpcResponse};
use futures::{SinkExt, StreamExt};
use tokio::sync::broadcast;
use tower_http::{
    cors::{Any, CorsLayer},
    services::ServeDir,
    trace::TraceLayer,
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod agui;
mod routes;

/// Scene change event for WebSocket broadcast.
#[derive(Debug, Clone)]
struct SceneChangeEvent {
    /// Session ID that changed.
    session_id: String,
    /// JSON representation of the scene update.
    payload: String,
}

/// Shared application state.
#[derive(Clone)]
struct AppState {
    /// MCP server instance.
    mcp: Arc<CanvasMcpServer>,
    /// Broadcast channel for scene changes.
    scene_tx: broadcast::Sender<SceneChangeEvent>,
}

/// Default port for the canvas server.
const DEFAULT_PORT: u16 = 9473; // "SAOR" on phone keypad

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG")
                .unwrap_or_else(|_| "canvas_server=debug,tower_http=debug".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

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

    // Create broadcast channel for scene changes (capacity: 100 messages)
    let (scene_tx, _) = broadcast::channel::<SceneChangeEvent>(100);
    let scene_tx_clone = scene_tx.clone();

    // Create MCP server with change notification callback
    let mut mcp = CanvasMcpServer::new();
    mcp.set_on_change(move |session_id, scene| {
        let event = SceneChangeEvent {
            session_id: session_id.to_string(),
            payload: serde_json::json!({
                "type": "scene_update",
                "session_id": session_id,
                "element_count": scene.element_count(),
                "timestamp": std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0)
            })
            .to_string(),
        };
        // Ignore send errors (no receivers is okay)
        let _ = scene_tx_clone.send(event);
    });

    // Create shared scene for AG-UI
    let scene = Arc::new(RwLock::new(Scene::new(800.0, 600.0)));

    // Create AG-UI state
    let agui_state = agui::AgUiState::new(scene);

    // Create shared state with MCP server
    let state = AppState {
        mcp: Arc::new(mcp),
        scene_tx,
    };

    // Build AG-UI router
    let agui_router = Router::new()
        .route("/stream", get(agui::stream_handler))
        .route("/render", post(agui::render_handler))
        .with_state(agui_state);

    // Build the router
    let app = Router::new()
        // API routes
        .route("/health", get(health_handler))
        .route("/ws", get(websocket_handler))
        .route("/mcp", post(mcp_handler))
        .route(
            "/api/scene",
            get(routes::get_scene).post(routes::update_scene),
        )
        // AG-UI endpoints
        .nest("/ag-ui", agui_router)
        // Serve WASM package at /pkg
        .nest_service("/pkg", pkg_service)
        // Serve manifest.json and sw.js from web directory
        .route("/manifest.json", get(manifest_handler))
        .route("/sw.js", get(sw_handler))
        // Fallback to index.html for SPA
        .fallback_service(web_service)
        .layer(CorsLayer::new().allow_origin(Any).allow_methods(Any))
        .layer(TraceLayer::new_for_http())
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

/// Health check endpoint.
async fn health_handler() -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION")
    }))
}

/// MCP JSON-RPC endpoint.
async fn mcp_handler(
    State(state): State<AppState>,
    Json(request): Json<JsonRpcRequest>,
) -> Json<JsonRpcResponse> {
    tracing::info!("MCP request: {}", request.method);
    let response = state.mcp.handle_request(request).await;
    Json(response)
}

/// WebSocket handler for real-time communication.
async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state.scene_tx.subscribe()))
}

/// Handle a WebSocket connection.
async fn handle_socket(socket: WebSocket, mut scene_rx: broadcast::Receiver<SceneChangeEvent>) {
    let (mut sender, mut receiver) = socket.split();

    // Send welcome message
    let welcome = serde_json::json!({
        "type": "welcome",
        "version": env!("CARGO_PKG_VERSION")
    });

    if sender
        .send(Message::Text(welcome.to_string().into()))
        .await
        .is_err()
    {
        return;
    }

    tracing::info!("WebSocket client connected");

    // Handle incoming messages and broadcast scene updates concurrently
    loop {
        tokio::select! {
            // Handle incoming WebSocket messages
            msg = receiver.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        tracing::debug!("Received: {}", text);

                        // Parse and handle client messages
                        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&text) {
                            let msg_type = parsed.get("type").and_then(|v| v.as_str()).unwrap_or("");

                            match msg_type {
                                "subscribe" => {
                                    // Client subscribing to session updates
                                    let session_id = parsed.get("session_id")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("default");
                                    let response = serde_json::json!({
                                        "type": "subscribed",
                                        "session_id": session_id
                                    });
                                    if sender.send(Message::Text(response.to_string().into())).await.is_err() {
                                        break;
                                    }
                                }
                                "ping" => {
                                    // Respond to ping with pong
                                    let response = serde_json::json!({ "type": "pong" });
                                    if sender.send(Message::Text(response.to_string().into())).await.is_err() {
                                        break;
                                    }
                                }
                                _ => {
                                    // Echo unknown messages back with ack
                                    let response = serde_json::json!({
                                        "type": "ack",
                                        "received": text.to_string()
                                    });
                                    if sender.send(Message::Text(response.to_string().into())).await.is_err() {
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) => {
                        tracing::info!("Client disconnected");
                        break;
                    }
                    Some(Err(e)) => {
                        tracing::error!("WebSocket error: {}", e);
                        break;
                    }
                    None => break,
                    _ => {}
                }
            }

            // Broadcast scene updates to client
            event = scene_rx.recv() => {
                match event {
                    Ok(scene_event) => {
                        tracing::debug!("Broadcasting scene update for session: {}", scene_event.session_id);
                        if sender.send(Message::Text(scene_event.payload.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("WebSocket client lagged behind by {} messages", n);
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        tracing::info!("Broadcast channel closed");
                        break;
                    }
                }
            }
        }
    }

    tracing::info!("WebSocket connection closed");
}
