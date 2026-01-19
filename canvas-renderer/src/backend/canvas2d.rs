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
    ///
    /// Logs element information for debugging purposes.
    fn render_element(element: &Element) {
        let t = &element.transform;
        let (kind_name, details) = Self::element_description(&element.kind);

        tracing::trace!(
            "Render {kind_name} at ({}, {}) size {}x{}{details}",
            t.x,
            t.y,
            t.width,
            t.height
        );
    }

    /// Get a description of an element kind for logging.
    fn element_description(kind: &ElementKind) -> (&'static str, String) {
        match kind {
            ElementKind::Text {
                content,
                font_size,
                color,
            } => (
                "text",
                format!(" content='{content}' font={font_size} color={color}"),
            ),
            ElementKind::Image { src, format } => {
                ("image", format!(" src={src} format={format:?}"))
            }
            ElementKind::Chart { chart_type, .. } => ("chart", format!(" type={chart_type}")),
            ElementKind::Model3D { .. } => (
                "3D placeholder",
                " (3D not supported in 2D mode)".to_string(),
            ),
            ElementKind::Video { stream_id, .. } => ("video", format!(" stream={stream_id}")),
            ElementKind::OverlayLayer { children, opacity } => {
                let count = children.len();
                ("overlay", format!(" children={count} opacity={opacity}"))
            }
            ElementKind::Group { children } => {
                let count = children.len();
                ("group", format!(" children={count}"))
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
