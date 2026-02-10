//! Scene export to image/document formats.
//!
//! Renders a [`Scene`] to PNG, JPEG, SVG, PDF, or WebP using an SVG intermediate
//! representation and the resvg/tiny-skia rasterization pipeline.

use std::fmt::Write;

use canvas_core::element::ElementKind;
use canvas_core::Scene;
use image::ImageEncoder;

use crate::error::{RenderError, RenderResult};

/// Export output format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    /// PNG image.
    Png,
    /// JPEG image.
    Jpeg,
    /// SVG vector graphics (returns the SVG XML string as UTF-8 bytes).
    Svg,
    /// PDF document with embedded raster image.
    Pdf,
}

/// Configuration for scene export.
#[derive(Debug, Clone)]
pub struct ExportConfig {
    /// Output width in pixels (default: scene viewport width).
    pub width: Option<u32>,
    /// Output height in pixels (default: scene viewport height).
    pub height: Option<u32>,
    /// DPI for print export (default: 96.0).
    pub dpi: f32,
    /// Background color as RGBA bytes.
    pub background: [u8; 4],
    /// JPEG quality 1-100 (default: 85).
    pub jpeg_quality: u8,
    /// Scale factor (e.g. 2.0 for retina).
    pub scale: f32,
}

impl Default for ExportConfig {
    fn default() -> Self {
        Self {
            width: None,
            height: None,
            dpi: 96.0,
            background: [255, 255, 255, 255],
            jpeg_quality: 85,
            scale: 1.0,
        }
    }
}

/// Exports a [`Scene`] to various image and document formats.
pub struct SceneExporter {
    config: ExportConfig,
}

impl SceneExporter {
    /// Create a new exporter with the given configuration.
    #[must_use]
    pub fn new(config: ExportConfig) -> Self {
        Self { config }
    }

    /// Create an exporter with default configuration.
    #[must_use]
    pub fn with_defaults() -> Self {
        Self::new(ExportConfig::default())
    }

    /// Export a scene to the specified format.
    ///
    /// # Errors
    ///
    /// Returns an error if the scene cannot be rendered or encoded.
    pub fn export(&self, scene: &Scene, format: ExportFormat) -> RenderResult<Vec<u8>> {
        match format {
            ExportFormat::Png => self.render_to_png(scene),
            ExportFormat::Jpeg => self.render_to_jpeg(scene),
            ExportFormat::Svg => {
                let svg = self.render_to_svg(scene)?;
                Ok(svg.into_bytes())
            }
            ExportFormat::Pdf => self.render_to_pdf(scene),
        }
    }

    /// Export the scene to PNG bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if rendering or encoding fails.
    pub fn render_to_png(&self, scene: &Scene) -> RenderResult<Vec<u8>> {
        let svg_string = self.render_to_svg(scene)?;
        let pixmap = Self::rasterize_svg(&svg_string)?;

        pixmap
            .encode_png()
            .map_err(|e| RenderError::Export(format!("PNG encoding failed: {e}")))
    }

    /// Export the scene to JPEG bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if rendering or encoding fails.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub fn render_to_jpeg(&self, scene: &Scene) -> RenderResult<Vec<u8>> {
        let svg_string = self.render_to_svg(scene)?;
        let pixmap = Self::rasterize_svg(&svg_string)?;

        let (width, height) = (pixmap.width(), pixmap.height());
        let bg = &self.config.background;
        let mut rgb_data = Vec::with_capacity((width * height * 3) as usize);
        for pixel in pixmap.data().chunks_exact(4) {
            let alpha = f32::from(pixel[3]) / 255.0;
            let inv = 1.0 - alpha;
            rgb_data.push((f32::from(pixel[0]).mul_add(alpha, f32::from(bg[0]) * inv)) as u8);
            rgb_data.push((f32::from(pixel[1]).mul_add(alpha, f32::from(bg[1]) * inv)) as u8);
            rgb_data.push((f32::from(pixel[2]).mul_add(alpha, f32::from(bg[2]) * inv)) as u8);
        }

