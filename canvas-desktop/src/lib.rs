//! # Saorsa Canvas Desktop
//!
//! Native desktop host for Saorsa Canvas using winit + wgpu.
//!
//! This crate provides a standalone desktop application that renders
//! the canvas scene graph with GPU acceleration on macOS (and later
//! Linux/Windows).
//!
//! ## Usage
//!
//! ```bash
//! cargo run -p canvas-desktop
//! ```
//!
//! ## With Communitas connection:
//!
//! ```bash
//! cargo run -p canvas-desktop -- --mcp-url http://localhost:3040/mcp --session default
//! ```
//!
//! ## Architecture
//!
//! - `CliArgs` - Command-line arguments parsed with clap
//! - `DesktopConfig` - Configuration for window size, title, and Communitas connection
//! - `CanvasDesktopApp` - Main application implementing `ApplicationHandler`
//! - Uses `canvas-renderer::WgpuBackend` for GPU rendering

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(clippy::all)]
#![deny(clippy::pedantic)]

mod app;
mod communitas;

pub use app::CanvasDesktopApp;
pub use communitas::{DesktopCommunitasError, DesktopMcpClient};

use clap::Parser;

/// Command-line arguments for canvas-desktop.
#[derive(Debug, Clone, Parser)]
#[command(name = "canvas-desktop")]
#[command(about = "Saorsa Canvas native desktop application")]
#[command(version)]
pub struct CliArgs {
    /// Communitas MCP server URL (e.g., <http://localhost:3040/mcp>)
    #[arg(long, env = "COMMUNITAS_MCP_URL")]
    pub mcp_url: Option<String>,

    /// Session ID for multi-canvas environments
    #[arg(long, env = "CANVAS_SESSION_ID")]
    pub session: Option<String>,

    /// Authentication token for Communitas
    #[arg(long, env = "COMMUNITAS_TOKEN")]
    pub token: Option<String>,

    /// Window width in pixels
    #[arg(long, default_value = "1280")]
    pub width: u32,

    /// Window height in pixels
    #[arg(long, default_value = "720")]
    pub height: u32,
}

/// Desktop application configuration.
#[derive(Debug, Clone)]
pub struct DesktopConfig {
    /// Window width in pixels.
    pub width: u32,
    /// Window height in pixels.
    pub height: u32,
    /// Window title.
    pub title: String,
    /// Communitas MCP server URL for scene sync.
    pub mcp_url: Option<String>,
    /// Session ID for multi-canvas environments.
    pub session: Option<String>,
    /// Authentication token for Communitas.
    pub token: Option<String>,
}

impl Default for DesktopConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl DesktopConfig {
    /// Create a new desktop configuration with default values.
    #[must_use]
    pub fn new() -> Self {
        Self {
            width: 1280,
            height: 720,
            title: "Saorsa Canvas".to_string(),
            mcp_url: None,
            session: None,
            token: None,
        }
    }
}

impl From<CliArgs> for DesktopConfig {
    fn from(args: CliArgs) -> Self {
        Self {
            width: args.width,
            height: args.height,
            title: "Saorsa Canvas".to_string(),
            mcp_url: args.mcp_url,
            session: args.session,
            token: args.token,
        }
    }
}
