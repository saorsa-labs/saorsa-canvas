# Saorsa Canvas: Development Plan

This document provides a phased implementation roadmap for Claude Code to execute. Each phase builds on the previous and can be implemented incrementally.

---

## Current State Assessment

### What Exists (Commit 548f1d2)

| Component | Status | Notes |
|-----------|--------|-------|
| `canvas-core` | ✅ Solid foundation | Scene graph, elements, events, transforms |
| `canvas-renderer` | ⚠️ Skeleton only | Backend trait defined, no actual rendering |
| `canvas-server` | ⚠️ Basic | WebSocket echo, no MCP integration |
| `canvas-mcp` | ⚠️ Stub | Tool/resource types defined, not connected |
| `web/` | ⚠️ Basic | Touch handling works, canvas draws grid only |
| `canvas-skill` | ✅ Documentation | Ready for Claude Code use |

### What's Missing

1. Actual GPU rendering (wgpu integration)
2. Chart rendering (plotters integration)
3. Image/3D model loading
4. WebRTC video compositing
5. MCP server integration (expose tools via JSON-RPC)
6. Real-time sync between server and web client
7. A2UI component rendering
8. Holographic/WebXR output
9. Offline queue and conflict resolution
10. Voice input bridge

---

## Phase 1: Core Rendering Pipeline (Week 1-2)

**Goal**: Actually render elements to the canvas using wgpu.

### Tasks

#### 1.1 Implement wgpu Backend (`canvas-renderer/src/backend/wgpu.rs`)

```rust
// Create the WgpuBackend struct
pub struct WgpuBackend {
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
}

impl WgpuBackend {
    pub async fn new(window: &impl HasRawWindowHandle) -> RenderResult<Self> {
        // 1. Create wgpu instance
        // 2. Get adapter (prefer high-performance)
        // 3. Create device and queue
        // 4. Create surface
        // 5. Build render pipeline with shaders
    }
}

impl RenderBackend for WgpuBackend {
    fn render(&self, scene: &Scene) -> RenderResult<()> {
        // 1. Create command encoder
        // 2. Begin render pass
        // 3. For each element in scene:
        //    - Compute vertex buffer
        //    - Draw
        // 4. Submit and present
    }
    
    fn resize(&mut self, width: u32, height: u32) -> RenderResult<()> {
        // Reconfigure surface
    }
    
    fn backend_type(&self) -> BackendType {
        BackendType::WebGpu
    }
}
```

#### 1.2 Add Basic Shaders

Create `canvas-renderer/src/shaders/`:

- `quad.wgsl` - Colored rectangles
- `text.wgsl` - Text rendering (use `wgpu-text` or `glyphon`)
- `image.wgsl` - Texture sampling

#### 1.3 Integrate with Web Target

In `web/`, use `wasm-bindgen` to expose the renderer:

```rust
// canvas-core/src/wasm.rs
#[wasm_bindgen]
pub struct CanvasApp {
    scene: Scene,
    renderer: Renderer,
}

#[wasm_bindgen]
impl CanvasApp {
    #[wasm_bindgen(constructor)]
    pub fn new(canvas_id: &str) -> Result<CanvasApp, JsValue> {
        // Get canvas element
        // Initialize wgpu for web
        // Create renderer
    }
    
    pub fn render(&mut self) {
        self.renderer.render(&self.scene).unwrap();
    }
    
    pub fn handle_touch(&mut self, x: f32, y: f32, phase: &str) {
        // Convert to TouchEvent and process
    }
}
```

#### 1.4 Update `web/index.html`

```javascript
import init, { CanvasApp } from './canvas_core.js';

await init();
const app = new CanvasApp('main-canvas');

function render() {
    app.render();
    requestAnimationFrame(render);
}
render();
```

### Deliverables

- [ ] `WgpuBackend` renders colored quads for all `ElementKind` variants
- [ ] WASM build works in Chrome/Firefox
- [ ] Touch events flow from JS → WASM → scene updates → re-render