        let mut buf = std::io::Cursor::new(Vec::new());
        let encoder =
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, self.config.jpeg_quality);
        encoder
            .write_image(&rgb_data, width, height, image::ColorType::Rgb8.into())
            .map_err(|e| RenderError::Export(format!("JPEG encoding failed: {e}")))?;

        Ok(buf.into_inner())
    }

    /// Export the scene to an SVG string.
    ///
    /// # Errors
    ///
    /// Returns an error if scene elements cannot be represented as SVG.
    #[allow(clippy::cast_precision_loss)]
    pub fn render_to_svg(&self, scene: &Scene) -> RenderResult<String> {
        let (out_w, out_h) = self.output_dimensions(scene);
        let scale = self.config.scale;
        let view_w = out_w as f32 / scale;
        let view_h = out_h as f32 / scale;

        let mut svg = String::with_capacity(4096);
        let _ = write!(
            svg,
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{out_w}\" height=\"{out_h}\" viewBox=\"0 0 {view_w} {view_h}\">",
        );

        // Background
        let bg = &self.config.background;
        let bg_alpha = f32::from(bg[3]) / 255.0;
        let _ = write!(
            svg,
            "<rect width=\"100%\" height=\"100%\" fill=\"rgba({},{},{},{})\"/>",
            bg[0], bg[1], bg[2], bg_alpha,
        );

        // Collect and sort elements by z-index
        let mut elements: Vec<_> = scene.elements().collect();
        elements.sort_by_key(|e| e.transform.z_index);

        for element in &elements {
            render_element_svg(&mut svg, element);
        }

        svg.push_str("</svg>");
        Ok(svg)
    }

    /// Export the scene to PDF bytes.
    ///
    /// Renders the scene as a raster image and embeds it in a PDF page.
    ///
    /// # Errors
    ///
    /// Returns an error if rendering or PDF generation fails.
    #[allow(clippy::cast_precision_loss)]
    pub fn render_to_pdf(&self, scene: &Scene) -> RenderResult<Vec<u8>> {
        let png_data = self.render_to_png(scene)?;
        let (out_w, out_h) = self.output_dimensions(scene);

        // Convert pixel dimensions to mm: pixels / dpi * 25.4
        let page_width_mm = out_w as f32 / self.config.dpi * 25.4;
        let page_height_mm = out_h as f32 / self.config.dpi * 25.4;

        let (doc, page1, layer1) = printpdf::PdfDocument::new(
            "Canvas Export",
            printpdf::Mm(page_width_mm),
            printpdf::Mm(page_height_mm),
            "Layer 1",
        );

        let current_layer = doc.get_page(page1).get_layer(layer1);

        // Decode PNG using printpdf's bundled image crate for compatibility
        let dynamic_image = printpdf::image_crate::load_from_memory(&png_data)
            .map_err(|e| RenderError::Export(format!("Failed to decode PNG for PDF: {e}")))?;

        let pdf_image = printpdf::Image::from_dynamic_image(&dynamic_image);

        let scale_x = page_width_mm / out_w as f32;
        let scale_y = page_height_mm / out_h as f32;

        let transform = printpdf::ImageTransform {
            translate_x: Some(printpdf::Mm(0.0)),
            translate_y: Some(printpdf::Mm(0.0)),
            scale_x: Some(scale_x),
            scale_y: Some(scale_y),
            ..Default::default()
        };

        pdf_image.add_to_layer(current_layer, transform);

        doc.save_to_bytes()
            .map_err(|e| RenderError::Export(format!("PDF save failed: {e}")))
    }

    /// Get output dimensions (width, height) in pixels.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn output_dimensions(&self, scene: &Scene) -> (u32, u32) {
        let base_w = self
            .config
            .width
            .unwrap_or_else(|| scene.viewport_width.max(1.0) as u32);
        let base_h = self
            .config
            .height
            .unwrap_or_else(|| scene.viewport_height.max(1.0) as u32);

        #[allow(clippy::cast_precision_loss)]
        let out_w = (base_w as f32 * self.config.scale) as u32;
        #[allow(clippy::cast_precision_loss)]
        let out_h = (base_h as f32 * self.config.scale) as u32;
        (out_w.max(1), out_h.max(1))
    }

    /// Rasterize an SVG string to a tiny-skia Pixmap.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn rasterize_svg(svg_string: &str) -> RenderResult<tiny_skia::Pixmap> {
        let opt = usvg::Options::default();
        let tree = usvg::Tree::from_str(svg_string, &opt)
            .map_err(|e| RenderError::Export(format!("SVG parsing failed: {e}")))?;

        let px_w = tree.size().width() as u32;
        let px_h = tree.size().height() as u32;

        let mut pixmap = tiny_skia::Pixmap::new(px_w.max(1), px_h.max(1))
            .ok_or_else(|| RenderError::Export("Failed to create pixmap".to_string()))?;

        resvg::render(&tree, tiny_skia::Transform::default(), &mut pixmap.as_mut());

        Ok(pixmap)
    }
}

