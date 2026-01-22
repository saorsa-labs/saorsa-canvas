//! # Holographic Rendering
//!
//! Integrates quilt rendering with the rendering backend to produce
//! holographic output for Looking Glass displays.
//!
//! ## Usage
//!
//! ```text
//! 1. Create a HolographicRenderer with a backend and config
//! 2. Call render_quilt() with a scene and camera
//! 3. The result is a QuiltRenderTarget with all views rendered
//! ```

use crate::backend::wgpu::{Viewport, WgpuBackend};
use crate::error::RenderResult;
use crate::quilt::{Quilt, QuiltRenderSettings, QuiltRenderTarget, QuiltView};
use crate::spatial::{Camera, HolographicConfig};
use canvas_core::Scene;
use serde::{Deserialize, Serialize};

/// Result of a holographic render operation.
#[derive(Debug, Clone)]
pub struct HolographicRenderResult {
    /// The rendered quilt target.
    pub target: QuiltRenderTarget,
    /// Number of views rendered.
    pub view_count: u32,
    /// Total render time in milliseconds.
    pub render_time_ms: f64,
}

/// Holographic rendering statistics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HolographicStats {
    /// Total frames rendered.
    pub frames_rendered: u64,
    /// Average render time in milliseconds.
    pub avg_render_time_ms: f64,
    /// Peak render time in milliseconds.
    pub peak_render_time_ms: f64,
    /// Total views rendered across all frames.
    pub total_views_rendered: u64,
}

impl HolographicStats {
    /// Update statistics with a new render result.
    pub fn update(&mut self, result: &HolographicRenderResult) {
        self.frames_rendered += 1;
        self.total_views_rendered += u64::from(result.view_count);

        // Update average (exponential moving average)
        let alpha = 0.1;
        self.avg_render_time_ms =
            alpha * result.render_time_ms + (1.0 - alpha) * self.avg_render_time_ms;

        // Update peak
        if result.render_time_ms > self.peak_render_time_ms {
            self.peak_render_time_ms = result.render_time_ms;
        }
    }

    /// Reset all statistics.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// Renderer for holographic output.
///
/// This wraps a regular renderer and adds quilt generation capability.
#[derive(Debug)]
pub struct HolographicRenderer {
    /// The holographic configuration.
    config: HolographicConfig,
    /// Render settings.
    settings: QuiltRenderSettings,
    /// Rendering statistics.
    stats: HolographicStats,
}

impl HolographicRenderer {
    /// Create a new holographic renderer with the given configuration.
    #[must_use]
    pub fn new(config: HolographicConfig) -> Self {
        Self {
            config,
            settings: QuiltRenderSettings::default(),
            stats: HolographicStats::default(),
        }
    }

    /// Create a holographic renderer with custom settings.
    #[must_use]
    pub fn with_settings(config: HolographicConfig, settings: QuiltRenderSettings) -> Self {
        Self {
            config,
            settings,
            stats: HolographicStats::default(),
        }
    }

    /// Get the current configuration.
    #[must_use]
    pub const fn config(&self) -> &HolographicConfig {
        &self.config
    }

    /// Get the current settings.
    #[must_use]
    pub const fn settings(&self) -> &QuiltRenderSettings {
        &self.settings
    }

    /// Get the current statistics.
    #[must_use]
    pub const fn stats(&self) -> &HolographicStats {
        &self.stats
    }

    /// Update the configuration.
    pub fn set_config(&mut self, config: HolographicConfig) {
        self.config = config;
    }

    /// Update the settings.
    pub fn set_settings(&mut self, settings: QuiltRenderSettings) {
        self.settings = settings;
    }

    /// Render a scene to a quilt.
    ///
    /// This is a software-based reference implementation. For GPU-accelerated
    /// rendering, use the wgpu backend's holographic extension.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub fn render_quilt(&mut self, scene: &Scene, camera: &Camera) -> HolographicRenderResult {
        let start = std::time::Instant::now();

        // Create the quilt with all camera positions
        let quilt = Quilt::new(self.config.clone(), camera);

        // Create the render target
        let mut target = QuiltRenderTarget::from_quilt(&quilt);

        // Clear with background color (clamp to valid range)
        let clear_color = Self::float_color_to_bytes(&self.settings.clear_color);
        target.clear(clear_color);

        // Render each view
        // In a real implementation, this would use the GPU to render
        // each view with proper 3D projection. For now, we just fill
        // each view with a gradient to demonstrate the quilt layout.
        for view in &quilt.views {
            self.render_view_placeholder(&mut target, view, scene);
        }

