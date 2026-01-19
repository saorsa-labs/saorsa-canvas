//! Rendering backend implementations.

pub mod canvas2d;
#[cfg(feature = "gpu")]
pub mod wgpu;

use canvas_core::Scene;

use crate::{BackendType, RenderResult};

/// Trait for rendering backends.
pub trait RenderBackend {
    /// Get the backend type.
    fn backend_type(&self) -> BackendType;

    /// Render a scene.
    ///
    /// # Errors
    ///
    /// Returns an error if rendering fails.
    fn render(&mut self, scene: &Scene) -> RenderResult<()>;

    /// Resize the rendering surface.
    ///
    /// # Errors
    ///
    /// Returns an error if resizing fails.
    fn resize(&mut self, width: u32, height: u32) -> RenderResult<()>;
}