/// Render a single element to SVG.
fn render_element_svg(svg: &mut String, element: &canvas_core::Element) {
    let tf = &element.transform;

    match &element.kind {
        ElementKind::Text {
            content,
            font_size,
            color,
        } => {
            let escaped = escape_xml(content);
            let escaped_color = escape_xml(color);
            let text_y = tf.y + font_size;
            let _ = write!(
                svg,
                "<text x=\"{}\" y=\"{text_y}\" font-size=\"{font_size}\" fill=\"{escaped_color}\" font-family=\"sans-serif\">{escaped}</text>",
                tf.x,
            );
        }

        ElementKind::Image { src, .. } => {
            let escaped_src = escape_xml(src);
            let _ = write!(
                svg,
                "<image x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" href=\"{escaped_src}\"/>",
                tf.x, tf.y, tf.width, tf.height,
            );
        }

        ElementKind::Chart { chart_type, data } => {
            render_chart_svg(svg, tf.x, tf.y, tf.width, tf.height, chart_type, data);

            let label = format!("{chart_type} chart");
            let escaped = escape_xml(&label);
            let lx = tf.x + 4.0;
            let ly = tf.y + 14.0;
            let _ = write!(
                svg,
                "<text x=\"{lx}\" y=\"{ly}\" font-size=\"12\" fill=\"#666\" font-family=\"sans-serif\">{escaped}</text>",
            );
        }

        ElementKind::Group { .. } | ElementKind::OverlayLayer { .. } => {
            let _ = write!(svg, "<g transform=\"translate({},{})\"></g>", tf.x, tf.y);
        }

        ElementKind::Model3D { .. } | ElementKind::Video { .. } => {
            let label = match &element.kind {
                ElementKind::Model3D { .. } => "3D Model",
                ElementKind::Video { .. } => "Video",
                _ => "Unknown",
            };
            let _ = write!(
                svg,
                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"#e0e0e0\" stroke=\"#999\" stroke-width=\"1\"/>",
                tf.x, tf.y, tf.width, tf.height,
            );
            let center_x = tf.x + tf.width / 2.0;
            let center_y = tf.y + tf.height / 2.0;
            let _ = write!(
                svg,
                "<text x=\"{center_x}\" y=\"{center_y}\" font-size=\"14\" fill=\"#666\" text-anchor=\"middle\" font-family=\"sans-serif\">{label}</text>",
            );
        }
    }
}

/// Render basic chart SVG elements for common chart types.
fn render_chart_svg(
    svg: &mut String,
    px: f32,
    py: f32,
    width: f32,
    height: f32,
    chart_type: &str,
    data: &serde_json::Value,
) {
    let _ = write!(
        svg,
        "<rect x=\"{px}\" y=\"{py}\" width=\"{width}\" height=\"{height}\" fill=\"#fafafa\" stroke=\"#ddd\" stroke-width=\"1\"/>",
    );

    match chart_type {
        "bar" => render_bar_chart_svg(svg, px, py, width, height, data),
        "pie" => render_pie_chart_svg(svg, px, py, width, height, data),
        _ => {} // For line, scatter, etc. show background box only
    }
}

/// Render a simple bar chart into SVG.
#[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
fn render_bar_chart_svg(
    svg: &mut String,
    px: f32,
    py: f32,
    width: f32,
    height: f32,
    data: &serde_json::Value,
) {
    let colors = [
        "#4e79a7", "#f28e2b", "#e15759", "#76b7b2", "#59a14f", "#edc948",
    ];

    let values: Vec<f64> = data
        .get("values")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(serde_json::Value::as_f64).collect())
        .unwrap_or_default();

    if values.is_empty() {
        return;
    }

    let max_val = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if max_val <= 0.0 {
        return;
    }

    let padding = 20.0_f32;
    let chart_x = px + padding;
    let chart_y = py + padding;
    let chart_w = width - padding * 2.0;
    let chart_h = height - padding * 2.0;

    let bar_count = values.len() as f32;
    let bar_gap = 4.0_f32;
    let bar_width = (chart_w - bar_gap * (bar_count - 1.0)) / bar_count;

    for (idx, val) in values.iter().enumerate() {
        let bar_h = ((*val / max_val) as f32) * chart_h;
        let bx = chart_x + (idx as f32) * (bar_width + bar_gap);
        let by = chart_y + chart_h - bar_h;
        let color = colors[idx % colors.len()];

        let _ = write!(
            svg,
            "<rect x=\"{bx}\" y=\"{by}\" width=\"{bar_width}\" height=\"{bar_h}\" fill=\"{color}\" rx=\"2\"/>",
        );
    }
}