        let elapsed = start.elapsed();
        let render_time_ms = elapsed.as_secs_f64() * 1000.0;

        // View count is guaranteed to fit in u32 (config.num_views is u32)
        let view_count = self.config.num_views;

        let result = HolographicRenderResult {
            target,
            view_count,
            render_time_ms,
        };

        self.stats.update(&result);

        result
    }

    /// Convert a float RGBA color (0.0-1.0) to byte RGBA (0-255).
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn float_color_to_bytes(color: &[f32; 4]) -> [u8; 4] {
        [
            (color[0].clamp(0.0, 1.0) * 255.0) as u8,
            (color[1].clamp(0.0, 1.0) * 255.0) as u8,
            (color[2].clamp(0.0, 1.0) * 255.0) as u8,
            (color[3].clamp(0.0, 1.0) * 255.0) as u8,
        ]
    }

    /// Render a single view of the quilt using the GPU backend.
    ///
    /// This renders the scene from the view's camera perspective to the
    /// specified viewport region. Used for GPU-accelerated quilt rendering.
    ///
    /// # Arguments
    ///
    /// * `backend` - The wgpu rendering backend
    /// * `scene` - The scene to render
    /// * `view` - The quilt view containing camera and viewport info
    ///
    /// # Errors
    ///
    /// Returns a `RenderError` if:
    /// - The viewport is invalid (zero dimensions or out of bounds)
    /// - The frame rendering fails
    pub fn render_view(
        &self,
        backend: &mut WgpuBackend,
        scene: &Scene,
        view: &QuiltView,
    ) -> RenderResult<()> {
        // Create viewport from QuiltView dimensions
        let viewport = Viewport::new(view.x_offset, view.y_offset, view.width, view.height);

        // Render the scene with this view's camera and viewport
        backend.render_with_camera(scene, Some(&view.camera), Some(viewport))
    }

    /// Placeholder view rendering (demonstrates quilt layout).
    ///
    /// In a real implementation, this would render the scene from
    /// the view's camera perspective.
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    fn render_view_placeholder(
        &self,
        target: &mut QuiltRenderTarget,
        view: &crate::quilt::QuiltView,
        _scene: &Scene,
    ) {
        // Calculate a gradient based on view index to visualize the quilt
        let progress = view.index as f32 / (self.config.num_views - 1).max(1) as f32;

        // Left views are more blue, right views are more red
        let red = (progress * 255.0) as u8;
        let green = 100_u8;
        let blue = ((1.0 - progress) * 255.0) as u8;

        // Fill the view area
        target.fill_rect(
            view.x_offset,
            view.y_offset,
            view.width,
            view.height,
            [red, green, blue, 255],
        );

        // Draw border rectangle around the view
        Self::draw_border(
            target,
            view.x_offset,
            view.y_offset,
            view.width,
            view.height,
        );
    }

    /// Draw a border rectangle at the specified position.
    fn draw_border(target: &mut QuiltRenderTarget, x: u32, y: u32, width: u32, height: u32) {
        const BORDER_COLOR: [u8; 4] = [255, 255, 255, 128];
        const BORDER_WIDTH: u32 = 2;

        // Define borders as (x, y, w, h) tuples
        let borders = [
            (x, y, width, BORDER_WIDTH),                         // Top
            (x, y + height - BORDER_WIDTH, width, BORDER_WIDTH), // Bottom
            (x, y, BORDER_WIDTH, height),                        // Left
            (x + width - BORDER_WIDTH, y, BORDER_WIDTH, height), // Right
        ];

        for (bx, by, bw, bh) in borders {
            target.fill_rect(bx, by, bw, bh, BORDER_COLOR);
        }
    }

    /// Get the expected quilt dimensions for the current configuration.
    #[must_use]
    pub fn quilt_dimensions(&self) -> (u32, u32) {
        (self.config.quilt_width(), self.config.quilt_height())
    }

    /// Reset rendering statistics.
    pub fn reset_stats(&mut self) {
        self.stats.reset();
    }
}

