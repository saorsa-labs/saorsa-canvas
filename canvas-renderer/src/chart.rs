//! Chart rendering utilities using plotters.
//!
//! Renders charts to RGBA pixel buffers that can be composited into the canvas.

use plotters::prelude::*;

use canvas_core::Element;

use crate::error::{RenderError, RenderResult};

/// Chart types supported by the canvas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChartType {
    /// Vertical bar chart.
    Bar,
    /// Horizontal bar chart.
    BarHorizontal,
    /// Line chart.
    Line,
    /// Area chart (filled line).
    Area,
    /// Pie chart.
    Pie,
    /// Donut chart.
    Donut,
    /// Scatter plot.
    Scatter,
}

impl std::str::FromStr for ChartType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "bar" => Ok(Self::Bar),
            "bar_horizontal" | "barh" => Ok(Self::BarHorizontal),
            "line" => Ok(Self::Line),
            "area" => Ok(Self::Area),
            "pie" => Ok(Self::Pie),
            "donut" => Ok(Self::Donut),
            "scatter" => Ok(Self::Scatter),
            _ => Err(format!("Unknown chart type: {s}")),
        }
    }
}

/// Data point for charts.
#[derive(Debug, Clone)]
pub struct DataPoint {
    /// X value or label index.
    pub x: f64,
    /// Y value.
    pub y: f64,
    /// Optional label.
    pub label: Option<String>,
}

/// Series data for multi-series charts.
#[derive(Debug, Clone)]
pub struct DataSeries {
    /// Series name.
    pub name: String,
    /// Series color as hex.
    pub color: Option<String>,
    /// Data points in this series.
    pub points: Vec<DataPoint>,
}

/// Chart configuration.
#[derive(Debug, Clone)]
pub struct ChartConfig {
    /// Chart type.
    pub chart_type: ChartType,
    /// Chart title.
    pub title: Option<String>,
    /// X-axis label.
    pub x_label: Option<String>,
    /// Y-axis label.
    pub y_label: Option<String>,
    /// X-axis labels (for categorical data).
    pub x_labels: Vec<String>,
    /// Data series.
    pub series: Vec<DataSeries>,
    /// Chart width in pixels.
    pub width: u32,
    /// Chart height in pixels.
    pub height: u32,
    /// Background color as hex.
    pub background: String,
    /// Show legend.
    pub show_legend: bool,
}

impl Default for ChartConfig {
    fn default() -> Self {
        Self {
            chart_type: ChartType::Bar,
            title: None,
            x_label: None,
            y_label: None,
            x_labels: Vec::new(),
            series: Vec::new(),
            width: 400,
            height: 300,
            background: "#ffffff".to_string(),
            show_legend: true,
        }
    }
}

/// Parse a hex color string to RGB.
fn parse_hex_color(hex: &str) -> RGBColor {
    let hex = hex.trim_start_matches('#');
    if hex.len() >= 6 {
        let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0);
        let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0);
        let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0);
        RGBColor(r, g, b)
    } else {
        RGBColor(0, 0, 0)
    }
}

/// Default color palette for charts.
const PALETTE: &[RGBColor] = &[
    RGBColor(54, 162, 235),   // Blue
    RGBColor(255, 99, 132),   // Red
    RGBColor(75, 192, 192),   // Teal
    RGBColor(255, 205, 86),   // Yellow
    RGBColor(153, 102, 255),  // Purple
    RGBColor(255, 159, 64),   // Orange
    RGBColor(201, 203, 207),  // Gray
    RGBColor(100, 181, 246),  // Light Blue
];

/// Get color for a series index.
fn get_series_color(index: usize, custom: Option<&str>) -> RGBColor {
    if let Some(hex) = custom {
        parse_hex_color(hex)
    } else {
        PALETTE[index % PALETTE.len()]
    }
}

