# Saorsa Canvas - Claude Code Instructions

## Project Overview

Saorsa Canvas is an AI-native visual interface layer. You are continuing development from commit 548f1d2 which established the scaffold.

**Read these documents before starting**:
1. `docs/VISION.md` - Understand what we're building and why
2. `docs/DEVELOPMENT_PLAN.md` - Detailed phased implementation
3. `docs/SPECS.md` - Standards we're implementing

## Current State

✅ **Done**:
- `canvas-core/` - Scene graph, elements, events, transforms (compiles, tested)
- `canvas-renderer/` - Backend trait defined, Canvas2D stub
- `canvas-server/` - Axum server with WebSocket echo
- `canvas-mcp/` - Tool/resource types defined
- `web/` - PWA shell with touch handling
- `canvas-skill/` - Claude Code skill docs

⚠️ **Incomplete**:
- No actual GPU rendering (wgpu backend not implemented)
- Charts don't render (plotters not integrated)
- MCP tools not connected to scene
- WebSocket doesn't sync scene state
- No video compositing
- No offline queue

## Your Mission

Implement the phases in `docs/DEVELOPMENT_PLAN.md` sequentially. Start with **Phase 1: Core Rendering Pipeline**.

## Critical Rules

1. **Run tests after every change**: `cargo test -p <crate>`
2. **Commit after each task**: Use descriptive messages
3. **Maintain code quality**: All existing lints must pass
   - `#![forbid(unsafe_code)]`
   - `#![deny(missing_docs)]`
   - `#![deny(clippy::all)]`
   - `#![deny(clippy::pedantic)]`
4. **Document public APIs**: Every `pub fn` needs rustdoc
5. **Keep WASM size small**: Use `opt-level = "z"` in release

## Build Commands

```bash
# Native build
cargo build --release

# Run tests
cargo test --workspace

# Run server
cargo run -p canvas-server

# WASM build (after Phase 1)
cargo build --release --target wasm32-unknown-unknown -p canvas-core --features wasm
wasm-bindgen --out-dir web/pkg --target web target/wasm32-unknown-unknown/release/canvas_core.wasm

# Check all lints
cargo clippy --workspace -- -D warnings
```

## Phase 1 Tasks (Start Here)

### Task 1.1: Implement WgpuBackend

**File**: `canvas-renderer/src/backend/wgpu.rs`

Create the GPU rendering backend:

```rust
pub struct WgpuBackend {
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    // Add pipeline, vertex buffers, etc.
}
```

**Steps**:
1. Add `gpu` feature to `canvas-renderer/Cargo.toml`
2. Implement `WgpuBackend::new()` - async initialization
3. Implement `RenderBackend` trait
4. Create basic shaders in `canvas-renderer/src/shaders/`
5. Test: Render a colored quad

### Task 1.2: Add Backend Module Structure

**File**: `canvas-renderer/src/backend/mod.rs`

```rust
pub mod canvas2d;

#[cfg(feature = "gpu")]
pub mod wgpu;

pub use canvas2d::Canvas2DBackend;

#[cfg(feature = "gpu")]
pub use self::wgpu::WgpuBackend;
```

### Task 1.3: WASM Bindings

**File**: `canvas-core/src/wasm.rs`

Expose canvas to JavaScript:

```rust
#[wasm_bindgen]
pub struct CanvasApp {
    scene: Scene,
    // renderer will be added after Phase 1 wgpu works
}

#[wasm_bindgen]
impl CanvasApp {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self { ... }
    
    pub fn add_element(&mut self, kind_json: &str) -> String { ... }
    
    pub fn handle_touch(&mut self, x: f32, y: f32, phase: &str) { ... }
    
    pub fn to_json(&self) -> String { ... }
}
```

### Task 1.4: Update Web Frontend

**File**: `web/index.html`

After WASM build works, update to load and use it:

```javascript
import init, { CanvasApp } from './pkg/canvas_core.js';

async function main() {
    await init();
    const app = new CanvasApp();
    // Connect to existing touch handlers
}
```

## Dependencies to Add

When you need them:

```toml
# canvas-renderer/Cargo.toml
[features]
default = []
gpu = ["wgpu", "raw-window-handle"]

[dependencies]
wgpu = { workspace = true, optional = true }
raw-window-handle = { workspace = true, optional = true }
image = "0.25"

# For text rendering (Phase 2)
glyphon = "0.7"
```

## Verification Checkpoints

After Phase 1:
- [ ] `cargo build -p canvas-renderer --features gpu` succeeds
- [ ] Unit test renders a quad and doesn't panic
- [ ] WASM builds without errors
- [ ] Browser loads WASM and calls methods

After Phase 2:
- [ ] Bar chart renders from JSON data
- [ ] Image loads and displays as texture

After Phase 3:
- [ ] `curl localhost:9473/mcp` returns tool list
- [ ] Calling `canvas_render` updates scene
- [ ] WebSocket clients receive scene updates

## Getting Help

If you're stuck:
1. Check `docs/SPECS.md` for reference implementations
2. Look at wgpu examples: https://github.com/gfx-rs/wgpu/tree/trunk/examples
3. Review plotters-wgpu for chart integration patterns
4. Ask for clarification if requirements are ambiguous

## Commit Message Format

```
feat(canvas-renderer): implement wgpu backend initialization

- Add WgpuBackend struct with device, queue, surface
- Create basic quad shader
- Implement RenderBackend trait
- Add gpu feature flag

Closes #123 (if applicable)
```

---

**Start with Phase 1, Task 1.1. The critical path is getting pixels on screen.**