/// Render a simple pie chart into SVG.
fn render_pie_chart_svg(
    svg: &mut String,
    px: f32,
    py: f32,
    width: f32,
    height: f32,
    data: &serde_json::Value,
) {
    let colors = [
        "#4e79a7", "#f28e2b", "#e15759", "#76b7b2", "#59a14f", "#edc948",
    ];

    let values: Vec<f64> = data
        .get("values")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(serde_json::Value::as_f64).collect())
        .unwrap_or_default();

    if values.is_empty() {
        return;
    }

    let total: f64 = values.iter().sum();
    if total <= 0.0 {
        return;
    }

    let cx = f64::from(px + width / 2.0);
    let cy = f64::from(py + height / 2.0);
    let radius = f64::from((width.min(height) / 2.0) - 10.0);

    let mut start_angle: f64 = -std::f64::consts::FRAC_PI_2;

    for (idx, val) in values.iter().enumerate() {
        let sweep = (val / total) * std::f64::consts::TAU;
        let end_angle = start_angle + sweep;

        let x1 = cx + radius * start_angle.cos();
        let y1 = cy + radius * start_angle.sin();
        let x2 = cx + radius * end_angle.cos();
        let y2 = cy + radius * end_angle.sin();
        let large_arc = i32::from(sweep > std::f64::consts::PI);
        let color = colors[idx % colors.len()];

        let _ = write!(
            svg,
            "<path d=\"M{cx},{cy} L{x1},{y1} A{radius},{radius} 0 {large_arc},1 {x2},{y2} Z\" fill=\"{color}\"/>",
        );

        start_angle = end_angle;
    }
}

