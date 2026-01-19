//! Test server harness for integration tests.
//!
//! Provides a way to spin up a real Axum server on a random port
//! for integration testing with WebSocket and HTTP clients.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::{
    extract::{ws::WebSocketUpgrade, State},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use canvas_core::SceneDocument;
use canvas_mcp::{server::CanvasMcpServer, JsonRpcRequest, JsonRpcResponse};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tower_http::cors::{Any, CorsLayer};

// Re-use sync types from canvas-server
// Note: We need to import from the crate being tested
use canvas_server::sync::{current_timestamp, handle_sync_socket, SyncOrigin, SyncState};

/// Shared application state for test server.
#[derive(Clone)]
struct TestAppState {
    mcp: Arc<CanvasMcpServer>,
    sync: SyncState,
}

/// A test server instance with control handles.
pub struct TestServer {
    addr: SocketAddr,
    sync: SyncState,
    shutdown_tx: Option<oneshot::Sender<()>>,
    handle: JoinHandle<()>,
}

impl TestServer {
    /// Start a new test server on a random available port.
    ///
    /// # Panics
    ///
    /// Panics if no port is available or server fails to bind.
    pub async fn start() -> Self {
        let port = portpicker::pick_unused_port().expect("no available port");
        let addr = SocketAddr::from(([127, 0, 0, 1], port));

        // Create sync state
        let sync_state = SyncState::new();

        // Create MCP server with change notification callback
        let scene_tx = sync_state.sender();
        let mut mcp = CanvasMcpServer::new(sync_state.store());
        mcp.set_on_change(move |session_id, scene| {
            let document = SceneDocument::from_scene(session_id, scene, current_timestamp());
            let event = canvas_server::sync::SyncEvent {
                session_id: session_id.to_string(),
                message: canvas_server::sync::ServerMessage::SceneUpdate { scene: document },
                origin: SyncOrigin::Local,
            };
            let _ = scene_tx.send(event);
        });

        let state = TestAppState {
            mcp: Arc::new(mcp),
            sync: sync_state.clone(),
        };

        // Build a minimal router for testing (no static files)
        let app = Router::new()
            .route("/health", get(health_handler))
            .route("/ws", get(ws_handler))
            .route("/ws/sync", get(ws_handler))
            .route("/mcp", post(mcp_handler))
            .layer(CorsLayer::new().allow_origin(Any).allow_methods(Any))
            .with_state(state);

        let listener = TcpListener::bind(addr).await.expect("failed to bind");
        let actual_addr = listener.local_addr().expect("failed to get local addr");

        // Create shutdown channel
        let (shutdown_tx, shutdown_rx) = oneshot::channel();

        // Spawn the server
        let handle = tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    let _ = shutdown_rx.await;
                })
                .await
                .expect("server error");
        });

        // Give the server a moment to start
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        Self {
            addr: actual_addr,
            sync: sync_state,
            shutdown_tx: Some(shutdown_tx),
            handle,
        }
    }

    /// Get the server's socket address.
    #[allow(dead_code)]
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Get the WebSocket URL for connecting to the server.
    pub fn ws_url(&self) -> String {
        format!("ws://{}/ws", self.addr)
    }

    /// Get the MCP endpoint URL.
    #[allow(dead_code)]
    pub fn mcp_url(&self) -> String {
        format!("http://{}/mcp", self.addr)
    }

    /// Get access to the sync state (for test assertions).
    #[allow(dead_code)]
    pub fn sync_state(&self) -> &SyncState {
        &self.sync
    }

    /// Gracefully shut down the server.
    pub async fn shutdown(mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        let _ = tokio::time::timeout(tokio::time::Duration::from_secs(5), self.handle).await;
    }
}

// Handler implementations for test server

async fn health_handler() -> &'static str {
    "ok"
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<TestAppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_sync_socket(socket, state.sync))
}

async fn mcp_handler(
    State(state): State<TestAppState>,
    Json(request): Json<JsonRpcRequest>,
) -> Json<JsonRpcResponse> {
    let response = state.mcp.handle_request(request).await;
    Json(response)
}
