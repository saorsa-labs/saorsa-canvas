//! Error types for canvas operations.

use thiserror::Error;

/// Result type for canvas operations.
pub type CanvasResult<T> = Result<T, CanvasError>;

/// Errors that can occur in canvas operations.
#[derive(Debug, Error)]
pub enum CanvasError {
    /// Element not found in scene.
    #[error("Element not found: {0}")]
    ElementNotFound(String),

    /// Invalid element operation.
    #[error("Invalid operation on element: {0}")]
    InvalidOperation(String),

    /// Scene serialization/deserialization error.
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Connection to AI/MCP lost.
    #[error("Connection lost: {0}")]
    ConnectionLost(String),

    /// Resource loading failed.
    #[error("Failed to load resource: {0}")]
    ResourceLoad(String),

    /// Rendering error.
    #[error("Rendering error: {0}")]
    Render(String),
}