/// Render a chart to an RGBA image buffer.
///
/// # Errors
///
/// Returns an error if chart rendering fails.
#[allow(clippy::too_many_lines)]
pub fn render_chart_to_buffer(config: &ChartConfig) -> RenderResult<Vec<u8>> {
    let (width, height) = (config.width, config.height);
    let mut buffer = vec![0u8; (width * height * 3) as usize];

    {
        let root = BitMapBackend::with_buffer(&mut buffer, (width, height)).into_drawing_area();
        let bg_color = parse_hex_color(&config.background);
        root.fill(&bg_color)
            .map_err(|e| RenderError::Frame(format!("Failed to fill background: {e}")))?;

        match config.chart_type {
            ChartType::Bar | ChartType::BarHorizontal => {
                render_bar_chart(&root, config)?;
            }
            ChartType::Line | ChartType::Area => {
                render_line_chart(&root, config)?;
            }
            ChartType::Scatter => {
                render_scatter_chart(&root, config)?;
            }
            ChartType::Pie | ChartType::Donut => {
                render_pie_chart(&root, config)?;
            }
        }

        root.present()
            .map_err(|e| RenderError::Frame(format!("Failed to present chart: {e}")))?;
    }

    // Convert RGB to RGBA
    let mut rgba_buffer = Vec::with_capacity((width * height * 4) as usize);
    for chunk in buffer.chunks(3) {
        rgba_buffer.push(chunk[0]); // R
        rgba_buffer.push(chunk[1]); // G
        rgba_buffer.push(chunk[2]); // B
        rgba_buffer.push(255);      // A
    }

    Ok(rgba_buffer)
}

/// Render a bar chart.
#[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn render_bar_chart(
    root: &DrawingArea<BitMapBackend, plotters::coord::Shift>,
    config: &ChartConfig,
) -> RenderResult<()> {
    if config.series.is_empty() {
        return Ok(());
    }

    // Calculate Y range from all series
    let mut y_min = 0.0_f64;
    let mut y_max = 0.0_f64;
    for series in &config.series {
        for point in &series.points {
            y_max = y_max.max(point.y);
            y_min = y_min.min(point.y);
        }
    }
    // Add some padding
    y_max *= 1.1;
    if y_min > 0.0 {
        y_min = 0.0;
    }

    let num_bars = config.series.first().map_or(0, |s| s.points.len());

    let mut chart = ChartBuilder::on(root)
        .caption(
            config.title.as_deref().unwrap_or(""),
            ("sans-serif", 20).into_font(),
        )
        .margin(10)
        .x_label_area_size(40)
        .y_label_area_size(50)
        .build_cartesian_2d(0..num_bars, y_min..y_max)
        .map_err(|e| RenderError::Frame(format!("Failed to build chart: {e}")))?;

    chart
        .configure_mesh()
        .x_labels(num_bars)
        .x_label_formatter(&|x| {
            config
                .x_labels
                .get(*x)
                .cloned()
                .unwrap_or_else(|| x.to_string())
        })
        .y_desc(config.y_label.as_deref().unwrap_or(""))
        .draw()
        .map_err(|e| RenderError::Frame(format!("Failed to draw mesh: {e}")))?;

    let num_series = config.series.len();
    let bar_width = 0.8 / num_series as f64;

    for (series_idx, series) in config.series.iter().enumerate() {
        let color = get_series_color(series_idx, series.color.as_deref());
        let offset = (series_idx as f64 - (num_series as f64 - 1.0) / 2.0) * bar_width;

        chart
            .draw_series(series.points.iter().enumerate().map(|(i, point)| {
                let x0 = i as f64 + offset - bar_width / 2.0 + 0.5;
                let x1 = i as f64 + offset + bar_width / 2.0 + 0.5;
                Rectangle::new(
                    [
                        (x0 as usize, 0.0_f64.max(y_min)),
                        (x1 as usize, point.y),
                    ],
                    color.filled(),
                )
            }))
            .map_err(|e| RenderError::Frame(format!("Failed to draw bars: {e}")))?
            .label(&series.name)
            .legend(move |(x, y)| Rectangle::new([(x, y - 5), (x + 10, y + 5)], color.filled()));
    }

    if config.show_legend && num_series > 1 {
        chart
            .configure_series_labels()
            .background_style(WHITE.mix(0.8))
            .border_style(BLACK)
            .draw()
            .map_err(|e| RenderError::Frame(format!("Failed to draw legend: {e}")))?;
    }

    Ok(())
}