### Verification

```bash
cd saorsa-canvas
cargo build --release --target wasm32-unknown-unknown -p canvas-core --features wasm
wasm-bindgen --out-dir web/pkg --target web target/wasm32-unknown-unknown/release/canvas_core.wasm
# Serve web/ and verify elements render
```

---

## Phase 2: Charts and Images (Week 2-3)

**Goal**: Render actual charts and images, not just placeholders.

### Tasks

#### 2.1 Chart Rendering with Plotters

Create `canvas-renderer/src/chart.rs`:

```rust
use plotters::prelude::*;
use plotters_canvas::CanvasBackend;

pub fn render_chart_to_texture(
    chart_type: &str,
    data: &serde_json::Value,
    width: u32,
    height: u32,
) -> RenderResult<Vec<u8>> {
    // Create in-memory image buffer
    let mut buffer = vec![0u8; (width * height * 4) as usize];
    
    {
        let root = BitMapBackend::with_buffer(&mut buffer, (width, height))
            .into_drawing_area();
        
        match chart_type {
            "bar" => draw_bar_chart(&root, data)?,
            "line" => draw_line_chart(&root, data)?,
            "pie" => draw_pie_chart(&root, data)?,
            _ => return Err(RenderError::UnsupportedChartType),
        }
    }
    
    Ok(buffer)
}
```

#### 2.2 Image Loading

```rust
// canvas-renderer/src/image.rs
use image::GenericImageView;

pub fn load_image(src: &str) -> RenderResult<TextureData> {
    let bytes = if src.starts_with("data:") {
        // Base64 decode
        decode_data_url(src)?
    } else {
        // HTTP fetch (async)
        todo!("Async image loading")
    };
    
    let img = image::load_from_memory(&bytes)?;
    Ok(TextureData {
        width: img.width(),
        height: img.height(),
        data: img.to_rgba8().into_raw(),
    })
}
```

#### 2.3 Texture Management

```rust
// canvas-renderer/src/texture_cache.rs
pub struct TextureCache {
    textures: HashMap<String, wgpu::Texture>,
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
}

impl TextureCache {
    pub fn get_or_create(&mut self, key: &str, data: &TextureData) -> &wgpu::Texture {
        self.textures.entry(key.to_string()).or_insert_with(|| {
            self.create_texture(data)
        })
    }
}
```

### Deliverables

- [ ] Bar, line, pie charts render from JSON data
- [ ] PNG/JPEG/WebP images load and display
- [ ] Textures are cached (no reload on every frame)

### Verification

```rust
// Add test in canvas-renderer
#[test]
fn test_bar_chart_renders() {
    let data = serde_json::json!({
        "labels": ["A", "B", "C"],
        "values": [10, 20, 15]
    });
    let pixels = render_chart_to_texture("bar", &data, 400, 300).unwrap();
    assert!(!pixels.iter().all(|&b| b == 0)); // Not all black
}
```

---

## Phase 3: MCP Integration (Week 3-4)

**Goal**: Expose canvas tools via MCP JSON-RPC, connect to Communitas.

### Tasks

#### 3.1 MCP Server Implementation

Create `canvas-mcp/src/server.rs`:

```rust
use rmcp::{Server, Tool, Resource};

pub struct CanvasMcpServer {
    scene: Arc<RwLock<Scene>>,
    sessions: HashMap<String, CanvasSession>,
}

impl CanvasMcpServer {
    pub fn tools(&self) -> Vec<Tool> {
        vec![
            Tool::new("canvas_render")
                .description("Render content to the canvas")
                .input_schema(RenderParams::schema()),
            Tool::new("canvas_interact")
                .description("Report user interaction")
                .input_schema(InteractParams::schema()),
            Tool::new("canvas_export")
                .description("Export canvas to image")
                .input_schema(ExportParams::schema()),
        ]
    }
    
    pub fn resources(&self) -> Vec<Resource> {
        vec![
            Resource::new("ui://saorsa/canvas")
                .description("Current canvas state")
                .mime_type("application/json"),
        ]
    }
    
    pub async fn handle_tool_call(&self, name: &str, params: Value) -> ToolResult {
        match name {
            "canvas_render" => {
                let p: RenderParams = serde_json::from_value(params)?;
                self.render(p).await
            }
            // ...
        }
    }
}
```

