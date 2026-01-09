//! # Quilt Rendering
//!
//! Renders multiple camera views into a single quilt texture for Looking Glass displays.
//!
//! ## Overview
//!
//! A quilt is a single texture containing a grid of rendered views. Each view shows
//! the scene from a slightly different camera angle, creating the illusion of depth
//! when displayed on a Looking Glass device.
//!
//! ```text
//! ┌────┬────┬────┬────┬────┬────┬────┬────┐
//! │ 40 │ 41 │ 42 │ 43 │ 44 │ 45 │ 46 │ 47 │  Row 5
//! ├────┼────┼────┼────┼────┼────┼────┼────┤
//! │ 32 │ 33 │ 34 │ 35 │ 36 │ 37 │ 38 │ 39 │  Row 4
//! ├────┼────┼────┼────┼────┼────┼────┼────┤
//! │ 24 │ 25 │ 26 │ 27 │ 28 │ 29 │ 30 │ 31 │  Row 3
//! ├────┼────┼────┼────┼────┼────┼────┼────┤
//! │ 16 │ 17 │ 18 │ 19 │ 20 │ 21 │ 22 │ 23 │  Row 2
//! ├────┼────┼────┼────┼────┼────┼────┼────┤
//! │  8 │  9 │ 10 │ 11 │ 12 │ 13 │ 14 │ 15 │  Row 1
//! ├────┼────┼────┼────┼────┼────┼────┼────┤
//! │  0 │  1 │  2 │  3 │  4 │  5 │  6 │  7 │  Row 0
//! └────┴────┴────┴────┴────┴────┴────┴────┘
//!         8 columns × 6 rows = 48 views
//! ```

use crate::spatial::{Camera, HolographicConfig};
use serde::{Deserialize, Serialize};

/// A single view in the quilt.
#[derive(Debug, Clone)]
pub struct QuiltView {
    /// Index of this view (0 to num_views-1).
    pub index: u32,
    /// Camera for rendering this view.
    pub camera: Camera,
    /// X offset in the quilt texture.
    pub x_offset: u32,
    /// Y offset in the quilt texture.
    pub y_offset: u32,
    /// Width of this view.
    pub width: u32,
    /// Height of this view.
    pub height: u32,
}

/// A complete quilt ready for rendering.
#[derive(Debug, Clone)]
pub struct Quilt {
    /// The holographic configuration.
    pub config: HolographicConfig,
    /// All views in the quilt.
    pub views: Vec<QuiltView>,
    /// Total width of the quilt texture.
    pub total_width: u32,
    /// Total height of the quilt texture.
    pub total_height: u32,
}

impl Quilt {
    /// Create a new quilt from a holographic configuration and base camera.
    #[must_use]
    pub fn new(config: HolographicConfig, base_camera: &Camera) -> Self {
        let total_width = config.quilt_width();
        let total_height = config.quilt_height();
        let mut views = Vec::with_capacity(config.num_views as usize);

        for i in 0..config.num_views {
            let camera = config.camera_for_view(base_camera, i);
            let (x_offset, y_offset) = config.view_offset(i);

            views.push(QuiltView {
                index: i,
                camera,
                x_offset,
                y_offset,
                width: config.view_width,
                height: config.view_height,
            });
        }

        Self {
            config,
            views,
            total_width,
            total_height,
        }
    }

    /// Get the center view (the one that appears at the "front" of the hologram).
    #[must_use]
    pub fn center_view(&self) -> Option<&QuiltView> {
        let center_index = self.config.num_views / 2;
        self.views.get(center_index as usize)
    }

    /// Get a specific view by index.
    #[must_use]
    pub fn view(&self, index: u32) -> Option<&QuiltView> {
        self.views.get(index as usize)
    }

    /// Calculate the aspect ratio of each view.
    #[must_use]
    #[allow(clippy::cast_precision_loss)] // Dimensions are small, precision loss negligible
    pub fn view_aspect_ratio(&self) -> f32 {
        if self.config.view_height == 0 {
            return 1.0;
        }
        self.config.view_width as f32 / self.config.view_height as f32
    }

    /// Calculate the total aspect ratio of the quilt.
    #[must_use]
    #[allow(clippy::cast_precision_loss)] // Dimensions are small, precision loss negligible
    pub fn quilt_aspect_ratio(&self) -> f32 {
        if self.total_height == 0 {
            return 1.0;
        }
        self.total_width as f32 / self.total_height as f32
    }
}