/// Render a line or area chart.
#[allow(clippy::cast_precision_loss)]
fn render_line_chart(
    root: &DrawingArea<BitMapBackend, plotters::coord::Shift>,
    config: &ChartConfig,
) -> RenderResult<()> {
    if config.series.is_empty() {
        return Ok(());
    }

    // Calculate ranges from all series
    let mut x_min = 0.0_f64;
    let mut x_max = 0.0_f64;
    let mut y_min = 0.0_f64;
    let mut y_max = 0.0_f64;

    for series in &config.series {
        for point in &series.points {
            x_min = x_min.min(point.x);
            x_max = x_max.max(point.x);
            y_min = y_min.min(point.y);
            y_max = y_max.max(point.y);
        }
    }

    // Add padding
    let x_padding = (x_max - x_min) * 0.05;
    let y_padding = (y_max - y_min) * 0.1;
    x_min -= x_padding;
    x_max += x_padding;
    y_max += y_padding;
    if y_min > 0.0 {
        y_min = 0.0;
    }

    let mut chart = ChartBuilder::on(root)
        .caption(
            config.title.as_deref().unwrap_or(""),
            ("sans-serif", 20).into_font(),
        )
        .margin(10)
        .x_label_area_size(40)
        .y_label_area_size(50)
        .build_cartesian_2d(x_min..x_max, y_min..y_max)
        .map_err(|e| RenderError::Frame(format!("Failed to build chart: {e}")))?;

    chart
        .configure_mesh()
        .x_desc(config.x_label.as_deref().unwrap_or(""))
        .y_desc(config.y_label.as_deref().unwrap_or(""))
        .draw()
        .map_err(|e| RenderError::Frame(format!("Failed to draw mesh: {e}")))?;

    let is_area = config.chart_type == ChartType::Area;

    for (series_idx, series) in config.series.iter().enumerate() {
        let color = get_series_color(series_idx, series.color.as_deref());

        let points: Vec<(f64, f64)> = series.points.iter().map(|p| (p.x, p.y)).collect();

        if is_area {
            chart
                .draw_series(AreaSeries::new(points.clone(), 0.0, color.mix(0.3)))
                .map_err(|e| RenderError::Frame(format!("Failed to draw area: {e}")))?;
        }

        chart
            .draw_series(LineSeries::new(points, color.stroke_width(2)))
            .map_err(|e| RenderError::Frame(format!("Failed to draw line: {e}")))?
            .label(&series.name)
            .legend(move |(x, y)| PathElement::new(vec![(x, y), (x + 15, y)], color.stroke_width(2)));
    }

    if config.show_legend && config.series.len() > 1 {
        chart
            .configure_series_labels()
            .background_style(WHITE.mix(0.8))
            .border_style(BLACK)
            .draw()
            .map_err(|e| RenderError::Frame(format!("Failed to draw legend: {e}")))?;
    }

    Ok(())
}

/// Render a scatter chart.
#[allow(clippy::cast_precision_loss)]
fn render_scatter_chart(
    root: &DrawingArea<BitMapBackend, plotters::coord::Shift>,
    config: &ChartConfig,
) -> RenderResult<()> {
    if config.series.is_empty() {
        return Ok(());
    }

    // Calculate ranges
    let mut x_min = f64::MAX;
    let mut x_max = f64::MIN;
    let mut y_min = f64::MAX;
    let mut y_max = f64::MIN;

    for series in &config.series {
        for point in &series.points {
            x_min = x_min.min(point.x);
            x_max = x_max.max(point.x);
            y_min = y_min.min(point.y);
            y_max = y_max.max(point.y);
        }
    }

    // Add padding
    let x_padding = (x_max - x_min) * 0.1;
    let y_padding = (y_max - y_min) * 0.1;
    x_min -= x_padding;
    x_max += x_padding;
    y_min -= y_padding;
    y_max += y_padding;

    let mut chart = ChartBuilder::on(root)
        .caption(
            config.title.as_deref().unwrap_or(""),
            ("sans-serif", 20).into_font(),
        )
        .margin(10)
        .x_label_area_size(40)
        .y_label_area_size(50)
        .build_cartesian_2d(x_min..x_max, y_min..y_max)
        .map_err(|e| RenderError::Frame(format!("Failed to build chart: {e}")))?;

    chart
        .configure_mesh()
        .x_desc(config.x_label.as_deref().unwrap_or(""))
        .y_desc(config.y_label.as_deref().unwrap_or(""))
        .draw()
        .map_err(|e| RenderError::Frame(format!("Failed to draw mesh: {e}")))?;

    for (series_idx, series) in config.series.iter().enumerate() {
        let color = get_series_color(series_idx, series.color.as_deref());

        chart
            .draw_series(
                series
                    .points
                    .iter()
                    .map(|p| Circle::new((p.x, p.y), 5, color.filled())),
            )
            .map_err(|e| RenderError::Frame(format!("Failed to draw scatter: {e}")))?
            .label(&series.name)
            .legend(move |(x, y)| Circle::new((x + 5, y), 5, color.filled()));
    }

    if config.show_legend && config.series.len() > 1 {
        chart
            .configure_series_labels()
            .background_style(WHITE.mix(0.8))
            .border_style(BLACK)
            .draw()
            .map_err(|e| RenderError::Frame(format!("Failed to draw legend: {e}")))?;
    }

    Ok(())
}

