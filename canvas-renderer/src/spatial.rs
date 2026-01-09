//! Spatial rendering for holographic and 3D displays.
//!
//! This module provides camera and view calculations needed for
//! multi-view rendering on holographic displays like Looking Glass.
//!
//! ## Holographic Rendering Concepts
//!
//! ```text
//!                     Looking Glass Display
//!                    ┌─────────────────────┐
//!                    │  ╱   ╱   ╱   ╱   ╱  │
//!                    │ ╱   ╱   ╱   ╱   ╱   │  Lenticular lens array
//!                    │╱   ╱   ╱   ╱   ╱    │  directs different views
//!                    └─────────────────────┘  to each eye
//!
//!          Camera Arc (45 views)
//!         ╭─────────────────────╮
//!        ╱  ╲                   ╲
//!       ●    ●    ●    ●    ●    ●   ← Camera positions
//!       0    8   16   24   32   44
//!              ↓
//!         ┌─────────────────┐
//!         │ Scene to render │
//!         └─────────────────┘
//! ```
//!
//! ## Quilt Format
//!
//! Multiple views are rendered into a single "quilt" texture:
//!
//! ```text
//! ┌────┬────┬────┬────┬────┐
//! │ 40 │ 41 │ 42 │ 43 │ 44 │
//! ├────┼────┼────┼────┼────┤
//! │ 35 │ 36 │ 37 │ 38 │ 39 │
//! ├────┼────┼────┼────┼────┤
//! │... │... │... │... │... │
//! ├────┼────┼────┼────┼────┤
//! │  0 │  1 │  2 │  3 │  4 │
//! └────┴────┴────┴────┴────┘
//!        Quilt (5x9 grid = 45 views)
//! ```

use serde::{Deserialize, Serialize};

/// A 3D vector for positions and directions.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Vec3 {
    /// X component.
    pub x: f32,
    /// Y component.
    pub y: f32,
    /// Z component.
    pub z: f32,
}

impl Vec3 {
    /// Create a new vector.
    #[must_use]
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    /// Zero vector.
    #[must_use]
    pub const fn zero() -> Self {
        Self::new(0.0, 0.0, 0.0)
    }

    /// Unit vector pointing up (Y+).
    #[must_use]
    pub const fn up() -> Self {
        Self::new(0.0, 1.0, 0.0)
    }

    /// Unit vector pointing forward (Z-).
    #[must_use]
    pub const fn forward() -> Self {
        Self::new(0.0, 0.0, -1.0)
    }

    /// Calculate the length (magnitude) of the vector.
    #[must_use]
    pub fn length(&self) -> f32 {
        (self.x * self.x + self.y * self.y + self.z * self.z).sqrt()
    }

    /// Normalize the vector to unit length.
    #[must_use]
    pub fn normalize(&self) -> Self {
        let len = self.length();
        if len > 0.0 {
            Self::new(self.x / len, self.y / len, self.z / len)
        } else {
            *self
        }
    }

    /// Cross product of two vectors.
    #[must_use]
    pub fn cross(&self, other: &Self) -> Self {
        Self::new(
            self.y * other.z - self.z * other.y,
            self.z * other.x - self.x * other.z,
            self.x * other.y - self.y * other.x,
        )
    }

    /// Dot product of two vectors.
    #[must_use]
    pub fn dot(&self, other: &Self) -> f32 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }

    /// Subtract two vectors.
    #[must_use]
    pub fn sub(&self, other: &Self) -> Self {
        Self::new(self.x - other.x, self.y - other.y, self.z - other.z)
    }

    /// Add two vectors.
    #[must_use]
    pub fn add(&self, other: &Self) -> Self {
        Self::new(self.x + other.x, self.y + other.y, self.z + other.z)
    }

    /// Scale vector by a scalar.
    #[must_use]
    pub fn scale(&self, s: f32) -> Self {
        Self::new(self.x * s, self.y * s, self.z * s)
    }
}

impl Default for Vec3 {
    fn default() -> Self {
        Self::zero()
    }
}

/// A 4x4 matrix for transformations.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mat4 {
    /// Matrix data in column-major order.
    pub data: [f32; 16],
}

impl Mat4 {
    /// Create identity matrix.
    #[must_use]
    pub fn identity() -> Self {
        #[rustfmt::skip]
        let data = [
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ];
        Self { data }
    }

