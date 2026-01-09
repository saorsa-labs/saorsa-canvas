# Saorsa Canvas: Vision & Architecture

## Executive Summary

**Saorsa Canvas** is an AI-native visual interface layer that runs on any hardware, any OS, and renders to any display surface—from a phone screen to a holographic display. It acts as the **universal canvas for AI-human interaction**, where the AI is the primary interface controller and the user participates through voice, touch, and gaze.

This is not a traditional UI framework. It is a **Model Context Protocol (MCP) visual surface** that any AI agent can render to, with humans as collaborative participants in the visual conversation.

## The Vision

### What We're Building

Imagine a video call where:
- The **AI is the interface**—it decides what to show, when, and how
- Your **video feed is composited into the canvas**, not a separate window
- You can **touch the screen while speaking**: "Change THIS part" becomes spatially aware
- The same canvas runs on your **phone, laptop, smart glasses, or holographic display**
- When **two users connect**, they share a canvas with the AI mediating the visual conversation

### Why This Matters

The current paradigm (apps, windows, buttons) is **human-centric control**. The user tells the computer what to do.

The new paradigm is **AI-centric intent**:
- User expresses what they *want*
- AI determines *how* to achieve it
- Canvas displays the *result* and captures *feedback*

This is Jakob Nielsen's "third UI paradigm"—**intent-based outcome specification**—made tangible.

## Core Principles

### 1. AI as Primary Controller

The canvas is not a drawing app. It's a **display surface that AI agents write to**. The human's role shifts from "operator" to "collaborator and critic".

```
┌─────────────────────────────────────────────────────────────┐
│                    TRADITIONAL UI                           │
│  User → clicks button → App responds → User sees result    │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│                    SAORSA CANVAS                            │
│  AI renders → User observes → User gestures/speaks →       │
│  AI interprets → AI updates → continuous loop               │
└─────────────────────────────────────────────────────────────┘
```

### 2. Universal Rendering Surface

One codebase renders to:
- **2D screens** (phone, tablet, desktop, TV)
- **Holographic displays** (Looking Glass, future devices)
- **Spatial computing** (VisionOS, Quest, Android XR)
- **Terminal** (sixel/kitty graphics for CLI tools)

The rendering backend adapts; the scene graph is universal.

### 3. MCP-Native Architecture

Saorsa Canvas implements the emerging **MCP Apps Extension (SEP-1865)** and follows the **ui:// URI scheme** for resources:

```rust
// AI agent renders to canvas via MCP
{
  "tool": "canvas_render",
  "params": {
    "uri": "ui://saorsa/video-call",
    "content": {
      "layout": "split",
      "left": { "type": "VideoFeed", "stream": "local" },
      "right": { "type": "VideoFeed", "stream": "remote" },
      "overlay": { "type": "Annotation", "elements": [...] }
    }
  }
}
```

### 4. Touch + Voice = Spatial Intent

When the user touches the canvas while speaking, both inputs are fused:

```
User touches a chart bar while saying: "Make this one red"
                    ↓
Canvas captures: { touch: {x: 150, y: 200, element_id: "bar-2"}, 
                   voice: "Make this one red" }
                    ↓
AI receives spatially-aware intent
                    ↓
AI updates: { element: "bar-2", style: { fill: "#ff0000" } }
```

This makes "THIS", "HERE", and "THAT" meaningful in human-AI dialogue.

### 5. Video-First Communication

The canvas is designed for **AI-mediated video calls**:

- User's video feed is a canvas layer, not a separate window
- AI can overlay annotations, shared content, or visualizations
- Touch interactions reference canvas coordinates *and* video content
- Multiple users share a synchronized canvas state