/// Render a pie or donut chart.
#[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation, clippy::cast_possible_wrap, clippy::cast_sign_loss)]
fn render_pie_chart(
    root: &DrawingArea<BitMapBackend, plotters::coord::Shift>,
    config: &ChartConfig,
) -> RenderResult<()> {
    if config.series.is_empty() {
        return Ok(());
    }

    // Use first series for pie chart
    let series = &config.series[0];
    if series.points.is_empty() {
        return Ok(());
    }

    // Calculate total
    let total: f64 = series.points.iter().map(|p| p.y.abs()).sum();
    if total == 0.0 {
        return Ok(());
    }

    // Draw title
    if let Some(title) = &config.title {
        root.draw(&Text::new(
            title.clone(),
            (config.width as i32 / 2, 15),
            ("sans-serif", 20).into_font().color(&BLACK),
        ))
        .map_err(|e| RenderError::Frame(format!("Failed to draw title: {e}")))?;
    }

    let center_x = f64::from(config.width) / 2.0;
    let center_y = f64::from(config.height) / 2.0 + 10.0;
    let radius = f64::from(config.width.min(config.height)) / 2.5;
    let inner_radius = if config.chart_type == ChartType::Donut {
        radius * 0.5
    } else {
        0.0
    };

    let mut start_angle = -std::f64::consts::FRAC_PI_2; // Start at top

    for (i, point) in series.points.iter().enumerate() {
        let fraction = point.y.abs() / total;
        let sweep_angle = fraction * std::f64::consts::PI * 2.0;
        let end_angle = start_angle + sweep_angle;
        let color = get_series_color(i, None);

        // Draw pie slice as filled polygon
        let num_segments = ((sweep_angle * 50.0) as usize).max(10);
        let mut vertices = Vec::with_capacity(num_segments + 3);

        // Inner edge (for donut) or center point
        if inner_radius > 0.0 {
            for j in 0..=num_segments {
                let angle = start_angle + (sweep_angle * j as f64 / num_segments as f64);
                let x = center_x + inner_radius * angle.cos();
                let y = center_y + inner_radius * angle.sin();
                vertices.push((x as i32, y as i32));
            }
        } else {
            vertices.push((center_x as i32, center_y as i32));
        }

        // Outer edge
        for j in (0..=num_segments).rev() {
            let angle = start_angle + (sweep_angle * j as f64 / num_segments as f64);
            let x = center_x + radius * angle.cos();
            let y = center_y + radius * angle.sin();
            vertices.push((x as i32, y as i32));
        }

        root.draw(&Polygon::new(vertices, color.filled()))
            .map_err(|e| RenderError::Frame(format!("Failed to draw pie slice: {e}")))?;

        start_angle = end_angle;
    }

    // Draw legend
    if config.show_legend {
        let legend_x = config.width as i32 - 100;
        let mut legend_y = 30;

        for (i, point) in series.points.iter().enumerate() {
            let color = get_series_color(i, None);
            let default_label = format!("{:.0}", point.y);
            let label = point.label.as_deref().unwrap_or(&default_label);

            root.draw(&Rectangle::new(
                [(legend_x, legend_y), (legend_x + 12, legend_y + 12)],
                color.filled(),
            ))
            .map_err(|e| RenderError::Frame(format!("Failed to draw legend box: {e}")))?;

            root.draw(&Text::new(
                label.to_string(),
                (legend_x + 18, legend_y + 10),
                ("sans-serif", 12).into_font().color(&BLACK),
            ))
            .map_err(|e| RenderError::Frame(format!("Failed to draw legend text: {e}")))?;

            legend_y += 18;
        }
    }

    Ok(())
}