    /// Create a look-at view matrix.
    #[must_use]
    pub fn look_at(eye: Vec3, target: Vec3, up: Vec3) -> Self {
        let f = target.sub(&eye).normalize();
        let s = f.cross(&up).normalize();
        let u = s.cross(&f);

        #[rustfmt::skip]
        let data = [
            s.x,  u.x,  -f.x, 0.0,
            s.y,  u.y,  -f.y, 0.0,
            s.z,  u.z,  -f.z, 0.0,
            -s.dot(&eye), -u.dot(&eye), f.dot(&eye), 1.0,
        ];
        Self { data }
    }

    /// Create a perspective projection matrix.
    #[must_use]
    pub fn perspective(fov_y_radians: f32, aspect: f32, near: f32, far: f32) -> Self {
        let f = 1.0 / (fov_y_radians / 2.0).tan();
        let nf = 1.0 / (near - far);

        #[rustfmt::skip]
        let data = [
            f / aspect, 0.0, 0.0, 0.0,
            0.0, f, 0.0, 0.0,
            0.0, 0.0, (far + near) * nf, -1.0,
            0.0, 0.0, 2.0 * far * near * nf, 0.0,
        ];
        Self { data }
    }

    /// Multiply two matrices.
    #[must_use]
    pub fn mul(&self, other: &Self) -> Self {
        let mut result = [0.0f32; 16];

        for row in 0..4 {
            for col in 0..4 {
                for k in 0..4 {
                    result[col * 4 + row] +=
                        self.data[k * 4 + row] * other.data[col * 4 + k];
                }
            }
        }

        Self { data: result }
    }
}

impl Default for Mat4 {
    fn default() -> Self {
        Self::identity()
    }
}

/// Camera for 3D rendering.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Camera {
    /// Camera position in world space.
    pub position: Vec3,
    /// Point the camera is looking at.
    pub target: Vec3,
    /// Up direction (usually Y+).
    pub up: Vec3,
    /// Field of view in radians.
    pub fov: f32,
    /// Near clipping plane.
    pub near: f32,
    /// Far clipping plane.
    pub far: f32,
}

impl Camera {
    /// Create a new camera with default settings.
    #[must_use]
    pub fn new() -> Self {
        Self {
            position: Vec3::new(0.0, 0.0, 5.0),
            target: Vec3::zero(),
            up: Vec3::up(),
            fov: std::f32::consts::FRAC_PI_4, // 45 degrees
            near: 0.1,
            far: 100.0,
        }
    }

    /// Create view matrix for this camera.
    #[must_use]
    pub fn view_matrix(&self) -> Mat4 {
        Mat4::look_at(self.position, self.target, self.up)
    }

    /// Create projection matrix for this camera.
    #[must_use]
    pub fn projection_matrix(&self, aspect: f32) -> Mat4 {
        Mat4::perspective(self.fov, aspect, self.near, self.far)
    }
}

impl Default for Camera {
    fn default() -> Self {
        Self::new()
    }
}

/// Configuration for holographic multi-view rendering.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HolographicConfig {
    /// Number of views to render (typically 45 for Looking Glass).
    pub num_views: u32,
    /// Number of columns in the quilt grid.
    pub quilt_columns: u32,
    /// Number of rows in the quilt grid.
    pub quilt_rows: u32,
    /// Width of each view in pixels.
    pub view_width: u32,
    /// Height of each view in pixels.
    pub view_height: u32,
    /// Total viewing angle in radians (how much the camera sweeps).
    pub view_cone: f32,
    /// Distance from camera to focal plane.
    pub focal_distance: f32,
}

impl HolographicConfig {
    /// Standard Looking Glass Portrait configuration (45 views, 5x9 quilt).
    #[must_use]
    pub fn looking_glass_portrait() -> Self {
        Self {
            num_views: 45,
            quilt_columns: 5,
            quilt_rows: 9,
            view_width: 420,
            view_height: 560,
            view_cone: 40.0_f32.to_radians(), // 40 degrees total
            focal_distance: 2.0,
        }
    }

    /// Standard Looking Glass 4K configuration (45 views, 5x9 quilt).
    #[must_use]
    pub fn looking_glass_4k() -> Self {
        Self {
            num_views: 45,
            quilt_columns: 5,
            quilt_rows: 9,
            view_width: 819,
            view_height: 455,
            view_cone: 40.0_f32.to_radians(),
            focal_distance: 2.0,
        }
    }

    /// Calculate the total quilt texture width.
    #[must_use]
    pub fn quilt_width(&self) -> u32 {
        self.quilt_columns * self.view_width
    }

    /// Calculate the total quilt texture height.
    #[must_use]
    pub fn quilt_height(&self) -> u32 {
        self.quilt_rows * self.view_height
    }

