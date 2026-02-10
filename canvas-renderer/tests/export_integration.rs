//! Integration tests for scene export (canvas-renderer).
//!
//! Tests export across multiple formats, large scenes, custom configurations,
//! and edge cases.

use canvas_core::element::{Element, ElementKind, Transform};
use canvas_core::Scene;
use canvas_renderer::export::{ExportConfig, ExportFormat, SceneExporter};

/// Create a text element at a given position.
fn text_element(content: &str, x: f32, y: f32) -> Element {
    Element::new(ElementKind::Text {
        content: content.to_string(),
        font_size: 16.0,
        color: "#000000".to_string(),
    })
    .with_transform(Transform {
        x,
        y,
        width: 200.0,
        height: 30.0,
        rotation: 0.0,
        z_index: 0,
    })
}

/// Create a chart element.
fn bar_chart_element(x: f32, y: f32) -> Element {
    Element::new(ElementKind::Chart {
        chart_type: "bar".to_string(),
        data: serde_json::json!({
            "labels": ["Q1", "Q2", "Q3", "Q4"],
            "values": [120, 250, 180, 300]
        }),
    })
    .with_transform(Transform {
        x,
        y,
        width: 400.0,
        height: 300.0,
        rotation: 0.0,
        z_index: 1,
    })
}

// ==========================================================================
// Large scene tests
// ==========================================================================

#[test]
fn test_large_scene_png_export() {
    let mut scene = Scene::new(800.0, 2000.0);
    for i in 0..100 {
        #[allow(clippy::cast_precision_loss)]
        let y = (i as f32) * 20.0;
        scene.add_element(text_element(&format!("Element {i}"), 10.0, y));
    }

    let exporter = SceneExporter::with_defaults();
    let png = exporter.export(&scene, ExportFormat::Png).expect("png");
    assert_eq!(&png[0..4], &[137, 80, 78, 71]);
    // Large scene should produce a reasonably sized PNG
    assert!(png.len() > 1000, "Expected > 1KB, got {} bytes", png.len());
}

#[test]
fn test_large_scene_svg_export() {
    let mut scene = Scene::new(800.0, 2000.0);
    for i in 0..100 {
        #[allow(clippy::cast_precision_loss)]
        let y = (i as f32) * 20.0;
        scene.add_element(text_element(&format!("Element {i}"), 10.0, y));
    }

    let exporter = SceneExporter::with_defaults();
    let svg_bytes = exporter.export(&scene, ExportFormat::Svg).expect("svg");
    let svg = String::from_utf8(svg_bytes).expect("utf8");
    assert!(svg.starts_with("<svg"));
    assert!(svg.contains("Element 0"));
    assert!(svg.contains("Element 99"));
}

// ==========================================================================
// Mixed content scenes
// ==========================================================================

#[test]
fn test_mixed_content_scene_png() {
    let mut scene = Scene::new(800.0, 600.0);
    scene.add_element(text_element("Title", 10.0, 20.0));
    scene.add_element(bar_chart_element(10.0, 60.0));
    scene.add_element(text_element("Footer", 10.0, 370.0));

    let exporter = SceneExporter::with_defaults();
    let png = exporter.export(&scene, ExportFormat::Png).expect("png");
    assert_eq!(&png[0..4], &[137, 80, 78, 71]);
}

#[test]
fn test_mixed_content_scene_svg() {
    let mut scene = Scene::new(800.0, 600.0);
    scene.add_element(text_element("Title", 10.0, 20.0));
    scene.add_element(bar_chart_element(10.0, 60.0));
    scene.add_element(text_element("Footer", 10.0, 370.0));

    let exporter = SceneExporter::with_defaults();
    let svg_bytes = exporter.export(&scene, ExportFormat::Svg).expect("svg");
    let svg = String::from_utf8(svg_bytes).expect("utf8");
    assert!(svg.contains("Title"));
    assert!(svg.contains("bar chart"));
    assert!(svg.contains("Footer"));
}

// ==========================================================================
// All formats produce output for same scene
// ==========================================================================

