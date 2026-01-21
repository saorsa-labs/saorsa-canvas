# Phase 2.4: Communitas CLI

> Goal: Add CLI flags to canvas-desktop for connecting to Communitas MCP and syncing scene state.

## Prerequisites

- [x] Phase 2.3 complete (Scene Rendering)
- [x] CommunitasMcpClient exists in canvas-server/src/communitas.rs
- [x] Scene graph and transforms working

## Overview

Currently canvas-desktop starts with a hardcoded test scene. This phase adds:

1. **CLI Argument Parsing** - Use clap for `--mcp-url`, `--session`, `--token` flags
2. **Communitas Integration** - Reuse CommunitasMcpClient to connect and sync
3. **Scene Sync** - Fetch initial scene from Communitas, receive updates via polling

Architecture:
- CLI parses connection config
- On startup, connect to Communitas MCP (if URL provided)
- Fetch scene document via `get_scene` tool
- Render fetched scene (replacing test scene)
- Periodic polling for updates (async task)

---

<task type="auto" priority="p1">
  <n>Add CLI argument parsing with clap</n>
  <files>
    Cargo.toml,
    canvas-desktop/Cargo.toml,
    canvas-desktop/src/main.rs,
    canvas-desktop/src/lib.rs
  </files>
  <action>
    Add clap dependency and parse CLI arguments:

    1. Add clap to workspace Cargo.toml:
       ```toml
       clap = { version = "4", features = ["derive"] }
       ```

    2. Add clap to canvas-desktop/Cargo.toml:
       ```toml
       clap.workspace = true
       ```

    3. Create CLI args struct in canvas-desktop/src/lib.rs:
       ```rust
       /// Command-line arguments for canvas-desktop.
       #[derive(Debug, Clone, clap::Parser)]
       #[command(name = "canvas-desktop")]
       #[command(about = "Saorsa Canvas native desktop application")]
       pub struct CliArgs {
           /// Communitas MCP server URL (e.g., https://localhost:3040/mcp)
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
       ```

    4. Update DesktopConfig to include connection fields:
       ```rust
       pub struct DesktopConfig {
           pub width: u32,
           pub height: u32,
           pub title: String,
           pub mcp_url: Option<String>,
           pub session: Option<String>,
           pub token: Option<String>,
       }

       impl From<CliArgs> for DesktopConfig {
           fn from(args: CliArgs) -> Self { ... }
       }
       ```

    5. Update main.rs to use clap::Parser:
       ```rust
       use clap::Parser;
       use canvas_desktop::{CliArgs, CanvasDesktopApp, DesktopConfig};

       fn main() -> anyhow::Result<()> {
           let args = CliArgs::parse();
           let config = DesktopConfig::from(args);
           ...
       }
       ```
  </action>
  <verify>
    cargo fmt --all -- --check
    cargo clippy -p canvas-desktop --all-features -- -D warnings
    cargo run -p canvas-desktop -- --help
  </verify>
  <done>
    - `canvas-desktop --help` shows all flags
    - `--mcp-url`, `--session`, `--token` flags accepted
    - Environment variables COMMUNITAS_MCP_URL, CANVAS_SESSION_ID, COMMUNITAS_TOKEN work
    - Window size flags work (--width, --height)
  </done>
</task>

---

<task type="auto" priority="p1">
  <n>Add Communitas client to canvas-desktop</n>
  <files>
    canvas-desktop/Cargo.toml,
    canvas-desktop/src/lib.rs,
    canvas-desktop/src/communitas.rs
  </files>
  <action>
    Create a minimal Communitas client for the desktop app:

    1. Add dependencies to canvas-desktop/Cargo.toml:
       ```toml
       reqwest = { workspace = true, features = ["json"] }
       tokio = { workspace = true, features = ["rt-multi-thread", "macros", "sync"] }
       url.workspace = true
       thiserror.workspace = true
       ```

    2. Create canvas-desktop/src/communitas.rs with a simplified client:
       - Reuse the core CommunitasMcpClient pattern from canvas-server
       - Focus on: initialize, get_scene, authenticate_with_token
       - Use reqwest with async/await
       - Define DesktopCommunitasError enum with thiserror

    3. Key methods needed:
       ```rust
       pub struct DesktopMcpClient { ... }

       impl DesktopMcpClient {
           pub fn new(url: &str) -> Result<Self, DesktopCommunitasError>;
           pub async fn initialize(&self) -> Result<(), DesktopCommunitasError>;
           pub async fn authenticate(&self, token: &str) -> Result<(), DesktopCommunitasError>;
           pub async fn get_scene(&self, session: Option<&str>) -> Result<SceneDocument, DesktopCommunitasError>;
       }
       ```

    4. Update lib.rs to expose module:
       ```rust
       mod communitas;
       pub use communitas::{DesktopMcpClient, DesktopCommunitasError};
       ```
  </action>
  <verify>
    cargo fmt --all -- --check
    cargo clippy -p canvas-desktop --all-features -- -D warnings
    cargo test -p canvas-desktop
  </verify>
  <done>
    - DesktopMcpClient compiles and exports from lib.rs
    - Can construct client with URL
    - Error types defined with thiserror
    - No unwrap/expect in production code
  </done>