    /// Calculate which row and column a view index maps to.
    #[must_use]
    pub fn view_to_grid(&self, view_index: u32) -> (u32, u32) {
        let col = view_index % self.quilt_columns;
        let row = view_index / self.quilt_columns;
        (col, row)
    }

    /// Calculate the pixel offset for a view in the quilt.
    #[must_use]
    pub fn view_offset(&self, view_index: u32) -> (u32, u32) {
        let (col, row) = self.view_to_grid(view_index);
        (col * self.view_width, row * self.view_height)
    }

    /// Calculate the camera position for a specific view.
    ///
    /// The camera moves along a horizontal arc centered on the target.
    #[must_use]
    #[allow(clippy::cast_precision_loss)] // View indices are small, precision loss negligible
    pub fn camera_for_view(&self, base_camera: &Camera, view_index: u32) -> Camera {
        if self.num_views <= 1 {
            return base_camera.clone();
        }

        // Calculate the angle offset for this view
        // View 0 is leftmost, view (num_views-1) is rightmost
        let t = view_index as f32 / (self.num_views - 1) as f32;
        let angle = (t - 0.5) * self.view_cone;

        // Calculate new camera position by rotating around the target
        let dir = base_camera.position.sub(&base_camera.target);
        let distance = dir.length();

        // Rotate the direction vector around Y axis
        let cos_a = angle.cos();
        let sin_a = angle.sin();
        let new_dir = Vec3::new(
            dir.x * cos_a + dir.z * sin_a,
            dir.y,
            -dir.x * sin_a + dir.z * cos_a,
        );

        Camera {
            position: base_camera.target.add(&new_dir.normalize().scale(distance)),
            target: base_camera.target,
            up: base_camera.up,
            fov: base_camera.fov,
            near: base_camera.near,
            far: base_camera.far,
        }
    }
}

impl Default for HolographicConfig {
    fn default() -> Self {
        Self::looking_glass_portrait()
    }
}

/// Result of rendering a quilt.
#[derive(Debug, Clone)]
pub struct QuiltRenderInfo {
    /// Width of the complete quilt texture.
    pub width: u32,
    /// Height of the complete quilt texture.
    pub height: u32,
    /// Number of views rendered.
    pub num_views: u32,
    /// Columns in the quilt grid.
    pub columns: u32,
    /// Rows in the quilt grid.
    pub rows: u32,
}

