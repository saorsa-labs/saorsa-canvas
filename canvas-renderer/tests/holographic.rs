//! Integration tests for holographic rendering.
//!
//! These tests verify the complete pipeline from scene to quilt rendering,
//! ensuring proper integration between spatial, quilt, and holographic modules.

use canvas_core::{Element, ElementKind, Scene};
use canvas_renderer::holographic::HolographicRenderer;
use canvas_renderer::quilt::QuiltRenderSettings;
use canvas_renderer::spatial::{Camera, HolographicConfig, Vec3};

/// Test configuration for reproducible tests.
fn test_config_small() -> HolographicConfig {
    HolographicConfig {
        num_views: 4,
        quilt_columns: 2,
        quilt_rows: 2,
        view_width: 50,
        view_height: 50,
        view_cone: 40.0_f32.to_radians(),
        focal_distance: 2.0,
    }
}

/// Test configuration matching Portrait preset dimensions.
fn test_config_portrait() -> HolographicConfig {
    HolographicConfig::looking_glass_portrait()
}

// ============================================================================
// Integration Tests: Scene -> Quilt -> Image data
// ============================================================================

#[test]
fn integration_scene_to_quilt_basic() {
    let config = test_config_small();
    let mut renderer = HolographicRenderer::new(config.clone());
    let scene = Scene::new(800.0, 600.0);
    let camera = Camera::default();

    let result = renderer.render_quilt(&scene, &camera);

    // Verify result structure
    assert_eq!(result.view_count, config.num_views);
    assert!(result.render_time_ms >= 0.0);
    assert_eq!(result.target.width, config.quilt_width());
    assert_eq!(result.target.height, config.quilt_height());
}

#[test]
fn integration_scene_to_quilt_with_elements() {
    let config = test_config_small();
    let mut renderer = HolographicRenderer::new(config);
    let mut scene = Scene::new(800.0, 600.0);

    // Add a text element
    let text_element = Element::new(ElementKind::Text {
        content: "Test".to_string(),
        font_size: 24.0,
        color: "#FFFFFF".to_string(),
    });
    scene.add_element(text_element);

    // Add an image element (simple test pattern)
    let image_element = Element::new(ElementKind::Image {
        src: "data:image/png;base64,".to_string(),
        format: canvas_core::ImageFormat::Png,
    });
    scene.add_element(image_element);

    let camera = Camera::default();
    let result = renderer.render_quilt(&scene, &camera);

    // Verify rendering completed
    assert_eq!(result.view_count, 4);
    assert!(!result.target.pixels.is_empty());
}

#[test]
fn integration_multiple_renders_accumulate_stats() {
    let config = test_config_small();
    let mut renderer = HolographicRenderer::new(config);
    let scene = Scene::new(800.0, 600.0);
    let camera = Camera::default();

    // Render multiple times
    for _ in 0..5 {
        renderer.render_quilt(&scene, &camera);
    }

    let stats = renderer.stats();
    assert_eq!(stats.frames_rendered, 5);
    assert_eq!(stats.total_views_rendered, 20); // 4 views × 5 frames
    assert!(stats.avg_render_time_ms > 0.0);
}

// ============================================================================
// Quilt Dimension Tests
// ============================================================================

#[test]
fn quilt_dimensions_match_config_small() {
    let config = test_config_small();
    let renderer = HolographicRenderer::new(config.clone());

    let (width, height) = renderer.quilt_dimensions();

    assert_eq!(width, config.view_width * config.quilt_columns);
    assert_eq!(height, config.view_height * config.quilt_rows);
    assert_eq!(width, 100); // 50 × 2
    assert_eq!(height, 100); // 50 × 2
}

#[test]
fn quilt_dimensions_match_config_portrait() {
    let config = test_config_portrait();
    let renderer = HolographicRenderer::new(config);

    let (width, height) = renderer.quilt_dimensions();

    // Portrait: 5 cols × 420px = 2100, 9 rows × 560px = 5040
    assert_eq!(width, 2100);
    assert_eq!(height, 5040);
}

#[test]
fn quilt_dimensions_match_config_4k() {
    let config = HolographicConfig::looking_glass_4k();
    let renderer = HolographicRenderer::new(config);

    let (width, height) = renderer.quilt_dimensions();

    // 4K: 5 cols × 819px = 4095, 9 rows × 455px = 4095
    assert_eq!(width, 4095);
    assert_eq!(height, 4095);
}

