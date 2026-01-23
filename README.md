# Saorsa Canvas

**The Universal AI Visual Interface**

> *Where AI meets the eye.*

[![crates.io](https://img.shields.io/crates/v/canvas-core.svg)](https://crates.io/crates/canvas-core)
[![docs.rs](https://docs.rs/canvas-core/badge.svg)](https://docs.rs/canvas-core)
[![License](https://img.shields.io/crates/l/canvas-core.svg)](https://github.com/saorsa-labs/saorsa-canvas#license)

Saorsa Canvas is an AI-native visual surface that runs on any device—from Raspberry Pi to Mac Studio to Looking Glass holographic displays. It's not a traditional UI framework; it's a **Model Context Protocol (MCP) canvas** where AI agents render content and humans participate through voice, touch, and gaze.

## Crates

| Crate | Description | crates.io |
|-------|-------------|-----------|
| [canvas-core](canvas-core/) | Scene graph, state management, input handling | [![crates.io](https://img.shields.io/crates/v/canvas-core.svg)](https://crates.io/crates/canvas-core) |
| [canvas-renderer](canvas-renderer/) | wgpu rendering backends with WebGL2/Canvas2D fallbacks | [![crates.io](https://img.shields.io/crates/v/canvas-renderer.svg)](https://crates.io/crates/canvas-renderer) |
| [canvas-mcp](canvas-mcp/) | MCP tools and resources for AI integration | [![crates.io](https://img.shields.io/crates/v/canvas-mcp.svg)](https://crates.io/crates/canvas-mcp) |
| [canvas-app](canvas-app/) | WASM application for web deployment | [![crates.io](https://img.shields.io/crates/v/canvas-app.svg)](https://crates.io/crates/canvas-app) |
| [canvas-server](canvas-server/) | Axum server with WebSocket and WebRTC signaling | [![crates.io](https://img.shields.io/crates/v/canvas-server.svg)](https://crates.io/crates/canvas-server) |
| [canvas-desktop](canvas-desktop/) | Native desktop host using winit + wgpu | [![crates.io](https://img.shields.io/crates/v/canvas-desktop.svg)](https://crates.io/crates/canvas-desktop) |

## Why This Exists

The current UI paradigm is **human-centric control**: users click buttons, navigate menus, and tell computers *how* to do things.

Saorsa Canvas implements the **third UI paradigm**—intent-based outcome specification:
- User expresses *what* they want
- AI determines *how* to achieve it
- Canvas displays the *result* and captures *feedback*

This is especially powerful for **AI-mediated video calls** where:
- Your video feed is composited into the canvas, not a separate window
- You can touch the screen while speaking: "Change THIS part" becomes spatially aware
- Two users share a synchronized canvas with AI mediating the visual conversation

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
canvas-core = "0.1"      # Scene graph and state management
canvas-renderer = "0.1"  # GPU rendering (optional: disable default features for WASM)
```

Or install the server binary:

```bash
cargo install canvas-server
```

## Quick Start

```bash
# Build everything
cargo build --release

# Run the canvas server
./target/release/saorsa-canvas

# Open http://localhost:9473 in your browser
```

## Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                     SAORSA CANVAS                                    │
│                   "Run Anywhere, Render Anywhere"                    │
├─────────────────────────────────────────────────────────────────────┤
│  PRESENTATION: 2D Screen | Holographic | Spatial XR | Terminal      │
├─────────────────────────────────────────────────────────────────────┤
│  RENDERING: wgpu (WebGPU/Vulkan/Metal) → WebGL2 → Canvas2D fallback │
├─────────────────────────────────────────────────────────────────────┤
│  CORE: Rust/WASM - Scene graph, input handling, video compositing   │
├─────────────────────────────────────────────────────────────────────┤
│  MCP: Tools (render, interact, export) + Resources (ui:// scheme)   │
├─────────────────────────────────────────────────────────────────────┤
│  TRANSPORT: WebSocket (local) | WebRTC (P2P) | HTTP/SSE (agents)    │
└─────────────────────────────────────────────────────────────────────┘
```

## Project Structure

```
saorsa-canvas/
├── canvas-core/       # Scene graph, elements, events, state (WASM-compatible)
├── canvas-renderer/   # wgpu rendering with WebGL2/Canvas2D fallbacks
├── canvas-mcp/        # MCP tools and resources for AI integration
├── canvas-app/        # WASM application for web deployment
├── canvas-server/     # Axum server with WebSocket and WebRTC signaling
├── canvas-desktop/    # Native desktop host using winit + wgpu
├── canvas-skill/      # Claude Code skill for CLI usage
├── web/               # PWA frontend (touch, voice, offline)
└── docs/              # Vision, specs, and development plan
    ├── VISION.md           # Full architectural vision
    ├── DEVELOPMENT_PLAN.md # Phased implementation for Claude Code
    └── SPECS.md            # Tracked standards and references
```

## Core Concepts

### AI as Primary Controller

The canvas is a display surface that AI agents write to. Humans are collaborators, not operators:

```
Traditional:  User → clicks button → App responds
Saorsa:       AI renders → User observes → User gestures/speaks → AI interprets → AI updates
```

### Touch + Voice = Spatial Intent

When you touch the canvas while speaking, both inputs are fused:

```
User touches a chart bar while saying: "Make this one red"
                    ↓
Canvas captures: { touch: {x: 150, y: 200, element: "bar-2"}, voice: "Make this one red" }
                    ↓
AI updates: { element: "bar-2", style: { fill: "#ff0000" } }
```

### Universal Rendering

Same WASM core renders to:
- **2D screens** (phone, tablet, desktop, TV)
- **Holographic displays** (Looking Glass)
- **Spatial computing** (VisionOS, Quest, Android XR)
- **Terminal** (sixel/kitty graphics)

## MCP Integration

Saorsa Canvas implements emerging AI-UI standards:

| Tool | Purpose |
|------|---------|
| `canvas_render` | Render charts, images, 3D models, video feeds |
| `canvas_interact` | Report touch/voice input with spatial context |
| `canvas_export` | Export canvas to PNG, JPEG, SVG, PDF |

```json
{
  "tool": "canvas_render",
  "params": {
    "session_id": "default",
    "content": {
      "type": "Chart",
      "data": {
        "chart_type": "bar",
        "data": { "labels": ["Jan", "Feb"], "values": [10, 20] }
      }
    }
  }
}
```

## Communitas Networking & WebRTC

When `COMMUNITAS_MCP_URL` (and optionally `COMMUNITAS_MCP_TOKEN`) are provided, the canvas server now:

- Performs the `network_start` MCP call so the Communitas core dials into the saorsa-gossip (ant-quic) overlay and boots the saorsa-webrtc media stack. You can hint a preferred port via `COMMUNITAS_NETWORK_PORT`.
- Mirrors Communitas WebRTC state into the canvas scene store and emits `call_state` broadcasts; the web status bar shows the active call ID and participant count.
- Disables the legacy browser-to-browser WebRTC signaling path so that production traffic always flows through saorsa-webrtc over ant-quic. (Run without `COMMUNITAS_MCP_URL` to re-enable the legacy signaling flow for local experimentation.)

This ensures every production deployment rides over the same secure transport used by the bootstrap nodes (`saorsa-2` through `saorsa-5`).

## Content Types

| Type | Format | Rendering |
|------|--------|-----------|
| Charts | JSON via plotters | Bar, line, pie, scatter |
| Images | PNG, JPEG, SVG, WebP | GPU-accelerated textures |
| 3D Models | glTF | Embedded viewer |
| Video | WebRTC streams | Live compositing |
| Text | Markdown/plain | Typography via glyphon |

## Development Status

**All 8 Phases Complete** (147 tests passing)

| Phase | Feature | Status |
|-------|---------|--------|
| 1 | Core Rendering Pipeline | ✅ wgpu backend, WASM bindings, render loop |
| 2 | Charts and Images | ✅ Plotters integration, image loading, texture cache |
| 3 | MCP Integration | ✅ Tool/resource handlers, WebSocket broadcast |
| 4 | WebRTC Video | ✅ VideoFeed element, live compositing |
| 5 | A2UI/AG-UI | ✅ Component tree parsing, SSE streaming |
| 6 | Holographic/Spatial | ✅ Looking Glass support, multi-view quilt rendering |
| 7 | Offline Mode | ✅ Operation queue, service worker sync, IndexedDB |
| 8 | Voice Input | ✅ Web Speech API, touch+voice fusion |

See `docs/DEVELOPMENT_PLAN.md` for implementation details.

## Claude Code Integration

Add as a skill to display visuals from CLI:

```bash
# Link the skill
ln -s /path/to/saorsa-canvas/canvas-skill ~/.claude/skills/canvas

# Now Claude Code can render to the canvas
```

## Part of the Saorsa Labs Ecosystem

Saorsa Canvas is the **visual presentation layer** for [Communitas](https://github.com/saorsa-labs/communitas):

```
Communitas (P2P collaboration)
    ├── Text/Voice/Video → libp2p / saorsa-webrtc
    └── Visual Presentation → Saorsa Canvas
```

## Documentation

- **[API.md](docs/API.md)** - Complete API reference (HTTP, MCP, WebSocket)
- **[CONFIGURATION.md](docs/CONFIGURATION.md)** - Environment variables and settings
- **[DEPLOYMENT.md](docs/DEPLOYMENT.md)** - Docker, Kubernetes, and production setup
- **[VISION.md](docs/VISION.md)** - Full architectural vision and rationale
- **[DEVELOPMENT_PLAN.md](docs/DEVELOPMENT_PLAN.md)** - Phased implementation for Claude Code
- **[SPECS.md](docs/SPECS.md)** - Tracked standards and external references

## License

MIT OR Apache-2.0

---

*Building the infrastructure for decentralized AI.*

**Saorsa Labs** | [saorsa.io](https://saorsa.io) | [GitHub](https://github.com/saorsa-labs)