## Technical Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                     SAORSA CANVAS ARCHITECTURE                       │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  ┌─────────────────────────────────────────────────────────────┐    │
│  │                  PRESENTATION LAYER                          │    │
│  ├─────────────┬─────────────┬─────────────┬───────────────────┤    │
│  │ 2D Display  │ Holographic │ Spatial XR  │ Terminal (sixel)  │    │
│  │ (web/native)│(Looking Glass)│(WebXR/Vision)│                  │    │
│  └──────┬──────┴──────┬──────┴──────┬──────┴─────────┬─────────┘    │
│         │             │             │                │              │
│  ┌──────┴─────────────┴─────────────┴────────────────┴──────────┐   │
│  │                   RENDER ABSTRACTION                          │   │
│  │  wgpu (WebGPU/Vulkan/Metal/DX12) → WebGL2 → Canvas2D fallback │   │
│  └──────────────────────────────────────────────────────────────┘   │
│                                │                                     │
│  ┌──────────────────────────────────────────────────────────────┐   │
│  │                      CANVAS CORE (Rust/WASM)                  │   │
│  ├───────────────────┬────────────────────┬─────────────────────┤   │
│  │ Scene Graph       │ Input Handler       │ Video Compositor    │   │
│  │ - Elements        │ - Touch events      │ - WebRTC streams    │   │
│  │ - Transforms      │ - Gesture recognition│ - Frame overlay    │   │
│  │ - Hierarchy       │ - Voice bridge      │ - Sync management   │   │
│  ├───────────────────┼────────────────────┼─────────────────────┤   │
│  │ Layout Engine     │ State Machine       │ A2UI Renderer       │   │
│  │ - Constraints     │ - Offline queue     │ - Component mapping │   │
│  │ - Responsive      │ - Conflict resolution│ - Cross-platform   │   │
│  └───────────────────┴────────────────────┴─────────────────────┘   │
│                                │                                     │
│  ┌──────────────────────────────────────────────────────────────┐   │
│  │                    MCP INTEGRATION LAYER                      │   │
│  ├───────────────────┬────────────────────┬─────────────────────┤   │
│  │ MCP-UI Resources  │ AG-UI Protocol     │ A2UI Components     │   │
│  │ (ui:// scheme)    │ (streaming events) │ (portable JSON UI)  │   │
│  ├───────────────────┴────────────────────┴─────────────────────┤   │
│  │ canvas_render | canvas_interact | canvas_export | canvas_call │   │
│  └──────────────────────────────────────────────────────────────┘   │
│                                │                                     │
│  ┌──────────────────────────────────────────────────────────────┐   │
│  │                    TRANSPORT LAYER                            │   │
│  │  WebSocket (local) | WebRTC (P2P) | HTTP/SSE (remote agents) │   │
│  └──────────────────────────────────────────────────────────────┘   │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

## Key Technologies & Standards

### Adopted Standards

| Standard | Purpose | Status |
|----------|---------|--------|
| **MCP Apps Extension (SEP-1865)** | AI-to-UI resource protocol | Proposed, Nov 2025 |
| **A2UI** | Portable agent-driven UI descriptions | Google, Dec 2025 |
| **AG-UI** | Agent-user streaming interaction protocol | CopilotKit, 2025 |
| **WebRTC** | P2P video and data channels | W3C Standard |
| **WebGPU** | Modern GPU API for web | W3C Standard |
| **WebXR** | Spatial/immersive rendering | W3C Standard |

### Core Dependencies

| Crate/Library | Purpose |
|---------------|---------|
| **wgpu** | GPU abstraction (Vulkan/Metal/DX12/WebGPU) |
| **skia-safe** or **tiny-skia** | 2D vector graphics fallback |
| **plotters** | Chart generation |
| **gltf** | 3D model loading |
| **webrtc-rs** / **saorsa-webrtc** | P2P video/data |
| **axum** | Local HTTP/WebSocket server |

### Looking Glass Integration

For holographic output, we integrate with Looking Glass Factory's SDK:

```javascript
// Web target: Looking Glass WebXR polyfill
import { LookingGlassWebXRPolyfill } from "@lookingglass/webxr"
new LookingGlassWebXRPolyfill()

// Now WebXR sessions render to Looking Glass display
```

The same scene graph renders to both 2D and holographic targets.

## Use Cases

### 1. AI-Mediated Video Call

Two users connect through Communitas. The AI:
- Composites both video feeds into a shared canvas
- Displays relevant documents, charts, or annotations
- Captures touch+voice to understand "look at THIS section"
- Manages turn-taking and summarization overlays

### 2. AI Presentation to User

AI needs to explain a concept:
- Renders charts, diagrams, 3D models dynamically
- User can interrupt: "What about over here?" (touch + voice)
- AI adjusts explanation based on spatial feedback
- Works identically on phone, desktop, or holographic display

### 3. Collaborative Data Analysis

User and AI explore a dataset:
- AI renders visualizations
- User touches interesting data points
- AI explains, zooms, correlates
- Natural language + gesture = efficient exploration

### 4. Remote Assistance

Expert guides field worker via canvas:
- Worker's camera feed is a canvas layer
- Expert's annotations overlay live video
- AI mediates (translating, enhancing, logging)
- Works offline with sync when reconnected

## Competitive Landscape

| Solution | Strength | Limitation |
|----------|----------|------------|
| **MCP-UI (community)** | First mover, Shopify backing | Web-only, e-commerce focus |
| **A2UI (Google)** | Portable, well-spec'd | Early stage, no video |
| **AG-UI (CopilotKit)** | Good streaming, state mgmt | Tied to CopilotKit |
| **Vercel AI SDK** | React integration | Web-only, no holographic |
| **visionOS** | Premium spatial computing | Apple-only, $3500 device |

**Saorsa Canvas differentiator**: Universal hardware (down to RPi), holographic-ready, video-first, MCP-native, open source.

## Integration with Communitas

Saorsa Canvas serves as the **visual presentation layer** for Communitas:

```
Communitas (P2P collaboration)
    │
    ├── Text/Voice/Video → libp2p / saorsa-webrtc
    │
    └── Visual Presentation → Saorsa Canvas
            │
            ├── Shared whiteboards
            ├── Document viewing
            ├── Video call compositing
            └── AI annotation overlays
```

The MCP tools in `canvas-mcp` are exposed through the Communitas MCP server, allowing any AI agent in the network to render to connected canvases.

## Success Metrics

1. **Latency**: Touch-to-render < 16ms (60fps), voice-to-response < 200ms
2. **Portability**: Same WASM runs on Raspberry Pi → Mac Studio → Looking Glass
3. **Offline**: Full view/pan/zoom without network, changes queue for sync
4. **Standards**: 100% MCP Apps Extension compliance
5. **Video**: WebRTC compositing works with 2+ participants

## Next Steps

See `docs/DEVELOPMENT_PLAN.md` for the phased implementation roadmap.

---

*Saorsa Canvas: Where AI meets the eye.*
