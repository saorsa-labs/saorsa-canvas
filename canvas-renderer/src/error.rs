//! Renderer error types.

use thiserror::Error;

/// Result type for renderer operations.
pub type RenderResult<T> = Result<T, RenderError>;

/// Errors that can occur during rendering.
#[derive(Debug, Error)]
pub enum RenderError {
    /// No suitable rendering backend available.
    #[error("No rendering backend available: {0}")]
    NoBackend(String),

    /// GPU initialization failed.
    #[error("GPU initialization failed: {0}")]
    GpuInit(String),

    /// Shader compilation failed.
    #[error("Shader compilation failed: {0}")]
    Shader(String),

    /// Surface/swapchain error.
    #[error("Surface error: {0}")]
    Surface(String),

    /// Resource loading failed.
    #[error("Failed to load resource: {0}")]
    Resource(String),

    /// Rendering frame failed.
    #[error("Frame render failed: {0}")]
    Frame(String),
}