#[test]
fn test_all_formats_for_same_scene() {
    let mut scene = Scene::new(400.0, 300.0);
    scene.add_element(text_element("Export test", 10.0, 20.0));
    scene.add_element(bar_chart_element(10.0, 50.0));

    let exporter = SceneExporter::with_defaults();

    // PNG
    let png = exporter.export(&scene, ExportFormat::Png).expect("png");
    assert_eq!(&png[0..4], &[137, 80, 78, 71]);

    // JPEG
    let jpeg = exporter.export(&scene, ExportFormat::Jpeg).expect("jpeg");
    assert_eq!(jpeg[0], 0xFF);
    assert_eq!(jpeg[1], 0xD8);

    // SVG
    let svg = exporter.export(&scene, ExportFormat::Svg).expect("svg");
    let svg_str = String::from_utf8(svg).expect("utf8");
    assert!(svg_str.starts_with("<svg"));

    // PDF
    let pdf = exporter.export(&scene, ExportFormat::Pdf).expect("pdf");
    assert_eq!(&pdf[0..5], b"%PDF-");
}

// ==========================================================================
// Custom configuration
// ==========================================================================

#[test]
fn test_custom_dpi_and_quality() {
    let mut scene = Scene::new(200.0, 200.0);
    scene.add_element(text_element("High DPI", 10.0, 20.0));

    let exporter = SceneExporter::new(ExportConfig {
        dpi: 300.0,
        jpeg_quality: 50,
        ..Default::default()
    });

    // JPEG at quality 50 should be smaller than quality 85
    let low_q = exporter.export(&scene, ExportFormat::Jpeg).expect("jpeg");
    assert_eq!(low_q[0], 0xFF);

    let high_exporter = SceneExporter::new(ExportConfig {
        dpi: 300.0,
        jpeg_quality: 95,
        ..Default::default()
    });
    let high_q = high_exporter
        .export(&scene, ExportFormat::Jpeg)
        .expect("jpeg");
    assert_eq!(high_q[0], 0xFF);

    // Higher quality should generally produce larger files
    assert!(
        high_q.len() >= low_q.len(),
        "Expected high-quality ({}) >= low-quality ({})",
        high_q.len(),
        low_q.len()
    );
}

#[test]
fn test_custom_background_color() {
    let scene = Scene::new(100.0, 100.0);

    // Black background
    let exporter = SceneExporter::new(ExportConfig {
        background: [0, 0, 0, 255],
        ..Default::default()
    });

    let svg_bytes = exporter.export(&scene, ExportFormat::Svg).expect("svg");
    let svg = String::from_utf8(svg_bytes).expect("utf8");
    assert!(svg.contains("rgba(0,0,0,1)"));
}

#[test]
fn test_transparent_background() {
    let scene = Scene::new(100.0, 100.0);

    let exporter = SceneExporter::new(ExportConfig {
        background: [0, 0, 0, 0],
        ..Default::default()
    });

    let svg_bytes = exporter.export(&scene, ExportFormat::Svg).expect("svg");
    let svg = String::from_utf8(svg_bytes).expect("utf8");
    assert!(svg.contains("rgba(0,0,0,0)"));
}

// ==========================================================================
// Edge cases
// ==========================================================================

#[test]
fn test_empty_scene_all_formats() {
    let scene = Scene::new(100.0, 100.0);
    let exporter = SceneExporter::with_defaults();

    let png = exporter.export(&scene, ExportFormat::Png).expect("png");
    assert!(!png.is_empty());

    let jpeg = exporter.export(&scene, ExportFormat::Jpeg).expect("jpeg");
    assert!(!jpeg.is_empty());

    let svg = exporter.export(&scene, ExportFormat::Svg).expect("svg");
    assert!(!svg.is_empty());

    let pdf = exporter.export(&scene, ExportFormat::Pdf).expect("pdf");
    assert!(!pdf.is_empty());
}

#[test]
fn test_tiny_scene_dimensions() {
    let mut scene = Scene::new(1.0, 1.0);
    scene.add_element(text_element("Tiny", 0.0, 0.0));

    let exporter = SceneExporter::with_defaults();
    let png = exporter.export(&scene, ExportFormat::Png).expect("png");
    assert_eq!(&png[0..4], &[137, 80, 78, 71]);
}

#[test]
fn test_special_characters_in_text() {
    let mut scene = Scene::new(400.0, 100.0);
    scene.add_element(text_element("Hello <world> & \"friends\"", 10.0, 20.0));

    let exporter = SceneExporter::with_defaults();

    // SVG should escape special characters
    let svg_bytes = exporter.export(&scene, ExportFormat::Svg).expect("svg");
    let svg = String::from_utf8(svg_bytes).expect("utf8");
    assert!(svg.contains("&lt;world&gt;"));
    assert!(svg.contains("&amp;"));
    assert!(svg.contains("&quot;friends&quot;"));

    // PNG should still render without error
    let png = exporter.export(&scene, ExportFormat::Png).expect("png");
    assert_eq!(&png[0..4], &[137, 80, 78, 71]);
}