/// Settings for quilt rendering.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuiltRenderSettings {
    /// Clear color for each view (RGBA).
    pub clear_color: [f32; 4],
    /// Whether to render in wireframe mode.
    pub wireframe: bool,
    /// Whether to enable depth testing.
    pub depth_test: bool,
    /// Whether to enable backface culling.
    pub backface_cull: bool,
}

impl Default for QuiltRenderSettings {
    fn default() -> Self {
        Self {
            clear_color: [0.1, 0.1, 0.1, 1.0], // Dark gray
            wireframe: false,
            depth_test: true,
            backface_cull: true,
        }
    }
}

/// Render target for a quilt.
///
/// This is a simple container for quilt pixel data. In a real implementation,
/// this would be backed by GPU textures.
#[derive(Debug, Clone)]
pub struct QuiltRenderTarget {
    /// Width of the quilt texture.
    pub width: u32,
    /// Height of the quilt texture.
    pub height: u32,
    /// Pixel data (RGBA, 8 bits per channel).
    pub pixels: Vec<u8>,
}

impl QuiltRenderTarget {
    /// Create a new render target with the specified dimensions.
    #[must_use]
    pub fn new(width: u32, height: u32) -> Self {
        let size = (width * height * 4) as usize;
        Self {
            width,
            height,
            pixels: vec![0; size],
        }
    }

    /// Create a render target from a quilt configuration.
    #[must_use]
    pub fn from_quilt(quilt: &Quilt) -> Self {
        Self::new(quilt.total_width, quilt.total_height)
    }

    /// Fill a rectangular region with a color.
    pub fn fill_rect(&mut self, x: u32, y: u32, width: u32, height: u32, color: [u8; 4]) {
        for row in y..(y + height).min(self.height) {
            for col in x..(x + width).min(self.width) {
                let idx = ((row * self.width + col) * 4) as usize;
                if idx + 3 < self.pixels.len() {
                    self.pixels[idx] = color[0];
                    self.pixels[idx + 1] = color[1];
                    self.pixels[idx + 2] = color[2];
                    self.pixels[idx + 3] = color[3];
                }
            }
        }
    }

    /// Clear the entire render target with a color.
    pub fn clear(&mut self, color: [u8; 4]) {
        for chunk in self.pixels.chunks_exact_mut(4) {
            chunk[0] = color[0];
            chunk[1] = color[1];
            chunk[2] = color[2];
            chunk[3] = color[3];
        }
    }

    /// Get the pixel at a specific coordinate.
    #[must_use]
    pub fn get_pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let idx = ((y * self.width + x) * 4) as usize;
        if idx + 3 < self.pixels.len() {
            Some([
                self.pixels[idx],
                self.pixels[idx + 1],
                self.pixels[idx + 2],
                self.pixels[idx + 3],
            ])
        } else {
            None
        }
    }
}

/// Looking Glass device preset configurations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LookingGlassPreset {
    /// Looking Glass Portrait (7.9" display).
    Portrait,
    /// Looking Glass 16" (15.6" display).
    LG16,
    /// Looking Glass 32" (31.5" display).
    LG32,
    /// Looking Glass Go (portable).
    Go,
}

impl LookingGlassPreset {
    /// Get the default holographic configuration for this device.
    #[must_use]
    pub fn config(self) -> HolographicConfig {
        match self {
            Self::Portrait => HolographicConfig::looking_glass_portrait(),
            Self::LG16 => HolographicConfig {
                num_views: 45,
                quilt_columns: 5,
                quilt_rows: 9,
                view_width: 768,
                view_height: 432,
                view_cone: 40.0_f32.to_radians(),
                focal_distance: 3.0,
            },
            Self::LG32 => HolographicConfig {
                num_views: 45,
                quilt_columns: 5,
                quilt_rows: 9,
                view_width: 1536,
                view_height: 864,
                view_cone: 40.0_f32.to_radians(),
                focal_distance: 4.0,
            },
            Self::Go => HolographicConfig {
                num_views: 45,
                quilt_columns: 5,
                quilt_rows: 9,
                view_width: 288,
                view_height: 512,
                view_cone: 35.0_f32.to_radians(),
                focal_distance: 2.0,
            },
        }
    }

    /// Get the display resolution for this device.
    #[must_use]
    pub const fn display_resolution(self) -> (u32, u32) {
        match self {
            Self::Portrait => (1536, 2048),
            Self::LG16 => (3840, 2160),
            Self::LG32 => (7680, 4320),
            Self::Go => (1440, 2560),
        }
    }