#### 3.2 Integrate with canvas-server

Update `canvas-server/src/main.rs`:

```rust
use canvas_mcp::CanvasMcpServer;

#[tokio::main]
async fn main() {
    let scene = Arc::new(RwLock::new(Scene::new(800.0, 600.0)));
    let mcp_server = CanvasMcpServer::new(scene.clone());
    
    let app = Router::new()
        .route("/", get(index_handler))
        .route("/ws", get(websocket_handler))
        .route("/mcp", post(move |body| mcp_handler(body, mcp_server.clone())))
        // ...
}

async fn mcp_handler(
    body: Json<JsonRpcRequest>,
    mcp: Arc<CanvasMcpServer>,
) -> Json<JsonRpcResponse> {
    // Route to MCP server
}
```

#### 3.3 Real-time Sync

When the scene changes, broadcast to WebSocket clients:

```rust
// In WebSocket handler
async fn handle_socket(socket: WebSocket, scene: Arc<RwLock<Scene>>) {
    let (mut sender, mut receiver) = socket.split();
    
    // Subscribe to scene changes
    let mut rx = scene.subscribe();
    
    loop {
        tokio::select! {
            // Send scene updates to client
            Ok(update) = rx.recv() => {
                let msg = serde_json::to_string(&update).unwrap();
                sender.send(Message::Text(msg)).await.ok();
            }
            // Handle client input
            Some(Ok(msg)) = receiver.next() => {
                // Process touch/voice events
            }
        }
    }
}
```

### Deliverables

- [ ] MCP tools callable via HTTP POST `/mcp`
- [ ] Scene changes propagate to all connected WebSocket clients
- [ ] `ui://saorsa/canvas` resource returns current scene JSON

### Verification

```bash
# Test MCP tool call
curl -X POST http://localhost:9473/mcp -H "Content-Type: application/json" -d '{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tools/call",
  "params": {
    "name": "canvas_render",
    "arguments": {
      "session_id": "default",
      "content": {
        "type": "Chart",
        "data": {"chart_type": "bar", "data": {"labels": ["A"], "values": [10]}}
      }
    }
  }
}'
```

---

## Phase 4: WebRTC Video Compositing (Week 4-5)

**Goal**: Composite live video feeds into the canvas.

### Tasks

#### 4.1 Add VideoFeed Element Type

Update `canvas-core/src/element.rs`:

```rust
pub enum ElementKind {
    // ... existing variants
    
    /// A live WebRTC video feed
    VideoFeed {
        /// Stream identifier (peer ID or local)
        stream_id: String,
        /// Whether to mirror the video (for local camera)
        mirror: bool,
        /// Crop region (optional)
        crop: Option<CropRect>,
    },
    
    /// A transparent overlay layer for annotations
    OverlayLayer {
        /// Child elements drawn on top of video
        children: Vec<ElementId>,
    },
}
```

#### 4.2 WebRTC Integration (WASM side)

In `web/`, add WebRTC handling:

```javascript
class VideoManager {
    constructor(canvasApp) {
        this.canvasApp = canvasApp;
        this.streams = new Map();
    }
    
    async addLocalCamera() {
        const stream = await navigator.mediaDevices.getUserMedia({ video: true });
        const video = document.createElement('video');
        video.srcObject = stream;
        await video.play();
        
        this.streams.set('local', video);
        this.canvasApp.register_video_stream('local');
    }
    
    getVideoFrame(streamId) {
        const video = this.streams.get(streamId);
        if (!video) return null;
        
        // Draw to offscreen canvas, get ImageData
        const canvas = new OffscreenCanvas(video.videoWidth, video.videoHeight);
        const ctx = canvas.getContext('2d');
        ctx.drawImage(video, 0, 0);
        return ctx.getImageData(0, 0, canvas.width, canvas.height);
    }
}
```

