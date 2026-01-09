# Saorsa Canvas: Standards & Specifications

This document tracks the emerging standards, protocols, and external projects that inform Saorsa Canvas development. Keep this updated as the ecosystem evolves.

---

## Core Standards

### MCP Apps Extension (SEP-1865)

**Status**: Proposed (November 2025)  
**Source**: [GitHub Discussion](https://github.com/modelcontextprotocol/specification/discussions/1865)  
**Maintainers**: Anthropic, OpenAI, Shopify, community

The MCP Apps Extension standardizes interactive UI within Model Context Protocol:

- **UI Templates as Resources**: `ui://` URI scheme for canvas templates
- **Intent-Based Messaging**: Components bubble up intents to agent, not direct state mutation
- **Prefetchable Templates**: Security and performance through pre-approval
- **Render Data System**: CSS styling for brand customization

**Key Design Principles**:
1. Agent maintains control (UI doesn't bypass agent)
2. Intents are atomic and declarative
3. Templates are sandboxed
4. Progressive enhancement for capabilities

**Our Implementation**:
- `canvas-mcp/src/resources.rs` - `canvas://` URI scheme (maps to `ui://`)
- `canvas-mcp/src/tools.rs` - Intent handlers (`canvas_render`, `canvas_interact`)

---

### A2UI (Agent-to-UI Protocol)

**Status**: Open Project (December 2025)  
**Source**: [GitHub - nicholasaleks/a2ui](https://github.com/nicholasaleks/a2ui) (Google-affiliated)  
**Paper**: "Agent-to-UI: A Framework for Cross-Platform Agent-Driven Interfaces"

A2UI separates UI structure from implementation:

```json
{
  "component": "container",
  "layout": "vertical",
  "children": [
    { "component": "text", "content": "Hello", "style": { "fontSize": 24 } },
    { "component": "button", "label": "Click", "action": "submit" }
  ]
}
```

**Key Features**:
- Framework-agnostic JSON UI descriptions
- Client maps to native widgets (React, Flutter, SwiftUI, etc.)
- Same payload renders across platforms
- "Safe like data, expressive like code"

**Our Implementation**:
- `canvas-core/src/a2ui.rs` (Phase 5) - Parse A2UI trees, map to canvas elements

---

### AG-UI (Agent-User Interaction Protocol)

**Status**: Active (2025)  
**Source**: [CopilotKit AG-UI](https://github.com/CopilotKit/ag-ui)  
**Spec**: Event-based protocol for agentic frontends

AG-UI handles streaming interaction between agents and UIs:

- **Event Streaming**: WebSocket/SSE for live updates
- **Typed Attachments**: Files, images, audio with MIME types
- **Stable Components**: Render model output as typed UI elements
- **State Synchronization**: Event-sourced diffs with conflict resolution
- **Handoffs**: Typed transitions between agent and frontend actions

**Event Types**:
```typescript
type AGUIEvent = 
  | { type: 'token', content: string }
  | { type: 'component', id: string, tree: A2UINode }
  | { type: 'state_update', path: string[], value: any }
  | { type: 'action_request', name: string, params: object }
  | { type: 'handoff', to: 'agent' | 'user', context: object }
```

**Our Implementation**:
- `canvas-server/src/agui.rs` (Phase 5) - SSE endpoint for event streaming

---

## Rendering Technologies

### wgpu

**Status**: Stable (v24.x)  
**Source**: [wgpu.rs](https://wgpu.rs/)  
**Spec**: WebGPU API implementation in Rust

Cross-platform GPU abstraction supporting:
- Vulkan (Linux, Windows, Android)
- Metal (macOS, iOS)
- DirectX 12 (Windows)
- WebGPU (browsers via wasm)
- OpenGL (fallback)

**Our Usage**:
- `canvas-renderer/src/backend/wgpu.rs` - Primary render backend
- Target: WebGPU in browser, native elsewhere

### WebGPU

**Status**: W3C Standard (2024)  
**Source**: [W3C WebGPU Spec](https://www.w3.org/TR/webgpu/)  
**Browser Support**: Chrome 113+, Firefox 121+, Safari 18+

Modern GPU API for the web, successor to WebGL:
- Compute shaders
- Better resource management
- Closer to native APIs

### Skia / tiny-skia

**Status**: Stable  
**Source**: [Skia](https://skia.org/), [tiny-skia](https://github.com/AmanCEO/tiny-skia)

2D graphics fallback when GPU unavailable:
- Used by Chrome, Android, Flutter
- `tiny-skia` is pure Rust, no system dependencies

**Our Usage**:
- `canvas-renderer/src/backend/canvas2d.rs` - CPU fallback renderer

---

## Holographic & Spatial Computing

### Looking Glass WebXR

**Status**: Stable  
**Source**: [Looking Glass Bridge SDK](https://docs.lookingglassfactory.com/developer-tools/webxr)  
**npm**: `@lookingglass/webxr`

WebXR polyfill for Looking Glass holographic displays:

```javascript
import { LookingGlassWebXRPolyfill } from "@lookingglass/webxr"
new LookingGlassWebXRPolyfill()
// Adds "Enter Looking Glass" button to WebXR sessions
```

**Key Concepts**:
- **Quilt**: Grid of views rendered from different angles
- **Light Field**: Display reconstructs 3D from quilt
- Works with Three.js, Babylon.js, React Three Fiber

**Our Implementation**:
- `web/looking-glass.js` (Phase 6) - WebXR integration
- `canvas-renderer/src/quilt.rs` (Phase 6) - Multi-view rendering

### WebXR Device API

**Status**: W3C Standard  
**Source**: [W3C WebXR](https://www.w3.org/TR/webxr/)  
**Browser Support**: Chrome, Edge, Firefox (Quest browser), Safari (visionOS)

Unified API for VR and AR:
- Immersive sessions (VR headsets, AR glasses)
- Inline sessions (3D in page)
- Input sources (controllers, hands, gaze)

### visionOS / RealityKit

**Status**: Apple Platform (2024+)  
**Source**: [Apple Developer - visionOS](https://developer.apple.com/visionos/)

Spatial computing for Apple Vision Pro:
- SwiftUI + RealityKit for 3D UI
- Eye tracking, hand gestures, voice input
- Spatial anchors for persistent placement

**Relevance**: Future native visionOS app alongside web canvas

### Android XR

**Status**: Announced (2025)  
**Source**: [Android XR](https://developer.android.com/xr)

Google/Samsung spatial computing platform:
- Project Moohan headset
- Open ecosystem (vs. Apple's closed)
- WebXR support expected

---

## Video & Communication

### WebRTC

**Status**: W3C Standard  
**Source**: [W3C WebRTC](https://www.w3.org/TR/webrtc/)  
**Rust**: [webrtc-rs](https://github.com/webrtc-rs/webrtc)

Peer-to-peer real-time communication:
- Video/audio streams
- Data channels
- NAT traversal (ICE, STUN, TURN)

**Our Usage**:
- Video feed compositing into canvas
- P2P data sync for shared sessions
- Integration with `saorsa-webrtc` crate

### Insertable Streams API

**Status**: WICG Draft  
**Source**: [WebRTC Insertable Streams](https://w3c.github.io/webrtc-insertable-streams/)

Process video frames before encoding/after decoding:

```javascript
const { readable, writable } = sender.createEncodedStreams();
readable
  .pipeThrough(new TransformStream({ transform: processFrame }))
  .pipeTo(writable);
```

**Our Usage**: Add overlays/annotations to video frames

### OffscreenCanvas

**Status**: Standard  
**Source**: [WHATWG OffscreenCanvas](https://html.spec.whatwg.org/multipage/canvas.html#the-offscreencanvas-interface)

Canvas rendering off main thread:

```javascript
const offscreen = new OffscreenCanvas(width, height);
const ctx = offscreen.getContext('2d');
// Render in worker, transfer to main thread
```

**Our Usage**: Video frame processing without blocking UI

---

## AI Avatar & Telepresence

### Real-Time Video Avatars

**Projects to Watch**:
- **Lemon Slice 2**: Single image → conversational video character
- **Beyond Presence**: Hyper-realistic AI agents, sub-second latency
- **AKOOL Streaming Avatar**: Custom avatars for video calls
- **Tavus CVI**: Conversational video interface

**Key Capabilities**:
- Video diffusion transformers generate pixels on-the-fly
- Lip-sync to audio input
- Facial expression transfer
- Real-time (< 200ms latency)

**Relevance**: Future AI avatar rendering in canvas

### RAVATAR / Holobox

**Source**: [RAVATAR](https://ravatar.com/)

Holographic AI avatars:
- RAVABOX display system
- Historical figure recreations
- Bilingual service assistants

**Relevance**: Holographic avatar target for Looking Glass

---

## UI Paradigm Research

### Third UI Paradigm (Nielsen)

**Source**: Jakob Nielsen, "AI: First New UI Paradigm in 60 Years" (2023)  
**Link**: [NN/g Article](https://www.nngroup.com/articles/ai-paradigm/)

Three paradigms of human-computer interaction:
1. **Batch Processing** (1945+): Submit job, wait for output
2. **Command-Based** (1984+): Direct manipulation, WIMP
3. **Intent-Based** (2024+): Specify outcome, AI determines method

**Key Insight**: Locus of control shifts from user to AI. User says *what*, AI figures out *how*.

**Our Application**: Canvas is the AI's output surface; user provides feedback, not commands.

### Generative UI

**Source**: Various (Vercel AI SDK, Microsoft Agent Framework)

AI dynamically constructs interfaces based on:
- User intent
- Context and history
- Available capabilities
- Real-time behavior analysis

**Pattern**:
```
User input → AI prediction → UI generation → Rendering → User feedback → Loop
```

---

## Related Projects

### Vercel AI SDK 3.0

**Source**: [Vercel AI SDK](https://sdk.vercel.ai/)

- Unified APIs across AI providers
- React Server Components integration
- Streaming responses
- Generative UI patterns

### Microsoft Agent Framework

**Source**: [Microsoft AutoGen](https://github.com/microsoft/autogen)

"Golden Triangle":
- DevUI (debugging agents)
- AG-UI (user interaction)
- OpenTelemetry (observability)

### CopilotKit

**Source**: [CopilotKit](https://www.copilotkit.ai/)

Open-source AI copilot framework:
- AG-UI protocol origin
- React integration
- Tool execution UI

---

## Specification Tracking

| Spec | Version | Last Checked | Status |
|------|---------|--------------|--------|
| MCP Apps Extension | Draft | 2025-11 | Monitor |
| A2UI | 0.1 | 2025-12 | Implement Phase 5 |
| AG-UI | 1.0 | 2025-01 | Implement Phase 5 |
| WebGPU | CR | 2024-04 | Using via wgpu |
| WebXR | REC | 2024-03 | Phase 6 |
| Looking Glass WebXR | 0.5.x | 2024-12 | Phase 6 |
| WebRTC | REC | 2024-01 | Phase 4 |

---

## Action Items

1. [ ] Subscribe to MCP Apps Extension discussion for updates
2. [ ] Review A2UI spec changes monthly
3. [ ] Test WebXR on Quest 3 and Vision Pro
4. [ ] Evaluate Looking Glass Go for portable holographic demos
5. [ ] Monitor Android XR announcements for WebXR compatibility

---

*Last updated: 2026-01-09*