    /// Get the display name for this preset.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Portrait => "Looking Glass Portrait",
            Self::LG16 => "Looking Glass 16\"",
            Self::LG32 => "Looking Glass 32\"",
            Self::Go => "Looking Glass Go",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quilt_view_count() {
        let config = HolographicConfig::looking_glass_portrait();
        let camera = Camera::default();
        let quilt = Quilt::new(config, &camera);

        assert_eq!(quilt.views.len(), 45); // Looking Glass Portrait has 45 views
    }

    #[test]
    fn test_quilt_dimensions() {
        let config = HolographicConfig::looking_glass_portrait();
        let camera = Camera::default();
        let quilt = Quilt::new(config.clone(), &camera);

        assert_eq!(quilt.total_width, config.quilt_columns * config.view_width);
        assert_eq!(quilt.total_height, config.quilt_rows * config.view_height);
    }

    #[test]
    fn test_quilt_center_view() {
        let config = HolographicConfig::looking_glass_portrait();
        let camera = Camera::default();
        let quilt = Quilt::new(config, &camera);

        let center = quilt.center_view().expect("should have center view");
        assert_eq!(center.index, 22); // 45/2 = 22
    }

    #[test]
    fn test_quilt_view_offsets() {
        let config = HolographicConfig {
            num_views: 8,
            quilt_columns: 4,
            quilt_rows: 2,
            view_width: 100,
            view_height: 100,
            view_cone: 40.0_f32.to_radians(),
            focal_distance: 3.0,
        };
        let camera = Camera::default();
        let quilt = Quilt::new(config, &camera);

        // View 0: bottom-left (col 0, row 0)
        assert_eq!(quilt.views[0].x_offset, 0);
        assert_eq!(quilt.views[0].y_offset, 0);

        // View 3: bottom-right (col 3, row 0)
        assert_eq!(quilt.views[3].x_offset, 300);
        assert_eq!(quilt.views[3].y_offset, 0);

        // View 4: second row, first column (col 0, row 1)
        assert_eq!(quilt.views[4].x_offset, 0);
        assert_eq!(quilt.views[4].y_offset, 100);

        // View 7: top-right (col 3, row 1)
        assert_eq!(quilt.views[7].x_offset, 300);
        assert_eq!(quilt.views[7].y_offset, 100);
    }

    #[test]
    fn test_quilt_view_cameras_differ() {
        let config = HolographicConfig::looking_glass_portrait();
        let camera = Camera::default();
        let quilt = Quilt::new(config, &camera);

        // Views should have different camera positions
        let view_0 = &quilt.views[0];
        let view_44 = &quilt.views[44]; // Last view (0-44 for 45 views)

        // The x positions should be different (camera arc)
        assert!(
            (view_0.camera.position.x - view_44.camera.position.x).abs() > 0.01,
            "Camera positions should differ between first and last view"
        );
    }

    #[test]
    fn test_quilt_aspect_ratio() {
        let config = HolographicConfig {
            num_views: 4,
            quilt_columns: 2,
            quilt_rows: 2,
            view_width: 160,
            view_height: 90,
            view_cone: 40.0_f32.to_radians(),
            focal_distance: 3.0,
        };
        let camera = Camera::default();
        let quilt = Quilt::new(config, &camera);

        // View aspect: 160/90 ≈ 1.78 (16:9)
        let view_aspect = quilt.view_aspect_ratio();
        assert!((view_aspect - 1.777_778).abs() < 0.001);

        // Quilt is 2x2, same aspect as view
        let quilt_aspect = quilt.quilt_aspect_ratio();
        assert!((quilt_aspect - 1.777_778).abs() < 0.001);
    }

    #[test]
    fn test_render_target_creation() {
        let target = QuiltRenderTarget::new(100, 100);

        assert_eq!(target.width, 100);
        assert_eq!(target.height, 100);
        assert_eq!(target.pixels.len(), 100 * 100 * 4);
    }

    #[test]
    fn test_render_target_clear() {
        let mut target = QuiltRenderTarget::new(10, 10);
        target.clear([255, 0, 0, 255]); // Red

        let pixel = target.get_pixel(5, 5).expect("should get pixel");
        assert_eq!(pixel, [255, 0, 0, 255]);
    }