#### 4.3 Video Texture Updates

In `canvas-renderer`, update video textures each frame:

```rust
impl Renderer {
    pub fn update_video_textures(&mut self, video_frames: &HashMap<String, ImageData>) {
        for (stream_id, frame) in video_frames {
            self.texture_cache.update_or_create(
                &format!("video:{}", stream_id),
                frame.width,
                frame.height,
                &frame.data,
            );
        }
    }
}
```

### Deliverables

- [ ] Local camera feed renders as a canvas element
- [ ] Video frames update at 30fps without blocking rendering
- [ ] Annotations can overlay video feed

### Verification

Open canvas in browser, grant camera permission, verify video appears.

---

## Phase 5: A2UI and AG-UI Integration (Week 5-6)

**Goal**: Accept A2UI component trees from agents, stream updates via AG-UI.

### Tasks

#### 5.1 A2UI Component Mapping

Create `canvas-core/src/a2ui.rs`:

```rust
use serde::{Deserialize, Serialize};

/// A2UI component tree (from Google's spec)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct A2UITree {
    pub root: A2UINode,
    pub data_model: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "component")]
pub enum A2UINode {
    #[serde(rename = "container")]
    Container { children: Vec<A2UINode>, layout: String },
    
    #[serde(rename = "text")]
    Text { content: String, style: Option<A2UIStyle> },
    
    #[serde(rename = "image")]
    Image { src: String, alt: Option<String> },
    
    #[serde(rename = "button")]
    Button { label: String, action: String },
    
    #[serde(rename = "chart")]
    Chart { chart_type: String, data: serde_json::Value },
    
    // Saorsa Canvas extension
    #[serde(rename = "video_feed")]
    VideoFeed { stream_id: String },
}

impl A2UITree {
    /// Convert A2UI tree to Saorsa Canvas scene elements
    pub fn to_scene_elements(&self) -> Vec<Element> {
        self.convert_node(&self.root, 0.0, 0.0)
    }
    
    fn convert_node(&self, node: &A2UINode, x: f32, y: f32) -> Vec<Element> {
        match node {
            A2UINode::Text { content, .. } => {
                vec![Element::new(ElementKind::Text {
                    content: content.clone(),
                    font_size: 16.0,
                    color: "#000000".to_string(),
                }).with_transform(Transform { x, y, ..Default::default() })]
            }
            // ... other mappings
        }
    }
}
```

#### 5.2 AG-UI Event Streaming

Add Server-Sent Events endpoint:

```rust
// canvas-server/src/routes.rs
use axum::response::sse::{Event, Sse};
use futures::stream::Stream;

pub async fn ag_ui_stream(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.scene.subscribe();
    
    let stream = async_stream::stream! {
        loop {
            match rx.recv().await {
                Ok(update) => {
                    let event = Event::default()
                        .event("scene_update")
                        .json_data(&update)
                        .unwrap();
                    yield Ok(event);
                }
                Err(_) => break,
            }
        }
    };
    
    Sse::new(stream)
}
```

### Deliverables

- [ ] A2UI JSON tree converts to canvas elements
- [ ] AG-UI SSE endpoint streams scene updates
- [ ] Round-trip: AI sends A2UI → renders → user touches → event streams back

---

## Phase 6: Holographic & Spatial (Week 6-7)

**Goal**: Render to Looking Glass and WebXR devices.

### Tasks

#### 6.1 Looking Glass WebXR Integration

Create `web/looking-glass.js`:

