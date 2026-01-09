# Saorsa Canvas

**Universal AI Visual Output Canvas**

A canvas that runs on ANY compute device (x86, ARM, RISC-V), ANY OS, enabling AI to display visual content—charts, 3D models, video calls—wherever you are.

## Vision

When AI needs to show you something, it renders here. Touch the canvas while speaking to interact: "change THIS part" becomes spatially aware.

## Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                     SAORSA CANVAS                                    │
│                    "Run Anywhere, Render Anywhere"                   │
├─────────────────────────────────────────────────────────────────────┤
│  CORE: WebAssembly + Rust                                            │
│  - Scene graph, state management, input handling                     │
│  - Compiles to WASM for true universal portability                   │
├─────────────────────────────────────────────────────────────────────┤
│  RENDERING: Custom minimal renderer on wgpu                          │
│  - WebGPU → WebGL2 → 2D fallback                                     │
│  - Progressive enhancement based on device capabilities              │
├─────────────────────────────────────────────────────────────────────┤
│  DELIVERY: PWA + Local Server                                        │
│  - Embedded Axum server (localhost only)                             │
│  - Works offline, no cloud dependency                                │
├─────────────────────────────────────────────────────────────────────┤
│  COMMUNICATION: WebRTC (saorsa-webrtc)                               │
│  - Bidirectional: touch → AI, render commands ← AI                   │
│  - Lowest latency for real-time interaction                          │
└─────────────────────────────────────────────────────────────────────┘
```

## Features

- **Universal**: Same WASM runs on desktop, mobile, web, smart glasses
- **Touch + Voice**: Direct manipulation with voice commands
- **Offline-first**: Graceful degradation when disconnected
- **MCP Native**: Extends Communitas MCP for AI integration

## Content Types

| Type | Format | Stage 1 |
|------|--------|---------|
| Charts | plotters.rs | ✓ |
| Images | PNG, JPEG, SVG | ✓ |
| 3D Models | glTF | ✓ |
| Video/WebRTC | Live streams | ✓ |

## Quick Start

```bash
# Build
cargo build --release

# Run
./target/release/saorsa-canvas

# Open http://localhost:9473 in your browser
```

## Project Structure

```
saorsa-canvas/
├── canvas-core/       # WASM core (scene graph, state)
├── canvas-renderer/   # wgpu rendering backend
├── canvas-server/     # Axum local server
├── canvas-mcp/        # MCP tools/resources
├── canvas-skill/      # Claude Code skill
└── web/               # PWA frontend
```

## Claude Code Integration

Add as a skill to display visuals from CLI:

```bash
# In your ~/.claude/skills/ directory
ln -s /path/to/saorsa-canvas/canvas-skill ~/.claude/skills/canvas
```

## License

MIT OR Apache-2.0

## Part of Saorsa Labs

Building the infrastructure for decentralized AI.
