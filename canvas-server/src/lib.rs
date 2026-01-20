//! # Saorsa Canvas Server Library
//!
//! Shared types and functionality for the canvas server.
//! This library is used by both the binary and integration tests.

use std::sync::Arc;

use canvas_mcp::CanvasMcpServer;

pub mod agui;
pub mod communitas;
pub mod health;
pub mod metrics;
pub mod routes;
pub mod sync;
pub mod validation;

pub use communitas::{
    spawn_network_retry_task, CommunitasMcpClient, NetworkRetryConfig, NetworkRetryHandle,
    RetryConfig,
};
pub use sync::SyncState;

/// Shared application state.
#[derive(Clone)]
pub struct AppState {
    /// MCP server instance.
    pub mcp: Arc<CanvasMcpServer>,
    /// Sync state for WebSocket scene synchronization.
    pub sync: SyncState,
    /// Optional Communitas MCP client for remote state.
    pub communitas: Option<CommunitasMcpClient>,
}

impl AppState {
    /// Get a reference to the sync state.
    pub fn sync(&self) -> &SyncState {
        &self.sync
    }

    /// Get a reference to the optional Communitas client.
    pub fn communitas(&self) -> Option<&CommunitasMcpClient> {
        self.communitas.as_ref()
    }
}
