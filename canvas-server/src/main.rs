//! # Saorsa Canvas Server
//!
//! Local embedded server for the Saorsa Canvas PWA.
//! Binds to localhost only for security.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

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
use canvas_mcp::{CanvasMcpServer, JsonRpcRequest, JsonRpcResponse};
use futures::{SinkExt, StreamExt};
use tower_http::{
    cors::{Any, CorsLayer},
    services::ServeDir,
    trace::TraceLayer,
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod routes;

/// Shared application state.
#[derive(Clone)]
struct AppState {
    /// MCP server instance.
    mcp: Arc<CanvasMcpServer>,
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
    let pkg_dir = manifest_dir.join("../canvas-app/pkg");

    tracing::info!("Serving web files from: {:?}", web_dir);
    tracing::info!("Serving WASM package from: {:?}", pkg_dir);

    // Build static file services
    let web_service = ServeDir::new(&web_dir);
    let pkg_service = ServeDir::new(&pkg_dir);

    // Create shared state with MCP server
    let state = AppState {
        mcp: Arc::new(CanvasMcpServer::new()),
    };

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
async fn websocket_handler(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(handle_socket)
}

/// Handle a WebSocket connection.
async fn handle_socket(socket: WebSocket) {
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

    // Handle incoming messages
    while let Some(msg) = receiver.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                tracing::debug!("Received: {}", text);

                // Echo back for now (TODO: integrate with MCP)
                let response = serde_json::json!({
                    "type": "ack",
                    "received": text.to_string()
                });

                if sender
                    .send(Message::Text(response.to_string().into()))
                    .await
                    .is_err()
                {
                    break;
                }
            }
            Ok(Message::Close(_)) => {
                tracing::info!("Client disconnected");
                break;
            }
            Err(e) => {
                tracing::error!("WebSocket error: {}", e);
                break;
            }
            _ => {}
        }
    }
}
