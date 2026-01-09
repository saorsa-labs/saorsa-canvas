//! # Saorsa Canvas Renderer
//!
//! Custom minimal renderer built on wgpu for maximum control and smallest footprint.
//!
//! ## Rendering Backends
//!
//! ```text
//! ┌─────────────────────────────────────────────┐
//! │            Renderer Trait                   │
//! ├─────────────┬─────────────┬─────────────────┤
//! │ WebGPU/wgpu │ WebGL2      │ 2D Fallback     │
//! │ (GPU)       │ (older GPU) │ (no GPU)        │
//! └─────────────┴─────────────┴─────────────────┘
//! ```

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(clippy::all)]
#![deny(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]

pub mod backend;
pub mod chart;
pub mod error;

pub use backend::RenderBackend;
pub use error::{RenderError, RenderResult};

use canvas_core::Scene;

/// Configuration for the renderer.
#[derive(Debug, Clone)]
pub struct RendererConfig {
    /// Preferred backend (will fall back if unavailable).
    pub preferred_backend: BackendType,
    /// Target frames per second.
    pub target_fps: u32,
    /// Enable anti-aliasing.
    pub anti_aliasing: bool,
    /// Background color (RGBA).
    pub background_color: [f32; 4],
}

impl Default for RendererConfig {
    fn default() -> Self {
        Self {
            preferred_backend: BackendType::WebGpu,
            target_fps: 60,
            anti_aliasing: true,
            background_color: [1.0, 1.0, 1.0, 1.0], // White
        }
    }
}

/// Available rendering backends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendType {
    /// WebGPU via wgpu (best quality, requires modern GPU).
    WebGpu,
    /// WebGL2 fallback (older GPU support).
    WebGl2,
    /// Pure 2D canvas fallback (no GPU required).
    Canvas2D,
}

/// The main renderer interface.
pub struct Renderer {
    config: RendererConfig,
    backend: Box<dyn RenderBackend>,
    frame_count: u64,
}

impl Renderer {
    /// Create a new renderer with the given configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if no suitable backend is available.
    pub fn new(config: RendererConfig) -> RenderResult<Self> {
        let backend = Self::create_backend(&config)?;

        Ok(Self {
            config,
            backend,
            frame_count: 0,
        })
    }

    /// Create the appropriate backend based on config and availability.
    fn create_backend(config: &RendererConfig) -> RenderResult<Box<dyn RenderBackend>> {
        match config.preferred_backend {
            BackendType::WebGpu => {
                #[cfg(feature = "gpu")]
                {
                    match backend::wgpu::WgpuBackend::new() {
                        Ok(b) => return Ok(Box::new(b)),
                        Err(e) => {
                            tracing::warn!("WebGPU unavailable, falling back: {}", e);
                        }
                    }
                }
                // Fall through to next backend
                Self::create_backend(&RendererConfig {
                    preferred_backend: BackendType::WebGl2,
                    ..config.clone()
                })
            }
            BackendType::WebGl2 => {
                // TODO: Implement WebGL2 backend
                tracing::warn!("WebGL2 not yet implemented, falling back to 2D");
                Self::create_backend(&RendererConfig {
                    preferred_backend: BackendType::Canvas2D,
                    ..config.clone()
                })
            }
            BackendType::Canvas2D => Ok(Box::new(backend::canvas2d::Canvas2DBackend::new())),
        }
    }

    /// Render a frame.
    ///
    /// # Errors
    ///
    /// Returns an error if rendering fails.
    pub fn render(&mut self, scene: &Scene) -> RenderResult<()> {
        self.backend.render(scene)?;
        self.frame_count += 1;
        Ok(())
    }

    /// Get the current frame count.
    #[must_use]
    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }

    /// Get the active backend type.
    #[must_use]
    pub fn active_backend(&self) -> BackendType {
        self.backend.backend_type()
    }

    /// Get the renderer configuration.
    #[must_use]
    pub fn config(&self) -> &RendererConfig {
        &self.config
    }

    /// Resize the rendering surface.
    ///
    /// # Errors
    ///
    /// Returns an error if resize fails.
    pub fn resize(&mut self, width: u32, height: u32) -> RenderResult<()> {
        self.backend.resize(width, height)
    }
}