    #[test]
    fn test_render_target_fill_rect() {
        let mut target = QuiltRenderTarget::new(10, 10);
        target.clear([0, 0, 0, 255]); // Black

        // Fill a 3x3 region with green
        target.fill_rect(2, 2, 3, 3, [0, 255, 0, 255]);

        // Inside the rect
        let pixel = target.get_pixel(3, 3).expect("should get pixel");
        assert_eq!(pixel, [0, 255, 0, 255]);

        // Outside the rect
        let pixel = target.get_pixel(0, 0).expect("should get pixel");
        assert_eq!(pixel, [0, 0, 0, 255]);
    }

    #[test]
    fn test_render_target_from_quilt() {
        let config = HolographicConfig::looking_glass_portrait();
        let camera = Camera::default();
        let quilt = Quilt::new(config, &camera);
        let target = QuiltRenderTarget::from_quilt(&quilt);

        assert_eq!(target.width, quilt.total_width);
        assert_eq!(target.height, quilt.total_height);
    }

    #[test]
    fn test_render_target_boundary_fill() {
        let mut target = QuiltRenderTarget::new(10, 10);

        // Fill beyond boundaries should not panic
        target.fill_rect(8, 8, 10, 10, [255, 255, 255, 255]);

        // Corner should be filled
        let pixel = target.get_pixel(9, 9).expect("should get pixel");
        assert_eq!(pixel, [255, 255, 255, 255]);

        // Out of bounds should return None
        assert!(target.get_pixel(10, 10).is_none());
    }

    #[test]
    fn test_render_settings_default() {
        let settings = QuiltRenderSettings::default();

        let expected = [0.1_f32, 0.1, 0.1, 1.0];
        for (i, (&actual, &exp)) in settings.clear_color.iter().zip(expected.iter()).enumerate() {
            assert!((actual - exp).abs() < f32::EPSILON, "clear_color[{i}] mismatch");
        }
        assert!(!settings.wireframe);
        assert!(settings.depth_test);
        assert!(settings.backface_cull);
    }

    #[test]
    fn test_looking_glass_preset_portrait() {
        let preset = LookingGlassPreset::Portrait;
        let config = preset.config();

        assert_eq!(config.num_views, 45);
        assert_eq!(config.quilt_columns, 5);
        assert_eq!(config.quilt_rows, 9);
    }

    #[test]
    fn test_looking_glass_preset_resolutions() {
        assert_eq!(LookingGlassPreset::Portrait.display_resolution(), (1536, 2048));
        assert_eq!(LookingGlassPreset::LG16.display_resolution(), (3840, 2160));
        assert_eq!(LookingGlassPreset::LG32.display_resolution(), (7680, 4320));
        assert_eq!(LookingGlassPreset::Go.display_resolution(), (1440, 2560));
    }

    #[test]
    fn test_looking_glass_preset_names() {
        assert_eq!(LookingGlassPreset::Portrait.name(), "Looking Glass Portrait");
        assert_eq!(LookingGlassPreset::LG16.name(), "Looking Glass 16\"");
        assert_eq!(LookingGlassPreset::LG32.name(), "Looking Glass 32\"");
        assert_eq!(LookingGlassPreset::Go.name(), "Looking Glass Go");
    }

    #[test]
    fn test_quilt_view_sizes() {
        let config = HolographicConfig {
            num_views: 4,
            quilt_columns: 2,
            quilt_rows: 2,
            view_width: 200,
            view_height: 150,
            view_cone: 40.0_f32.to_radians(),
            focal_distance: 3.0,
        };
        let camera = Camera::default();
        let quilt = Quilt::new(config, &camera);

        for view in &quilt.views {
            assert_eq!(view.width, 200);
            assert_eq!(view.height, 150);
        }
    }

    #[test]
    fn test_quilt_view_get() {
        let config = HolographicConfig::looking_glass_portrait();
        let camera = Camera::default();
        let quilt = Quilt::new(config, &camera);

        assert!(quilt.view(0).is_some());
        assert!(quilt.view(44).is_some()); // Last valid view
        assert!(quilt.view(45).is_none()); // Out of bounds
    }

    #[test]
    fn test_render_target_small() {
        // Edge case: very small render target
        let mut target = QuiltRenderTarget::new(1, 1);
        target.clear([42, 42, 42, 255]);

        let pixel = target.get_pixel(0, 0).expect("should get pixel");
        assert_eq!(pixel, [42, 42, 42, 255]);
    }
}
