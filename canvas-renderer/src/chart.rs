//! Chart rendering utilities using plotters.

use std::str::FromStr;

use canvas_core::Element;

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

impl FromStr for ChartType {
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
    /// X value or label.
    pub x: f64,
    /// Y value.
    pub y: f64,
    /// Optional label.
    pub label: Option<String>,
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
    /// Data series.
    pub data: Vec<DataPoint>,
    /// Chart width in pixels.
    pub width: u32,
    /// Chart height in pixels.
    pub height: u32,
}

impl Default for ChartConfig {
    fn default() -> Self {
        Self {
            chart_type: ChartType::Bar,
            title: None,
            x_label: None,
            y_label: None,
            data: Vec::new(),
            width: 400,
            height: 300,
        }
    }
}

/// Render a chart to an image buffer.
///
/// # Errors
///
/// Returns an error if chart rendering fails.
pub fn render_chart_to_buffer(_config: &ChartConfig) -> Result<Vec<u8>, String> {
    // TODO: Implement actual chart rendering with plotters
    // For now, return a placeholder

    Err("Chart rendering not yet implemented".to_string())
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

    // Convert data to JSON
    let data_json: Vec<serde_json::Value> = config
        .data
        .iter()
        .map(|dp| {
            serde_json::json!({
                "x": dp.x,
                "y": dp.y,
                "label": dp.label
            })
        })
        .collect();

    Element::new(ElementKind::Chart {
        chart_type: chart_type_str.to_string(),
        data: serde_json::json!({
            "title": config.title,
            "x_label": config.x_label,
            "y_label": config.y_label,
            "points": data_json
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
