# Phase 2.1: Desktop Scaffold

> **Milestone**: M2 - Native Desktop & Rendering
> **Status**: ACTIVE
> **Goal**: Working canvas-desktop app with winit window and wgpu surface on macOS

## Overview

Create a native desktop application that:
1. Opens a window using winit 0.30
2. Creates a wgpu surface from the window
3. Renders an empty scene (white background)
4. Handles resize and close events

## Prerequisites

- Phase 1.4 complete (integration tests passing)
- canvas-renderer has working `WgpuBackend`
- wgpu 24 and winit 0.30 available in workspace

## Tasks

### Task 2.1.1: Fix Dependencies
**Files**: `canvas-desktop/Cargo.toml`

- Use workspace winit (0.30) instead of hardcoded 0.28
- Add required dependencies: tracing-subscriber, raw-window-handle

### Task 2.1.2: Create Event Loop Module
**Files**: `canvas-desktop/src/app.rs`

Create the main application struct:
```rust
pub struct CanvasDesktopApp {
    config: DesktopConfig,
    renderer: Option<WgpuBackend>,
    scene: Scene,
}
```

Implement winit 0.30 `ApplicationHandler` trait:
- `resumed`: Create/recreate wgpu surface
- `window_event`: Handle resize, close, redraw

### Task 2.1.3: Create Main Entry Point
**Files**: `canvas-desktop/src/main.rs`

- Initialize tracing
- Create event loop with `winit::event_loop::EventLoop::new()`
- Create window with configurable size/title
- Run event loop with `CanvasDesktopApp`

### Task 2.1.4: Wire Up wgpu Surface
**Files**: `canvas-renderer/src/backend/wgpu.rs` (if needed)

Current `WgpuBackend::configure_surface` takes a `wgpu::Surface<'static>`.

Add helper for creating surface from winit window:
```rust
pub fn create_surface_from_window(
    instance: &wgpu::Instance,
    window: Arc<Window>,
) -> Result<wgpu::Surface<'static>, ...>
```

### Task 2.1.5: Integration Test
**Test**: Run `cargo run -p canvas-desktop`

Verify:
- [ ] Window opens on macOS
- [ ] White background renders
- [ ] Window resizes without panic
- [ ] Window closes cleanly

## Architecture

```
┌──────────────────────────────────────────────┐
│                canvas-desktop                │
├──────────────────────────────────────────────┤
│  main.rs                                     │
│    └─ EventLoop::new()                       │
│    └─ event_loop.run_app(app)                │
├──────────────────────────────────────────────┤
│  app.rs                                      │
│    └─ CanvasDesktopApp                       │
│         ├─ window: Option<Arc<Window>>       │
│         ├─ renderer: Option<WgpuBackend>     │
│         └─ scene: Scene                      │
├──────────────────────────────────────────────┤
│  ApplicationHandler impl                     │
│    └─ resumed(): create window + surface     │
│    └─ window_event(): resize, close, redraw  │
└──────────────────────────────────────────────┘
         │
         ▼
┌──────────────────────────────────────────────┐
│            canvas-renderer                   │
│  WgpuBackend::configure_surface(surface)     │
│  WgpuBackend::render(&scene)                 │
│  WgpuBackend::resize(w, h)                   │
└──────────────────────────────────────────────┘
```

## winit 0.30 Changes

winit 0.30 uses `ApplicationHandler` trait instead of closures:

```rust
impl ApplicationHandler for CanvasDesktopApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // Create window and surface here
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::Resized(size) => { ... }
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => { ... }
            _ => {}
        }
    }
}
```

## Acceptance Criteria

1. `cargo build -p canvas-desktop` succeeds
2. `cargo run -p canvas-desktop` opens a window
3. Window shows white background (GPU rendered)
4. Resize works without artifacts
5. Close button exits cleanly

## Dependencies

```toml
[dependencies]
canvas-core = { path = "../canvas-core" }
canvas-renderer = { path = "../canvas-renderer", features = ["gpu"] }
anyhow.workspace = true
winit.workspace = true
tracing.workspace = true
tracing-subscriber.workspace = true
```

## Notes

- Surface creation requires `Arc<Window>` for 'static lifetime
- wgpu 24 may need specific surface handling for macOS Metal
- Raw window handle is provided by winit automatically
