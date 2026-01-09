//! 2D Canvas fallback backend for devices without GPU support.
//!
//! This backend uses pure 2D drawing (SVG/Canvas2D in browsers)
//! when WebGPU/WebGL are unavailable.

use canvas_core::{Element, ElementKind, Scene};

use crate::{BackendType, RenderResult};

use super::RenderBackend;

/// 2D Canvas fallback renderer.
pub struct Canvas2DBackend {
    width: u32,
    height: u32,
}

impl Canvas2DBackend {
    /// Create a new 2D canvas backend.
    #[must_use]
    pub fn new() -> Self {
        Self {
            width: 800,
            height: 600,
        }
    }

    /// Render a single element to the 2D context.
    fn render_element(element: &Element) {
        let t = &element.transform;

        match &element.kind {
            ElementKind::Text {
                content,
                font_size,
                color,
            } => {
                tracing::trace!(
                    "Render text '{}' at ({}, {}) size {} color {}",
                    content,
                    t.x,
                    t.y,
                    font_size,
                    color
                );
            }
            ElementKind::Image { src, format } => {
                tracing::trace!(
                    "Render image {:?} from {} at ({}, {})",
                    format,
                    src,
                    t.x,
                    t.y
                );
            }
            ElementKind::Chart { chart_type, .. } => {
                tracing::trace!(
                    "Render {} chart at ({}, {}) size {}x{}",
                    chart_type,
                    t.x,
                    t.y,
                    t.width,
                    t.height
                );
            }
            ElementKind::Model3D { .. } => {
                // In 2D fallback mode, show a placeholder for 3D models
                tracing::trace!(
                    "Render 3D placeholder at ({}, {}) - 3D not supported in 2D mode",
                    t.x,
                    t.y
                );
            }
            ElementKind::Video { stream_id, .. } => {
                tracing::trace!(
                    "Render video stream {} at ({}, {}) size {}x{}",
                    stream_id,
                    t.x,
                    t.y,
                    t.width,
                    t.height
                );
            }
            ElementKind::OverlayLayer { children, opacity } => {
                tracing::trace!(
                    "Render overlay layer with {} children at opacity {}",
                    children.len(),
                    opacity
                );
            }
            ElementKind::Group { children } => {
                tracing::trace!("Render group with {} children", children.len());
            }
        }
    }
}

impl Default for Canvas2DBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderBackend for Canvas2DBackend {
    fn backend_type(&self) -> BackendType {
        BackendType::Canvas2D
    }

    fn render(&mut self, scene: &Scene) -> RenderResult<()> {
        tracing::trace!(
            "Canvas2D render: {} elements, viewport {}x{}",
            scene.element_count(),
            self.width,
            self.height
        );

        // Sort elements by z-index and render
        let mut elements: Vec<_> = scene.elements().collect();
        elements.sort_by_key(|e| e.transform.z_index);

        for element in elements {
            Self::render_element(element);
        }

        Ok(())
    }

    fn resize(&mut self, width: u32, height: u32) -> RenderResult<()> {
        self.width = width;
        self.height = height;
        tracing::debug!("Canvas2D resized to {}x{}", width, height);
        Ok(())
    }
}