/// Escape special XML characters.
fn escape_xml(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use canvas_core::element::{Element, ElementKind, Transform};

    fn text_element(content: &str, ex: f32, ey: f32) -> Element {
        Element::new(ElementKind::Text {
            content: content.to_string(),
            font_size: 16.0,
            color: "#000000".to_string(),
        })
        .with_transform(Transform {
            x: ex,
            y: ey,
            width: 200.0,
            height: 30.0,
            rotation: 0.0,
            z_index: 0,
        })
    }

    #[test]
    fn test_svg_export_empty_scene() {
        let scene = Scene::new(800.0, 600.0);
        let exporter = SceneExporter::with_defaults();
        let svg = exporter.render_to_svg(&scene).expect("svg export");
        assert!(svg.starts_with("<svg"));
        assert!(svg.ends_with("</svg>"));
        assert!(svg.contains("width=\"800\""));
        assert!(svg.contains("height=\"600\""));
    }

    #[test]
    fn test_svg_export_with_text() {
        let mut scene = Scene::new(800.0, 600.0);
        scene.add_element(text_element("Hello World", 10.0, 20.0));

        let exporter = SceneExporter::with_defaults();
        let svg = exporter.render_to_svg(&scene).expect("svg export");
        assert!(svg.contains("Hello World"));
        assert!(svg.contains("font-size=\"16\""));
    }

    #[test]
    fn test_png_export_produces_valid_bytes() {
        let mut scene = Scene::new(100.0, 100.0);
        scene.add_element(text_element("Test", 10.0, 20.0));

        let exporter = SceneExporter::with_defaults();
        let png = exporter.render_to_png(&scene).expect("png export");

        // PNG magic bytes: \x89PNG
        assert!(png.len() > 8);
        assert_eq!(&png[0..4], &[137, 80, 78, 71]);
    }

    #[test]
    fn test_jpeg_export_produces_valid_bytes() {
        let mut scene = Scene::new(100.0, 100.0);
        scene.add_element(text_element("Test", 10.0, 20.0));

        let exporter = SceneExporter::with_defaults();
        let jpeg = exporter.render_to_jpeg(&scene).expect("jpeg export");

        // JPEG magic bytes: FFD8
        assert!(jpeg.len() > 2);
        assert_eq!(jpeg[0], 0xFF);
        assert_eq!(jpeg[1], 0xD8);
    }

    #[test]
    fn test_pdf_export_produces_valid_bytes() {
        let mut scene = Scene::new(200.0, 200.0);
        scene.add_element(text_element("PDF Test", 10.0, 20.0));

        let exporter = SceneExporter::with_defaults();
        let pdf = exporter.render_to_pdf(&scene).expect("pdf export");

        // PDF header: %PDF-
        assert!(pdf.len() > 5);
        assert_eq!(&pdf[0..5], b"%PDF-");
    }

    #[test]
    fn test_export_dispatch() {
        let mut scene = Scene::new(100.0, 100.0);
        scene.add_element(text_element("Dispatch", 10.0, 20.0));

        let exporter = SceneExporter::with_defaults();

        let png = exporter.export(&scene, ExportFormat::Png).expect("png");
        assert_eq!(&png[0..4], &[137, 80, 78, 71]);

        let jpeg = exporter.export(&scene, ExportFormat::Jpeg).expect("jpeg");
        assert_eq!(jpeg[0], 0xFF);

        let svg = exporter.export(&scene, ExportFormat::Svg).expect("svg");
        let svg_str = String::from_utf8(svg).expect("utf8");
        assert!(svg_str.starts_with("<svg"));

        let pdf = exporter.export(&scene, ExportFormat::Pdf).expect("pdf");
        assert_eq!(&pdf[0..5], b"%PDF-");
    }

    #[test]
    fn test_custom_dimensions() {
        let scene = Scene::new(800.0, 600.0);
        let exporter = SceneExporter::new(ExportConfig {
            width: Some(400),
            height: Some(300),
            ..Default::default()
        });

        let svg = exporter.render_to_svg(&scene).expect("svg");
        assert!(svg.contains("width=\"400\""));
        assert!(svg.contains("height=\"300\""));
    }

    #[test]
    fn test_bar_chart_export() {
        let mut scene = Scene::new(400.0, 300.0);
        let chart = Element::new(ElementKind::Chart {
            chart_type: "bar".to_string(),
            data: serde_json::json!({
                "labels": ["A", "B", "C"],
                "values": [10, 25, 15]
            }),
        })
        .with_transform(Transform {
            x: 10.0,
            y: 10.0,
            width: 380.0,
            height: 280.0,
            rotation: 0.0,
            z_index: 0,
        });
        scene.add_element(chart);

        let exporter = SceneExporter::with_defaults();
        let svg = exporter.render_to_svg(&scene).expect("svg");
        assert!(svg.contains("bar chart"));
        assert!(svg.contains("#4e79a7"));
    }

    #[test]
    fn test_pie_chart_export() {
        let mut scene = Scene::new(400.0, 300.0);
        let chart = Element::new(ElementKind::Chart {
            chart_type: "pie".to_string(),
            data: serde_json::json!({
                "labels": ["A", "B"],
                "values": [60, 40]
            }),
        })
        .with_transform(Transform {
            x: 10.0,
            y: 10.0,
            width: 380.0,
            height: 280.0,
            rotation: 0.0,
            z_index: 0,
        });
        scene.add_element(chart);

        let exporter = SceneExporter::with_defaults();
        let svg = exporter.render_to_svg(&scene).expect("svg");
        assert!(svg.contains("<path"));
    }

    #[test]
    fn test_xml_escaping() {
        let mut scene = Scene::new(200.0, 100.0);
        scene.add_element(text_element("A < B & C > D", 10.0, 20.0));

        let exporter = SceneExporter::with_defaults();
        let svg = exporter.render_to_svg(&scene).expect("svg");
        assert!(svg.contains("A &lt; B &amp; C &gt; D"));
    }

    #[test]
    fn test_video_placeholder() {
        let mut scene = Scene::new(200.0, 200.0);
        let video = Element::new(ElementKind::Video {
            stream_id: "test".to_string(),
            is_live: false,
            mirror: false,
            crop: None,
            media_config: None,
        })
        .with_transform(Transform {
            x: 10.0,
            y: 10.0,
            width: 180.0,
            height: 180.0,
            rotation: 0.0,
            z_index: 0,
        });
        scene.add_element(video);

        let exporter = SceneExporter::with_defaults();
        let svg = exporter.render_to_svg(&scene).expect("svg");
        assert!(svg.contains("Video"));
        assert!(svg.contains("#e0e0e0"));
    }

    #[test]
    fn test_empty_scene_png() {
        let scene = Scene::new(50.0, 50.0);
        let exporter = SceneExporter::with_defaults();
        let png = exporter.render_to_png(&scene).expect("empty png");
        assert_eq!(&png[0..4], &[137, 80, 78, 71]);
    }

    #[test]
    fn test_scale_factor() {
        let scene = Scene::new(100.0, 100.0);
        let exporter = SceneExporter::new(ExportConfig {
            scale: 2.0,
            ..Default::default()
        });

        let svg = exporter.render_to_svg(&scene).expect("svg");
        // At 2x scale, output should be 200x200
        assert!(svg.contains("width=\"200\""));
        assert!(svg.contains("height=\"200\""));
        // But viewBox should still map to 100x100
        assert!(svg.contains("viewBox=\"0 0 100 100\""));
    }
}