/// Create a chart element from configuration.
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn create_chart_element(config: &ChartConfig) -> Element {
    use canvas_core::{ElementKind, Transform};

    let chart_type_str = match config.chart_type {
        ChartType::Bar => "bar",
        ChartType::BarHorizontal => "bar_horizontal",
        ChartType::Line => "line",
        ChartType::Area => "area",
        ChartType::Pie => "pie",
        ChartType::Donut => "donut",
        ChartType::Scatter => "scatter",
    };

    // Convert series to JSON
    let series_json: Vec<serde_json::Value> = config
        .series
        .iter()
        .map(|s| {
            serde_json::json!({
                "name": s.name,
                "color": s.color,
                "points": s.points.iter().map(|p| {
                    serde_json::json!({
                        "x": p.x,
                        "y": p.y,
                        "label": p.label
                    })
                }).collect::<Vec<_>>()
            })
        })
        .collect();

    Element::new(ElementKind::Chart {
        chart_type: chart_type_str.to_string(),
        data: serde_json::json!({
            "title": config.title,
            "x_label": config.x_label,
            "y_label": config.y_label,
            "x_labels": config.x_labels,
            "series": series_json,
            "background": config.background,
            "show_legend": config.show_legend
        }),
    })
    .with_transform(Transform {
        x: 0.0,
        y: 0.0,
        width: config.width as f32,
        height: config.height as f32,
        rotation: 0.0,
        z_index: 0,
    })
}

/// Parse chart configuration from JSON data.
///
/// # Errors
///
/// Returns an error if the JSON cannot be parsed.
#[allow(clippy::cast_precision_loss)]
pub fn parse_chart_config(
    chart_type: &str,
    data: &serde_json::Value,
    width: u32,
    height: u32,
) -> RenderResult<ChartConfig> {
    let chart_type: ChartType = chart_type
        .parse()
        .map_err(|e: String| RenderError::Resource(e))?;

    let title = data.get("title").and_then(serde_json::Value::as_str).map(String::from);
    let x_label = data
        .get("x_label")
        .and_then(serde_json::Value::as_str)
        .map(String::from);
    let y_label = data
        .get("y_label")
        .and_then(serde_json::Value::as_str)
        .map(String::from);

    // Parse x_labels
    let x_labels: Vec<String> = data
        .get("x_labels")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    // Parse series - support both "series" array and legacy single-series format
    let series = if let Some(series_arr) = data.get("series").and_then(|v| v.as_array()) {
        series_arr
            .iter()
            .enumerate()
            .map(|(i, s)| parse_series(s, i))
            .collect()
    } else if let Some(points) = data.get("points").and_then(|v| v.as_array()) {
        // Legacy format with just "points"
        vec![DataSeries {
            name: "Data".to_string(),
            color: None,
            points: points.iter().map(parse_data_point).collect(),
        }]
    } else if let Some(labels) = data.get("labels").and_then(|v| v.as_array()) {
        // Simple format with labels and values arrays
        let values = data.get("values").and_then(|v| v.as_array());
        vec![DataSeries {
            name: "Data".to_string(),
            color: None,
            points: labels
                .iter()
                .enumerate()
                .map(|(i, label)| DataPoint {
                    x: i as f64,
                    y: values
                        .and_then(|v| v.get(i))
                        .and_then(serde_json::Value::as_f64)
                        .unwrap_or(0.0),
                    label: label.as_str().map(String::from),
                })
                .collect(),
        }]
    } else {
        Vec::new()
    };

    let background = data
        .get("background")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("#ffffff")
        .to_string();

    let show_legend = data
        .get("show_legend")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(true);

    Ok(ChartConfig {
        chart_type,
        title,
        x_label,
        y_label,
        x_labels,
        series,
        width,
        height,
        background,
        show_legend,
    })
}

/// Parse a data series from JSON.
fn parse_series(value: &serde_json::Value, index: usize) -> DataSeries {
    let name = value
        .get("name")
        .and_then(serde_json::Value::as_str)
        .map_or_else(|| format!("Series {}", index + 1), String::from);

    let color = value
        .get("color")
        .and_then(serde_json::Value::as_str)
        .map(String::from);

    let points: Vec<DataPoint> = value
        .get("points")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().map(parse_data_point).collect())
        .unwrap_or_default();

    DataSeries {
        name,
        color,
        points,
    }
}