/// Looking Glass `HoloPlay` service connection.
///
/// This represents a connection to the `HoloPlay` Service which manages
/// Looking Glass displays on the system.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HoloPlayInfo {
    /// Whether a Looking Glass display is connected.
    pub display_connected: bool,
    /// Display name if connected.
    pub display_name: Option<String>,
    /// Display resolution.
    pub display_resolution: Option<(u32, u32)>,
    /// Recommended view count.
    pub recommended_views: Option<u32>,
    /// View cone angle in degrees.
    pub view_cone_degrees: Option<f32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_holographic_renderer_creation() {
        let config = HolographicConfig::looking_glass_portrait();
        let renderer = HolographicRenderer::new(config);

        assert_eq!(renderer.config().num_views, 45);
    }

    #[test]
    fn test_holographic_renderer_with_settings() {
        let config = HolographicConfig::looking_glass_portrait();
        let settings = QuiltRenderSettings {
            clear_color: [0.0, 0.0, 0.0, 1.0],
            wireframe: true,
            depth_test: false,
            backface_cull: false,
        };

        let renderer = HolographicRenderer::with_settings(config, settings);

        assert!(renderer.settings().wireframe);
        assert!(!renderer.settings().depth_test);
    }

    #[test]
    fn test_holographic_render_quilt() {
        let config = HolographicConfig {
            num_views: 4,
            quilt_columns: 2,
            quilt_rows: 2,
            view_width: 100,
            view_height: 100,
            view_cone: 40.0_f32.to_radians(),
            focal_distance: 2.0,
        };

        let mut renderer = HolographicRenderer::new(config);
        let scene = Scene::new(800.0, 600.0);
        let camera = Camera::default();

        let result = renderer.render_quilt(&scene, &camera);

        assert_eq!(result.view_count, 4);
        assert_eq!(result.target.width, 200); // 2 columns * 100
        assert_eq!(result.target.height, 200); // 2 rows * 100
    }

    #[test]
    fn test_holographic_stats_update() {
        let mut stats = HolographicStats::default();

        let result = HolographicRenderResult {
            target: QuiltRenderTarget::new(100, 100),
            view_count: 45,
            render_time_ms: 16.6,
        };

        stats.update(&result);

        assert_eq!(stats.frames_rendered, 1);
        assert_eq!(stats.total_views_rendered, 45);
        assert!(stats.avg_render_time_ms > 0.0);
    }

    #[test]
    fn test_holographic_stats_peak() {
        let mut stats = HolographicStats::default();

        // First render
        stats.update(&HolographicRenderResult {
            target: QuiltRenderTarget::new(10, 10),
            view_count: 4,
            render_time_ms: 10.0,
        });

        // Second render with higher time
        stats.update(&HolographicRenderResult {
            target: QuiltRenderTarget::new(10, 10),
            view_count: 4,
            render_time_ms: 20.0,
        });

        assert!((stats.peak_render_time_ms - 20.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_holographic_quilt_dimensions() {
        let config = HolographicConfig::looking_glass_portrait();
        let renderer = HolographicRenderer::new(config);

        let (width, height) = renderer.quilt_dimensions();

        // Portrait: 5 cols * 420px = 2100, 9 rows * 560px = 5040
        assert_eq!(width, 2100);
        assert_eq!(height, 5040);
    }

    #[test]
    fn test_holographic_set_config() {
        let config1 = HolographicConfig::looking_glass_portrait();
        let config2 = HolographicConfig::looking_glass_4k();

        let mut renderer = HolographicRenderer::new(config1);
        assert_eq!(renderer.config().view_width, 420);

        renderer.set_config(config2);
        assert_eq!(renderer.config().view_width, 819);
    }

    #[test]
    fn test_holoplay_info_default() {
        let info = HoloPlayInfo::default();

        assert!(!info.display_connected);
        assert!(info.display_name.is_none());
    }

    #[test]
    fn test_holographic_stats_reset() {
        let mut stats = HolographicStats {
            frames_rendered: 100,
            avg_render_time_ms: 16.6,
            peak_render_time_ms: 33.3,
            total_views_rendered: 4500,
        };

        stats.reset();

        assert_eq!(stats.frames_rendered, 0);
        assert!(stats.avg_render_time_ms.abs() < f64::EPSILON);
    }

    #[test]
    fn test_render_produces_colored_views() {
        let config = HolographicConfig {
            num_views: 2,
            quilt_columns: 2,
            quilt_rows: 1,
            view_width: 10,
            view_height: 10,
            view_cone: 40.0_f32.to_radians(),
            focal_distance: 2.0,
        };

        let mut renderer = HolographicRenderer::new(config);
        let scene = Scene::new(100.0, 100.0);
        let camera = Camera::default();

        let result = renderer.render_quilt(&scene, &camera);

        // View 0 (left) should be more blue
        let left_pixel = result.target.get_pixel(5, 5).expect("should get pixel");
        // View 1 (right) should be more red
        let right_pixel = result.target.get_pixel(15, 5).expect("should get pixel");

        // Left view has more blue than red
        assert!(left_pixel[2] > left_pixel[0]);
        // Right view has more red than blue
        assert!(right_pixel[0] > right_pixel[2]);
    }
}