// ============================================================================
// Viewport Size Tests
// ============================================================================

#[test]
fn each_view_viewport_correct_size() {
    let config = test_config_small();
    let mut renderer = HolographicRenderer::new(config.clone());
    let scene = Scene::new(800.0, 600.0);
    let camera = Camera::default();

    let result = renderer.render_quilt(&scene, &camera);

    // Verify total pixel count matches expected quilt dimensions
    let total_pixels = result.target.pixels.len();

    assert_eq!(
        total_pixels,
        (config.view_width * config.quilt_columns * config.view_height * config.quilt_rows * 4)
            as usize
    );
}

#[test]
fn view_positions_are_correct() {
    let config = test_config_small();
    let quilt = canvas_renderer::quilt::Quilt::new(config.clone(), &Camera::default());

    // Verify view positions in the quilt grid
    // Views should be laid out left-to-right, bottom-to-top
    for (i, view) in quilt.views.iter().enumerate() {
        let col = i as u32 % config.quilt_columns;
        let row = i as u32 / config.quilt_columns;

        assert_eq!(view.x_offset, col * config.view_width);
        assert_eq!(view.y_offset, row * config.view_height);
        assert_eq!(view.width, config.view_width);
        assert_eq!(view.height, config.view_height);
    }
}

// ============================================================================
// Visual Regression Tests
// ============================================================================

#[test]
fn visual_regression_gradient_pattern() {
    // Test that the placeholder rendering produces expected gradient pattern
    let config = test_config_small();
    let mut renderer = HolographicRenderer::new(config.clone());
    let scene = Scene::new(800.0, 600.0);
    let camera = Camera::default();

    let result = renderer.render_quilt(&scene, &camera);

    // View 0 (leftmost) should be predominantly blue
    let view0_center_x = config.view_width / 2;
    let view0_center_y = config.view_height / 2;
    let pixel0 = result
        .target
        .get_pixel(view0_center_x, view0_center_y)
        .expect("should get pixel from view 0");

    // View 0 (index 0 of 4) should have more blue than red
    assert!(pixel0[2] > pixel0[0], "View 0 should be more blue than red");

    // View 3 (rightmost) should be predominantly red
    let view3_x = config.view_width + config.view_width / 2; // Second column
    let view3_y = config.view_height + config.view_height / 2; // Second row
    let pixel3 = result
        .target
        .get_pixel(view3_x, view3_y)
        .expect("should get pixel from view 3");

    // View 3 (index 3 of 4) should have more red than blue
    assert!(pixel3[0] > pixel3[2], "View 3 should be more red than blue");
}

#[test]
fn visual_regression_view_borders() {
    // Test that view borders are rendered
    let config = test_config_small();
    let mut renderer = HolographicRenderer::new(config.clone());
    let scene = Scene::new(800.0, 600.0);
    let camera = Camera::default();

    let result = renderer.render_quilt(&scene, &camera);

    // Check for border color at edge of view 0
    // Border should be white (255, 255, 255, 128)
    let border_pixel = result
        .target
        .get_pixel(0, 0)
        .expect("should get border pixel");

    // Border pixels should have high RGB values (white tint)
    assert!(border_pixel[0] > 200, "Border should have high red");
    assert!(border_pixel[1] > 200, "Border should have high green");
    assert!(border_pixel[2] > 200, "Border should have high blue");
}

#[test]
fn visual_regression_consistent_output() {
    // Test that rendering is deterministic
    let config = test_config_small();
    let scene = Scene::new(800.0, 600.0);
    let camera = Camera::default();

    let mut renderer1 = HolographicRenderer::new(config.clone());
    let result1 = renderer1.render_quilt(&scene, &camera);

    let mut renderer2 = HolographicRenderer::new(config);
    let result2 = renderer2.render_quilt(&scene, &camera);

    // Results should be identical
    assert_eq!(result1.target.pixels, result2.target.pixels);
}

// ============================================================================
// Performance Tests
// ============================================================================

#[test]
fn performance_45_view_render_completes() {
    // Test that rendering 45 views completes in reasonable time
    let config = test_config_portrait();
    let mut renderer = HolographicRenderer::new(config);
    let scene = Scene::new(800.0, 600.0);
    let camera = Camera::default();

    let start = std::time::Instant::now();
    let result = renderer.render_quilt(&scene, &camera);
    let elapsed = start.elapsed();

    assert_eq!(result.view_count, 45);

    // Should complete in under 1 second for software renderer
    assert!(
        elapsed.as_secs_f64() < 1.0,
        "45-view render took too long: {:?}",
        elapsed
    );

    // Log performance for documentation
    println!(
        "Performance: 45-view quilt rendered in {:.2}ms",
        result.render_time_ms
    );
}

