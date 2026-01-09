# Saorsa Canvas Skill

Display visual content through a universal AI canvas that works on any device.

## Overview

Saorsa Canvas is the visual presentation layer for AI agents. When an AI needs to show something—charts, images, video, 3D models—it renders here. The canvas runs on phones, desktops, holographic displays, and terminals.

**Key Concept**: The AI controls the canvas. Users observe and provide feedback through touch + voice.

## When to Use

Invoke this skill when the user asks to:
- Display a chart, graph, or visualization
- Show an image or diagram
- Render a 3D model
- Create a visual presentation
- Start a video call or screen share
- Annotate or mark up visual content

## Quick Start

```bash
# Start the canvas server
cd ~/Desktop/Devel/projects/saorsa-canvas
cargo run -p canvas-server

# Server starts on http://localhost:9473
# Open in browser to see the canvas
```

## MCP Tools

### canvas_render

Render content to the canvas:

```json
{
  "tool": "canvas_render",
  "params": {
    "session_id": "default",
    "content": {
      "type": "Chart",
      "data": {
        "chart_type": "bar",
        "data": {
          "labels": ["Q1", "Q2", "Q3", "Q4"],
          "values": [100, 150, 120, 180]
        },
        "title": "Quarterly Revenue"
      }
    }
  }
}
```

**Content Types**:
- `Chart` - bar, line, pie, scatter
- `Image` - PNG, JPEG, SVG, WebP (URL or base64)
- `Model3D` - glTF models
- `Video` - WebRTC stream ID
- `Text` - Labels and annotations

### canvas_interact

Report user interaction (touch + voice fusion):

```json
{
  "tool": "canvas_interact",
  "params": {
    "session_id": "default",
    "interaction": {
      "type": "Voice",
      "data": {
        "transcript": "Make this bar red",
        "context_element": "bar-2"
      }
    }
  }
}
```

### canvas_export

Export canvas to file:

```json
{
  "tool": "canvas_export",
  "params": {
    "session_id": "default",
    "format": "png",
    "quality": 90
  }
}
```

## Touch + Voice Interaction

The canvas fuses touch and voice input for spatial intent:

```
User touches element while saying: "Change THIS to blue"
                    ↓
Canvas captures: { touch: {element: "chart-bar-3"}, voice: "Change THIS to blue" }
                    ↓
AI understands: Update element chart-bar-3 color to blue
```

This makes "THIS", "HERE", and "THAT" meaningful in conversation.

## Project Structure

```
saorsa-canvas/
├── canvas-core/       # WASM core: scene graph, events
├── canvas-renderer/   # GPU rendering (wgpu)
├── canvas-server/     # Local HTTP/WebSocket server
├── canvas-mcp/        # MCP tool implementations
├── canvas-skill/      # This skill file
├── web/               # PWA frontend
└── docs/
    ├── VISION.md           # Architecture and philosophy
    ├── DEVELOPMENT_PLAN.md # Implementation roadmap
    └── SPECS.md            # Standards reference
```

## Development

See `CLAUDE.md` in the project root for development instructions.

**Build**:
```bash
cargo build --release
```

**Test**:
```bash
cargo test --workspace
```

**Run**:
```bash
cargo run -p canvas-server
# Open http://localhost:9473
```

## Integration with Communitas

Saorsa Canvas is the visual layer for the Communitas P2P collaboration platform. MCP tools are exposed through the Communitas MCP server, allowing any connected AI agent to render to user canvases.

## Offline Mode

When disconnected:
- View, pan, zoom still work
- Interactions queue locally
- Sync happens on reconnect
- Banner shows offline status

## Future Capabilities

- **Holographic output**: Looking Glass displays via WebXR
- **Spatial computing**: VisionOS, Quest, Android XR
- **Video compositing**: WebRTC feeds as canvas layers
- **Terminal rendering**: Sixel/Kitty graphics for CLI

---

*Part of Saorsa Labs - Building infrastructure for decentralized AI*