```javascript
import { LookingGlassWebXRPolyfill } from "@lookingglass/webxr";

export class HolographicRenderer {
    constructor(canvasApp) {
        this.canvasApp = canvasApp;
        this.polyfill = new LookingGlassWebXRPolyfill({
            tileHeight: 512,
            numViews: 45,
            targetDiam: 3,
        });
    }
    
    async enterHolographic() {
        const session = await navigator.xr.requestSession('immersive-vr');
        // ... WebXR render loop using same scene graph
    }
}
```

#### 6.2 Multi-View Rendering

The Looking Glass requires rendering the scene from multiple viewpoints (a "quilt"). Add to renderer:

```rust
impl Renderer {
    pub fn render_quilt(&mut self, scene: &Scene, views: u32) -> RenderResult<Texture> {
        let view_size = self.calculate_view_size(views);
        let quilt = self.create_quilt_texture(views, view_size);
        
        for i in 0..views {
            let camera = self.camera_for_view(i, views);
            self.render_to_tile(&quilt, i, scene, &camera)?;
        }
        
        Ok(quilt)
    }
}
```

### Deliverables

- [ ] "Enter Holographic" button in UI
- [ ] Scene renders on Looking Glass display
- [ ] Touch input still works (from primary monitor)

---

## Phase 7: Offline Mode & Sync (Week 7-8)

**Goal**: Full functionality offline with eventual consistency.

### Tasks

#### 7.1 Offline Queue

```rust
// canvas-core/src/offline.rs
pub struct OfflineQueue {
    pending_ops: Vec<Operation>,
    last_sync: Instant,
}

#[derive(Clone, Serialize, Deserialize)]
pub enum Operation {
    AddElement { element: Element, timestamp: u64 },
    UpdateElement { id: ElementId, changes: serde_json::Value, timestamp: u64 },
    RemoveElement { id: ElementId, timestamp: u64 },
    Interaction { event: InputEvent, timestamp: u64 },
}

impl OfflineQueue {
    pub fn enqueue(&mut self, op: Operation) {
        self.pending_ops.push(op);
        self.persist_to_storage();
    }
    
    pub async fn sync(&mut self, connection: &mut Connection) -> SyncResult {
        // Send pending ops
        // Receive remote ops
        // Resolve conflicts (last-write-wins for now)
    }
}
```

#### 7.2 Service Worker Enhancement

Update `web/sw.js`:

```javascript
const CACHE_NAME = 'saorsa-canvas-v1';
const ASSETS = [
    '/',
    '/index.html',
    '/pkg/canvas_core.js',
    '/pkg/canvas_core_bg.wasm',
];

self.addEventListener('fetch', (event) => {
    // Cache-first for assets
    // Network-first for API calls, queue if offline
});

self.addEventListener('sync', (event) => {
    if (event.tag === 'canvas-sync') {
        event.waitUntil(syncOfflineChanges());
    }
});
```

### Deliverables

- [ ] View/pan/zoom works offline
- [ ] Interactions queue locally
- [ ] Sync happens on reconnect
- [ ] Conflict resolution doesn't lose data

---

## Phase 8: Voice Input Bridge (Week 8)

**Goal**: Capture speech, fuse with touch, send to AI.

### Tasks

#### 8.1 Web Speech API Integration

```javascript
// web/voice.js
export class VoiceInput {
    constructor(onResult) {
        this.recognition = new (window.SpeechRecognition || 
                                window.webkitSpeechRecognition)();
        this.recognition.continuous = true;
        this.recognition.interimResults = true;
        
        this.recognition.onresult = (event) => {
            const result = event.results[event.results.length - 1];
            onResult({
                transcript: result[0].transcript,
                confidence: result[0].confidence,
                isFinal: result.isFinal,
            });
        };
    }
    
    start() { this.recognition.start(); }
    stop() { this.recognition.stop(); }
}
```

#### 8.2 Touch + Voice Fusion

