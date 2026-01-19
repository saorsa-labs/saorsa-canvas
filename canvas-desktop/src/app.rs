//! Desktop application using winit 0.30 `ApplicationHandler`.

use std::sync::Arc;

use anyhow::Result;
use canvas_core::{Element, ElementKind, Scene, Transform};
use canvas_renderer::backend::wgpu::WgpuBackend;
use canvas_renderer::RenderBackend;
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::WindowEvent,
    event_loop::ActiveEventLoop,
    window::{Window, WindowAttributes, WindowId},
};

use crate::DesktopConfig;

/// Desktop canvas application.
///
/// Manages the winit window and wgpu renderer lifecycle using the
/// `ApplicationHandler` trait introduced in winit 0.30.
pub struct CanvasDesktopApp {
    config: DesktopConfig,
    window: Option<Arc<Window>>,
    renderer: Option<WgpuBackend>,
    scene: Scene,
}

impl CanvasDesktopApp {
    /// Create a new desktop application with the given configuration.
    ///
    /// If `initial_scene` is `Some`, that scene is used. Otherwise, a test scene
    /// is created with sample chart and text elements.
    #[must_use]
    pub fn new(config: DesktopConfig, initial_scene: Option<Scene>) -> Self {
        let scene = initial_scene.unwrap_or_else(|| Self::create_test_scene(&config));
        tracing::debug!("Scene created with {} elements", scene.element_count());

        Self {
            config,
            window: None,
            renderer: None,
            scene,
        }
    }

    /// Create a test scene with sample elements for development/demo purposes.
    #[allow(clippy::cast_precision_loss)] // Window dimensions fit in f32
    fn create_test_scene(config: &DesktopConfig) -> Scene {
        let mut scene = Scene::new(config.width as f32, config.height as f32);

        // Add test elements to verify rendering pipeline
        let chart_element = Element::new(ElementKind::Chart {
            chart_type: "bar".to_string(),
            data: serde_json::json!({
                "title": "Sales by Quarter",
                "labels": ["Q1", "Q2", "Q3", "Q4"],
                "values": [120, 200, 150, 180],
                "background": "#ffffff"
            }),
        })
        .with_transform(Transform {
            x: 100.0,
            y: 100.0,
            width: 400.0,
            height: 300.0,
            rotation: 0.0,
            z_index: 0,
        });

        let text_element = Element::new(ElementKind::Text {
            content: "Saorsa Canvas".to_string(),
            font_size: 24.0,
            color: "#FFFFFF".to_string(),
        })
        .with_transform(Transform {
            x: 100.0,
            y: 420.0,
            width: 200.0,
            height: 40.0,
            rotation: 0.0,
            z_index: 1,
        });

        scene.add_element(chart_element);
        scene.add_element(text_element);

        tracing::debug!("Test scene created with {} elements", scene.element_count());
        scene
    }

    /// Initialize the renderer with the current window.
    fn init_renderer(&mut self, window: Arc<Window>) -> Result<()> {
        // Use WgpuBackend::from_window which handles instance/surface/device setup
        let mut backend = WgpuBackend::from_window(window.clone())?;

        // Set a visible background color (dark blue-gray) to confirm pipeline works
        backend.set_background_color(0.1, 0.12, 0.18, 1.0);

        self.renderer = Some(backend);
        self.window = Some(window);

        tracing::info!("Renderer initialized successfully");

        Ok(())
    }

    /// Handle window resize.
    fn handle_resize(&mut self, size: PhysicalSize<u32>) {
        if size.width == 0 || size.height == 0 {
            return;
        }

        if let Some(renderer) = &mut self.renderer {
            if let Err(e) = renderer.resize(size.width, size.height) {
                tracing::error!("Failed to resize renderer: {e}");
            }
        }
    }

    /// Render the current scene.
    fn render(&mut self) {
        if let Some(renderer) = &mut self.renderer {
            if let Err(e) = renderer.render(&self.scene) {
                tracing::error!("Render error: {e}");
            }
        }
    }
}

impl ApplicationHandler for CanvasDesktopApp {
    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        tracing::info!("App suspended - dropping surface to free resources");
        if let Some(renderer) = &mut self.renderer {
            renderer.drop_surface();
        }
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        tracing::debug!("ApplicationHandler::resumed called");

        // If we have a window but renderer lost its surface, recreate the renderer
        if let Some(window) = &self.window {
            if let Some(renderer) = &self.renderer {
                if !renderer.has_surface() {
                    tracing::info!("Recreating renderer after resume");
                    let window = Arc::clone(window);
                    // Drop old renderer and create new one
                    self.renderer = None;
                    if let Err(e) = self.init_renderer(window) {
                        tracing::error!("Failed to recreate renderer: {e}");
                        event_loop.exit();
                        return;
                    }
                    if let Some(w) = &self.window {
                        w.request_redraw();
                    }
                    return;
                }
            }
        }

        // Only create window if we don't have one
        if self.window.is_some() {
            tracing::debug!("Window already exists, skipping creation");
            return;
        }

        tracing::debug!(
            "Creating window with size {}x{}",
            self.config.width,
            self.config.height
        );

        let attrs = WindowAttributes::default()
            .with_title(&self.config.title)
            .with_inner_size(PhysicalSize::new(self.config.width, self.config.height));

        match event_loop.create_window(attrs) {
            Ok(window) => {
                tracing::debug!("Window created successfully");
                let window = Arc::new(window);
                if let Err(e) = self.init_renderer(window) {
                    tracing::error!("Failed to initialize renderer: {e}");
                    event_loop.exit();
                } else {
                    tracing::debug!("Renderer initialization complete");
                    // Request initial redraw
                    if let Some(w) = &self.window {
                        w.request_redraw();
                    }
                }
            }
            Err(e) => {
                tracing::error!("Failed to create window: {e}");
                event_loop.exit();
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                tracing::info!("Close requested, exiting");
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                tracing::debug!("Window resized to {}x{}", size.width, size.height);
                self.handle_resize(size);
                // Request redraw after resize
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => {
                self.render();
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                tracing::info!("Scale factor changed to {scale_factor}");
                if let Some(renderer) = &mut self.renderer {
                    renderer.set_scale_factor(scale_factor);
                }
                // Get new physical size and resize
                let new_size = self.window.as_ref().map(|w| w.inner_size());
                if let Some(size) = new_size {
                    self.handle_resize(size);
                }
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            _ => {}
        }
    }
}