</task>

---

<task type="auto" priority="p1">
  <n>Integrate Communitas sync on startup</n>
  <files>
    canvas-desktop/src/main.rs,
    canvas-desktop/src/app.rs
  </files>
  <action>
    Connect to Communitas on startup and fetch initial scene:

    1. Update main.rs to initialize Communitas before event loop:
       ```rust
       fn main() -> anyhow::Result<()> {
           // ... existing setup ...

           // If MCP URL provided, fetch initial scene
           let initial_scene = if let Some(ref mcp_url) = config.mcp_url {
               let rt = tokio::runtime::Runtime::new()?;
               rt.block_on(async {
                   fetch_initial_scene(&config).await
               })?
           } else {
               None
           };

           let mut app = CanvasDesktopApp::new(config, initial_scene);
           // ... event loop ...
       }

       async fn fetch_initial_scene(config: &DesktopConfig) -> anyhow::Result<Option<Scene>> {
           let client = DesktopMcpClient::new(config.mcp_url.as_ref().unwrap())?;
           client.initialize().await?;

           if let Some(ref token) = config.token {
               client.authenticate(token).await?;
           }

           let doc = client.get_scene(config.session.as_deref()).await?;
           Ok(Some(doc.into_scene()?))
       }
       ```

    2. Update CanvasDesktopApp::new to accept optional initial scene:
       ```rust
       pub fn new(config: DesktopConfig, initial_scene: Option<Scene>) -> Self {
           let scene = initial_scene.unwrap_or_else(|| {
               // Fallback to test scene
               Self::create_test_scene(&config)
           });
           ...
       }
       ```

    3. Extract test scene creation to separate method:
       ```rust
       fn create_test_scene(config: &DesktopConfig) -> Scene {
           let mut scene = Scene::new(config.width as f32, config.height as f32);
           // ... existing chart and text elements ...
           scene
       }
       ```

    4. Add error handling for connection failures:
       - Log warning if connection fails
       - Fall back to test scene
       - Don't crash the app
  </action>
  <verify>
    cargo fmt --all -- --check
    cargo clippy -p canvas-desktop --all-features -- -D warnings
    cargo test -p canvas-desktop
    cargo run -p canvas-desktop  # Should start with test scene
    cargo run -p canvas-desktop -- --mcp-url http://localhost:9999  # Should warn and fallback
  </verify>
  <done>
    - App starts without --mcp-url (test scene)
    - App attempts connection with --mcp-url
    - Connection failures logged and gracefully handled
    - Initial scene fetched and rendered when connection succeeds
    - Token authentication works when --token provided
  </done>
</task>

---

## Verification

```bash
# Full verification
cargo fmt --all -- --check
cargo clippy --workspace --all-features -- -D warnings
cargo test --workspace

# CLI verification
cargo run -p canvas-desktop -- --help
cargo run -p canvas-desktop -- --width 800 --height 600
cargo run -p canvas-desktop -- --mcp-url http://localhost:3040/mcp
```

## Risks

- **Medium**: Async runtime integration with winit event loop - may need careful threading
- **Low**: Network errors during scene fetch - mitigated by fallback to test scene

## Notes

- Periodic scene polling deferred to M3 (requires WebSocket or persistent connection)
- Real-time sync via WebSocket is a future enhancement
- Current implementation is pull-only (fetch on startup)

## Exit Criteria

- [x] `canvas-desktop --help` shows all connection options
- [x] `--mcp-url` flag connects to Communitas MCP
- [x] `--token` flag authenticates with Communitas
- [x] `--session` flag selects specific session
- [x] Connection failures gracefully fall back to test scene
- [x] All clippy warnings resolved
- [x] ROADMAP.md updated with Phase 2.4 DONE