```rust
// canvas-core/src/fusion.rs
pub struct InputFusion {
    pending_touch: Option<(TouchEvent, Instant)>,
    fusion_window_ms: u64,
}

impl InputFusion {
    pub fn process(&mut self, event: InputEvent) -> Option<FusedIntent> {
        match event {
            InputEvent::Touch(touch) => {
                self.pending_touch = Some((touch, Instant::now()));
                None
            }
            InputEvent::Voice { transcript, is_final, .. } if is_final => {
                if let Some((touch, ts)) = self.pending_touch.take() {
                    if ts.elapsed().as_millis() < self.fusion_window_ms as u128 {
                        return Some(FusedIntent::SpatialVoice {
                            transcript,
                            location: (touch.primary_touch()?.x, touch.primary_touch()?.y),
                            element_id: touch.target_element,
                        });
                    }
                }
                Some(FusedIntent::VoiceOnly { transcript })
            }
            _ => None,
        }
    }
}
```

### Deliverables

- [ ] Voice button activates speech recognition
- [ ] Touch within 2s of voice is fused
- [ ] Fused intent sent to AI via MCP

---

## Implementation Order

For Claude Code to execute:

```
PHASE 1 (Critical Path):
  1. canvas-renderer/src/backend/wgpu.rs - Implement WgpuBackend
  2. canvas-renderer/src/backend/mod.rs - Export backends
  3. canvas-core/src/wasm.rs - WASM bindings
  4. web/index.html - Load WASM, call render loop

PHASE 2:
  5. canvas-renderer/src/chart.rs - Plotters integration
  6. canvas-renderer/src/image.rs - Image loading
  7. canvas-renderer/src/texture_cache.rs - Texture management

PHASE 3:
  8. canvas-mcp/src/server.rs - MCP server impl
  9. canvas-server/src/routes.rs - MCP HTTP endpoint
  10. canvas-server/src/sync.rs - WebSocket broadcast

PHASE 4:
  11. canvas-core/src/element.rs - Add VideoFeed variant
  12. web/video.js - WebRTC handling
  13. canvas-renderer/src/video.rs - Video texture updates

PHASE 5:
  14. canvas-core/src/a2ui.rs - A2UI parser
  15. canvas-server/src/agui.rs - AG-UI SSE endpoint

PHASE 6:
  16. web/looking-glass.js - WebXR polyfill
  17. canvas-renderer/src/quilt.rs - Multi-view rendering

PHASE 7:
  18. canvas-core/src/offline.rs - Offline queue
  19. web/sw.js - Enhanced service worker

PHASE 8:
  20. web/voice.js - Speech recognition
  21. canvas-core/src/fusion.rs - Touch+voice fusion
```

---

## Testing Strategy

### Unit Tests

```bash
cargo test -p canvas-core
cargo test -p canvas-renderer
cargo test -p canvas-mcp
```

### Integration Tests

```bash
# Start server
cargo run -p canvas-server &

# Run browser tests (using playwright)
npm test -w web
```

### Visual Regression

Use `pixelmatch` to compare rendered frames against baselines.

---

## Dependencies to Add

Update `Cargo.toml`:

```toml
[workspace.dependencies]
# Add these
image = "0.25"
glyphon = "0.7"  # Text rendering for wgpu
async-stream = "0.3"
tokio-stream = "0.1"

# For WASM web-sys features
web-sys = { version = "0.3", features = [
    # Add
    "MediaDevices",
    "MediaStream",
    "MediaStreamTrack",
    "HtmlVideoElement",
    "OffscreenCanvas",
    "OffscreenCanvasRenderingContext2d",
    "ImageData",
    "SpeechRecognition",
    "SpeechRecognitionEvent",
    "SpeechRecognitionResultList",
] }
```

---

## Claude Code Instructions

When implementing each phase:

1. **Read existing code** before modifying
2. **Run tests** after each file change
3. **Commit often** with descriptive messages
4. **Ask for clarification** if requirements are ambiguous
5. **Document public APIs** with rustdoc comments

Start with Phase 1, Task 1.1. The critical path is getting pixels on screen.

---

*Document version: 2.0*
*Last updated: 2026-01-09*
