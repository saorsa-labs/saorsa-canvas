//! # Saorsa Canvas Server
//!
//! Local embedded server for the Saorsa Canvas PWA.
//! Binds to localhost only for security.

use std::net::SocketAddr;

use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    response::IntoResponse,
    routing::get,
    Router,
};
use futures::{SinkExt, StreamExt};
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod routes;

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

    // Build the router
    let app = Router::new()
        .route("/", get(index_handler))
        .route("/health", get(health_handler))
        .route("/ws", get(websocket_handler))
        .route(
            "/api/scene",
            get(routes::get_scene).post(routes::update_scene),
        )
        .layer(CorsLayer::new().allow_origin(Any).allow_methods(Any))
        .layer(TraceLayer::new_for_http());

    // Bind to localhost ONLY (security requirement)
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;

    tracing::info!("Saorsa Canvas server starting on http://{}", addr);
    tracing::info!("Open http://localhost:{} in your browser", port);

    axum::serve(listener, app).await?;

    Ok(())
}

/// Serve the main index page.
async fn index_handler() -> impl IntoResponse {
    axum::response::Html(include_str!("../../web/index.html"))
}

/// Health check endpoint.
async fn health_handler() -> impl IntoResponse {
    axum::Json(serde_json::json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION")
    }))
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