/// Parse a data point from JSON.
fn parse_data_point(value: &serde_json::Value) -> DataPoint {
    DataPoint {
        x: value.get("x").and_then(serde_json::Value::as_f64).unwrap_or(0.0),
        y: value.get("y").and_then(serde_json::Value::as_f64).unwrap_or(0.0),
        label: value
            .get("label")
            .and_then(serde_json::Value::as_str)
            .map(String::from),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bar_chart_renders() {
        let config = ChartConfig {
            chart_type: ChartType::Bar,
            title: Some("Test Bar Chart".to_string()),
            x_labels: vec!["A".to_string(), "B".to_string(), "C".to_string()],
            series: vec![DataSeries {
                name: "Data".to_string(),
                color: None,
                points: vec![
                    DataPoint {
                        x: 0.0,
                        y: 10.0,
                        label: Some("A".to_string()),
                    },
                    DataPoint {
                        x: 1.0,
                        y: 20.0,
                        label: Some("B".to_string()),
                    },
                    DataPoint {
                        x: 2.0,
                        y: 15.0,
                        label: Some("C".to_string()),
                    },
                ],
            }],
            width: 400,
            height: 300,
            ..Default::default()
        };

        let pixels = render_chart_to_buffer(&config).expect("Should render");
        assert_eq!(pixels.len(), 400 * 300 * 4);
        // Check not all pixels are the same (chart was drawn)
        assert!(!pixels.chunks(4).all(|c| c == [255, 255, 255, 255]));
    }

    #[test]
    fn test_line_chart_renders() {
        let config = ChartConfig {
            chart_type: ChartType::Line,
            title: Some("Test Line Chart".to_string()),
            series: vec![DataSeries {
                name: "Series 1".to_string(),
                color: Some("#ff0000".to_string()),
                points: vec![
                    DataPoint {
                        x: 0.0,
                        y: 5.0,
                        label: None,
                    },
                    DataPoint {
                        x: 1.0,
                        y: 10.0,
                        label: None,
                    },
                    DataPoint {
                        x: 2.0,
                        y: 7.0,
                        label: None,
                    },
                ],
            }],
            width: 400,
            height: 300,
            ..Default::default()
        };

        let pixels = render_chart_to_buffer(&config).expect("Should render");
        assert_eq!(pixels.len(), 400 * 300 * 4);
    }

    #[test]
    fn test_pie_chart_renders() {
        let config = ChartConfig {
            chart_type: ChartType::Pie,
            title: Some("Test Pie Chart".to_string()),
            series: vec![DataSeries {
                name: "Data".to_string(),
                color: None,
                points: vec![
                    DataPoint {
                        x: 0.0,
                        y: 30.0,
                        label: Some("A".to_string()),
                    },
                    DataPoint {
                        x: 1.0,
                        y: 50.0,
                        label: Some("B".to_string()),
                    },
                    DataPoint {
                        x: 2.0,
                        y: 20.0,
                        label: Some("C".to_string()),
                    },
                ],
            }],
            width: 400,
            height: 300,
            ..Default::default()
        };

        let pixels = render_chart_to_buffer(&config).expect("Should render");
        assert_eq!(pixels.len(), 400 * 300 * 4);
    }

    #[test]
    fn test_parse_chart_config() {
        let data = serde_json::json!({
            "title": "Sales",
            "labels": ["Q1", "Q2", "Q3"],
            "values": [100, 200, 150]
        });

        let config = parse_chart_config("bar", &data, 400, 300).expect("Should parse");
        assert_eq!(config.title, Some("Sales".to_string()));
        assert_eq!(config.series.len(), 1);
        assert_eq!(config.series[0].points.len(), 3);
    }

    #[test]
    fn test_parse_hex_color() {
        assert_eq!(parse_hex_color("#ff0000"), RGBColor(255, 0, 0));
        assert_eq!(parse_hex_color("00ff00"), RGBColor(0, 255, 0));
        assert_eq!(parse_hex_color("#0000ff"), RGBColor(0, 0, 255));
    }
}