#[test]
fn performance_multiple_frames() {
    // Test sustained rendering performance
    let config = test_config_small();
    let mut renderer = HolographicRenderer::new(config);
    let scene = Scene::new(800.0, 600.0);
    let camera = Camera::default();

    let start = std::time::Instant::now();
    for _ in 0..10 {
        renderer.render_quilt(&scene, &camera);
    }
    let elapsed = start.elapsed();

    let stats = renderer.stats();

    println!(
        "Performance: 10 frames in {:?}, avg {:.2}ms/frame, peak {:.2}ms",
        elapsed, stats.avg_render_time_ms, stats.peak_render_time_ms
    );

    // Should maintain reasonable performance
    assert!(stats.avg_render_time_ms < 100.0);
}

// ============================================================================
// Camera View Tests
// ============================================================================

#[test]
fn camera_views_span_view_cone() {
    let config = test_config_portrait();
    let base_camera = Camera::default();
    let quilt = canvas_renderer::quilt::Quilt::new(config.clone(), &base_camera);

    // First and last view cameras should be at opposite ends of view cone
    let first_view = &quilt.views[0];
    let last_view = &quilt.views[config.num_views as usize - 1];

    // Cameras should have different positions
    assert_ne!(
        first_view.camera.position, last_view.camera.position,
        "First and last views should have different camera positions"
    );

    // All cameras should look at the same focal target
    let focal_target = base_camera.target;
    for view in &quilt.views {
        // Verify cameras are pointed toward the focal target
        // (exact comparison depends on camera implementation)
        let cam_target = view.camera.target;
        let distance = cam_target.sub(&focal_target).length();
        assert!(
            distance < 0.001,
            "Camera {} should look at focal target",
            view.index
        );
    }
}

// ============================================================================
// Settings Tests
// ============================================================================

#[test]
fn custom_settings_applied() {
    let config = test_config_small();
    let settings = QuiltRenderSettings {
        clear_color: [1.0, 0.0, 0.0, 1.0], // Red clear color
        wireframe: true,
        depth_test: false,
        backface_cull: false,
    };

    let renderer = HolographicRenderer::with_settings(config, settings);

    assert!(renderer.settings().wireframe);
    assert!(!renderer.settings().depth_test);
    assert_eq!(renderer.settings().clear_color[0], 1.0);
}

#[test]
fn config_can_be_updated() {
    let config1 = test_config_small();
    let config2 = test_config_portrait();

    let mut renderer = HolographicRenderer::new(config1);
    assert_eq!(renderer.config().num_views, 4);

    renderer.set_config(config2);
    assert_eq!(renderer.config().num_views, 45);
}

// ============================================================================
// Edge Cases
// ============================================================================

#[test]
fn render_empty_scene() {
    let config = test_config_small();
    let mut renderer = HolographicRenderer::new(config);
    let scene = Scene::new(800.0, 600.0);
    let camera = Camera::default();

    // Should not panic on empty scene
    let result = renderer.render_quilt(&scene, &camera);
    assert!(!result.target.pixels.is_empty());
}

#[test]
fn render_with_zero_position_camera() {
    let config = test_config_small();
    let mut renderer = HolographicRenderer::new(config);
    let scene = Scene::new(800.0, 600.0);
    let camera = Camera {
        position: Vec3::new(0.0, 0.0, 0.0),
        target: Vec3::new(0.0, 0.0, -1.0),
        up: Vec3::new(0.0, 1.0, 0.0),
        ..Camera::default()
    };

    let result = renderer.render_quilt(&scene, &camera);
    assert!(result.view_count > 0);
}

#[test]
fn stats_reset_works() {
    let config = test_config_small();
    let mut renderer = HolographicRenderer::new(config);
    let scene = Scene::new(800.0, 600.0);
    let camera = Camera::default();

    // Render a few times
    for _ in 0..3 {
        renderer.render_quilt(&scene, &camera);
    }
    assert_eq!(renderer.stats().frames_rendered, 3);

    // Reset stats
    renderer.reset_stats();
    assert_eq!(renderer.stats().frames_rendered, 0);
    assert!(renderer.stats().avg_render_time_ms.abs() < f64::EPSILON);
}