impl QuiltRenderInfo {
    /// Create render info from holographic config.
    #[must_use]
    pub fn from_config(config: &HolographicConfig) -> Self {
        Self {
            width: config.quilt_width(),
            height: config.quilt_height(),
            num_views: config.num_views,
            columns: config.quilt_columns,
            rows: config.quilt_rows,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPSILON: f32 = 1e-5;

    fn approx_eq(a: f32, b: f32) -> bool {
        (a - b).abs() < EPSILON
    }

    // ===========================================
    // TDD: Vec3 Tests
    // ===========================================

    #[test]
    fn test_vec3_new() {
        let v = Vec3::new(1.0, 2.0, 3.0);
        assert!(approx_eq(v.x, 1.0));
        assert!(approx_eq(v.y, 2.0));
        assert!(approx_eq(v.z, 3.0));
    }

    #[test]
    fn test_vec3_zero() {
        let v = Vec3::zero();
        assert!(approx_eq(v.x, 0.0));
        assert!(approx_eq(v.y, 0.0));
        assert!(approx_eq(v.z, 0.0));
    }

    #[test]
    fn test_vec3_length() {
        let v = Vec3::new(3.0, 4.0, 0.0);
        assert!(approx_eq(v.length(), 5.0));
    }

    #[test]
    fn test_vec3_normalize() {
        let v = Vec3::new(3.0, 0.0, 0.0);
        let n = v.normalize();
        assert!(approx_eq(n.x, 1.0));
        assert!(approx_eq(n.y, 0.0));
        assert!(approx_eq(n.z, 0.0));
    }

    #[test]
    fn test_vec3_normalize_zero() {
        let v = Vec3::zero();
        let n = v.normalize();
        // Zero vector normalizes to zero
        assert!(approx_eq(n.length(), 0.0));
    }

    #[test]
    fn test_vec3_cross() {
        let x = Vec3::new(1.0, 0.0, 0.0);
        let y = Vec3::new(0.0, 1.0, 0.0);
        let z = x.cross(&y);
        assert!(approx_eq(z.x, 0.0));
        assert!(approx_eq(z.y, 0.0));
        assert!(approx_eq(z.z, 1.0));
    }

    #[test]
    fn test_vec3_dot() {
        let a = Vec3::new(1.0, 2.0, 3.0);
        let b = Vec3::new(4.0, 5.0, 6.0);
        assert!(approx_eq(a.dot(&b), 32.0)); // 1*4 + 2*5 + 3*6 = 32
    }

    #[test]
    fn test_vec3_add_sub() {
        let a = Vec3::new(1.0, 2.0, 3.0);
        let b = Vec3::new(4.0, 5.0, 6.0);
        let sum = a.add(&b);
        let diff = a.sub(&b);

        assert!(approx_eq(sum.x, 5.0));
        assert!(approx_eq(sum.y, 7.0));
        assert!(approx_eq(sum.z, 9.0));

        assert!(approx_eq(diff.x, -3.0));
        assert!(approx_eq(diff.y, -3.0));
        assert!(approx_eq(diff.z, -3.0));
    }

    #[test]
    fn test_vec3_scale() {
        let v = Vec3::new(1.0, 2.0, 3.0);
        let scaled = v.scale(2.0);
        assert!(approx_eq(scaled.x, 2.0));
        assert!(approx_eq(scaled.y, 4.0));
        assert!(approx_eq(scaled.z, 6.0));
    }

    // ===========================================
    // TDD: Mat4 Tests
    // ===========================================

    #[test]
    fn test_mat4_identity() {
        let m = Mat4::identity();
        // Diagonal should be 1s
        assert!(approx_eq(m.data[0], 1.0));
        assert!(approx_eq(m.data[5], 1.0));
        assert!(approx_eq(m.data[10], 1.0));
        assert!(approx_eq(m.data[15], 1.0));
        // Off-diagonal should be 0s
        assert!(approx_eq(m.data[1], 0.0));
        assert!(approx_eq(m.data[4], 0.0));
    }

    #[test]
    fn test_mat4_mul_identity() {
        let m = Mat4::identity();
        let result = m.mul(&m);
        // Identity * Identity = Identity
        assert!(approx_eq(result.data[0], 1.0));
        assert!(approx_eq(result.data[5], 1.0));
        assert!(approx_eq(result.data[10], 1.0));
        assert!(approx_eq(result.data[15], 1.0));
    }

    #[test]
    fn test_mat4_look_at() {
        let eye = Vec3::new(0.0, 0.0, 5.0);
        let target = Vec3::zero();
        let up = Vec3::up();
        let view = Mat4::look_at(eye, target, up);

        // The view matrix should translate -eye when looking at origin
        // The last column should contain the translation
        assert!(view.data[15].abs() - 1.0 < EPSILON);
    }

    #[test]
    fn test_mat4_perspective() {
        let proj = Mat4::perspective(
            std::f32::consts::FRAC_PI_4, // 45 degrees
            1.0,                          // square aspect
            0.1,
            100.0,
        );
        // Perspective matrix should have -1 in [2,3] position (row 3, col 2)
        assert!(approx_eq(proj.data[11], -1.0));
    }

    // ===========================================
    // TDD: Camera Tests
    // ===========================================

    #[test]
    fn test_camera_default() {
        let cam = Camera::new();
        assert!(approx_eq(cam.position.z, 5.0));
        assert!(approx_eq(cam.target.x, 0.0));
        assert!(approx_eq(cam.target.y, 0.0));
        assert!(approx_eq(cam.target.z, 0.0));
    }

    #[test]
    fn test_camera_view_matrix() {
        let cam = Camera::new();
        let view = cam.view_matrix();
        // Should be a valid 4x4 matrix
        assert_eq!(view.data.len(), 16);
    }

    #[test]
    fn test_camera_projection_matrix() {
        let cam = Camera::new();
        let proj = cam.projection_matrix(16.0 / 9.0);
        // Should be a valid 4x4 matrix with perspective division
        assert!(approx_eq(proj.data[11], -1.0));
    }

    // ===========================================
    // TDD: HolographicConfig Tests
    // ===========================================

    #[test]
    fn test_holographic_config_portrait() {
        let config = HolographicConfig::looking_glass_portrait();
        assert_eq!(config.num_views, 45);
        assert_eq!(config.quilt_columns, 5);
        assert_eq!(config.quilt_rows, 9);
        assert_eq!(config.num_views, config.quilt_columns * config.quilt_rows);
    }

    #[test]
    fn test_holographic_config_4k() {
        let config = HolographicConfig::looking_glass_4k();
        assert_eq!(config.num_views, 45);
        assert_eq!(config.quilt_columns, 5);
        assert_eq!(config.quilt_rows, 9);
    }

    #[test]
    fn test_quilt_dimensions() {
        let config = HolographicConfig::looking_glass_portrait();
        let width = config.quilt_width();
        let height = config.quilt_height();

        assert_eq!(width, 5 * 420); // 2100
        assert_eq!(height, 9 * 560); // 5040
    }

    #[test]
    fn test_view_to_grid() {
        let config = HolographicConfig::looking_glass_portrait();

        // View 0 should be at (0, 0)
        let (col, row) = config.view_to_grid(0);
        assert_eq!(col, 0);
        assert_eq!(row, 0);

        // View 4 should be at (4, 0) - last column of first row
        let (col, row) = config.view_to_grid(4);
        assert_eq!(col, 4);
        assert_eq!(row, 0);

        // View 5 should be at (0, 1) - first column of second row
        let (col, row) = config.view_to_grid(5);
        assert_eq!(col, 0);
        assert_eq!(row, 1);

        // View 44 should be at (4, 8) - last view
        let (col, row) = config.view_to_grid(44);
        assert_eq!(col, 4);
        assert_eq!(row, 8);
    }

    #[test]
    fn test_view_offset() {
        let config = HolographicConfig::looking_glass_portrait();

        // View 0 starts at (0, 0)
        let (x, y) = config.view_offset(0);
        assert_eq!(x, 0);
        assert_eq!(y, 0);

        // View 1 starts at (420, 0)
        let (x, y) = config.view_offset(1);
        assert_eq!(x, 420);
        assert_eq!(y, 0);

        // View 5 starts at (0, 560) - second row
        let (x, y) = config.view_offset(5);
        assert_eq!(x, 0);
        assert_eq!(y, 560);
    }

    #[test]
    fn test_camera_for_view_single() {
        let config = HolographicConfig {
            num_views: 1,
            ..Default::default()
        };
        let base = Camera::new();
        let view_cam = config.camera_for_view(&base, 0);

        // Single view should return the same camera
        assert!(approx_eq(view_cam.position.x, base.position.x));
        assert!(approx_eq(view_cam.position.y, base.position.y));
        assert!(approx_eq(view_cam.position.z, base.position.z));
    }

    #[test]
    fn test_camera_for_view_center() {
        let config = HolographicConfig::looking_glass_portrait();
        let base = Camera::new();

        // Middle view (22) should be approximately at the center
        let center_cam = config.camera_for_view(&base, 22);

        // Center camera should have similar position to base
        // (small offset due to discrete view count)
        let distance_from_base =
            center_cam.position.sub(&base.position).length();
        assert!(distance_from_base < 0.5);
    }

    #[test]
    fn test_camera_for_view_symmetry() {
        let config = HolographicConfig::looking_glass_portrait();
        let base = Camera::new();

        // First and last views should be symmetric about center
        let left_cam = config.camera_for_view(&base, 0);
        let right_cam = config.camera_for_view(&base, 44);

        // X positions should be opposite
        assert!(approx_eq(left_cam.position.x, -right_cam.position.x));
        // Y positions should be the same
        assert!(approx_eq(left_cam.position.y, right_cam.position.y));
    }

    #[test]
    fn test_camera_for_view_maintains_distance() {
        let config = HolographicConfig::looking_glass_portrait();
        let base = Camera::new();
        let base_distance = base.position.sub(&base.target).length();

        // All views should maintain the same distance to target
        for i in 0..config.num_views {
            let view_cam = config.camera_for_view(&base, i);
            let view_distance = view_cam.position.sub(&view_cam.target).length();
            assert!(
                approx_eq(view_distance, base_distance),
                "View {i} distance {view_distance} != base {base_distance}"
            );
        }
    }

    #[test]
    fn test_camera_for_view_progression() {
        let config = HolographicConfig::looking_glass_portrait();
        let base = Camera::new();

        // Camera X position should increase from left to right
        let mut prev_x = f32::NEG_INFINITY;
        for i in 0..config.num_views {
            let view_cam = config.camera_for_view(&base, i);
            assert!(
                view_cam.position.x > prev_x,
                "View {} x {} should be > {}",
                i,
                view_cam.position.x,
                prev_x
            );
            prev_x = view_cam.position.x;
        }
    }

    // ===========================================
    // TDD: QuiltRenderInfo Tests
    // ===========================================

    #[test]
    fn test_quilt_render_info_from_config() {
        let config = HolographicConfig::looking_glass_portrait();
        let info = QuiltRenderInfo::from_config(&config);

        assert_eq!(info.width, 2100);
        assert_eq!(info.height, 5040);
        assert_eq!(info.num_views, 45);
        assert_eq!(info.columns, 5);
        assert_eq!(info.rows, 9);
    }
}
