//! # Saorsa Canvas Desktop
//!
//! Native desktop application for Saorsa Canvas.

use canvas_core::Scene;
use canvas_desktop::{CanvasDesktopApp, CliArgs, DesktopConfig, DesktopMcpClient};
use clap::Parser;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use winit::event_loop::EventLoop;

fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "canvas_desktop=debug,canvas_renderer=debug,wgpu=warn".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("Starting Saorsa Canvas Desktop");

    // Parse CLI arguments
    let args = CliArgs::parse();
    let config = DesktopConfig::from(args);

    tracing::info!(
        "Window config: {}x{} \"{}\"",
        config.width,
        config.height,
        config.title
    );

    if let Some(ref url) = config.mcp_url {
        tracing::info!("Communitas MCP URL: {}", url);
    }

    // Fetch initial scene from Communitas if configured
    let initial_scene = if config.mcp_url.is_some() {
        match fetch_initial_scene(&config) {
            Ok(scene) => {
                tracing::info!("Successfully fetched scene from Communitas");
                Some(scene)
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to fetch scene from Communitas, using test scene: {}",
                    e
                );
                None
            }
        }
    } else {
        None
    };

    // Create application
    tracing::debug!("Creating CanvasDesktopApp");
    let mut app = CanvasDesktopApp::new(config, initial_scene);

    // Create and run event loop
    tracing::debug!("Creating event loop");
    let event_loop = EventLoop::new()?;
    tracing::debug!("Event loop created, starting run_app");

    let result = event_loop.run_app(&mut app);
    tracing::debug!("run_app returned: {:?}", result);
    result?;

    tracing::info!("Saorsa Canvas Desktop exited");
    Ok(())
}

/// Fetch the initial scene from Communitas MCP server.
fn fetch_initial_scene(config: &DesktopConfig) -> anyhow::Result<Scene> {
    let mcp_url = config
        .mcp_url
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("No MCP URL configured"))?;

    // Create a tokio runtime for the async fetch
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        tracing::debug!("Connecting to Communitas MCP at {}", mcp_url);
        let client = DesktopMcpClient::new(mcp_url)?;

        // Initialize the connection
        let init_result = client.initialize().await?;
        tracing::debug!(
            "Connected to {} v{}",
            init_result.server_info.name,
            init_result.server_info.version
        );

        // Authenticate if token provided
        if let Some(ref token) = config.token {
            tracing::debug!("Authenticating with token");
            client.authenticate(token).await?;
        }

        // Fetch the scene
        let session = config.session.as_deref();
        tracing::debug!("Fetching scene for session: {:?}", session);
        let doc = client.get_scene(session).await?;

        // Convert to Scene
        let scene = doc
            .into_scene()
            .map_err(|e| anyhow::anyhow!("Failed to convert scene document: {}", e))?;

        Ok(scene)
    })
}
