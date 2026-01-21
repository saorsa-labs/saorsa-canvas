# Phase 2.2: GPU Surface Robustness

> Goal: Make wgpu surface handling production-ready with error recovery, DPI awareness, and visual feedback.

## Context

Phase 2.1 established basic surface creation via `WgpuBackend::from_window()` and resize handling. Phase 2.2 hardens this for real-world usage:

1. **Surface error recovery** - Handle lost/outdated surfaces gracefully
2. **Scale factor awareness** - Proper Retina/HiDPI support
3. **Visual feedback** - Render a test pattern to prove the pipeline works
4. **Lifecycle handling** - macOS suspend/resume cycles

## Tasks

### Task 2.2.1: Surface Error Recovery

**File**: `canvas-renderer/src/backend/wgpu.rs`

Handle `SurfaceError` variants in `render()`:

```rust
fn render(&mut self, scene: &Scene) -> RenderResult<()> {
    let output = match surface.get_current_texture() {
        Ok(output) => output,
        Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
            // Reconfigure surface and retry
            self.reconfigure_surface()?;
            surface.get_current_texture()
                .map_err(|e| RenderError::Surface(e.to_string()))?
        }
        Err(wgpu::SurfaceError::Timeout) => {
            tracing::warn!("Surface timeout, skipping frame");
            return Ok(());
        }
        Err(e) => return Err(RenderError::Surface(e.to_string())),
    };
    // ... rest of render
}
```

Add `reconfigure_surface()` method that re-applies current config.

**Acceptance**:
- [ ] `SurfaceError::Lost` triggers reconfigure
- [ ] `SurfaceError::Outdated` triggers reconfigure
- [ ] `SurfaceError::Timeout` logs warning and skips frame
- [ ] No panic on surface errors

### Task 2.2.2: Scale Factor Handling

**File**: `canvas-desktop/src/app.rs`

Handle `ScaleFactorChanged` event in `window_event()`:

```rust
WindowEvent::ScaleFactorChanged { scale_factor, inner_size_writer } => {
    tracing::info!("Scale factor changed to {}", scale_factor);
    // Get new physical size
    if let Some(window) = &self.window {
        let new_size = window.inner_size();
        self.handle_resize(new_size);
    }
}
```

**File**: `canvas-renderer/src/backend/wgpu.rs`

Store and expose scale factor:

```rust
pub struct WgpuBackend {
    // ... existing fields
    scale_factor: f64,
}

impl WgpuBackend {
    pub fn set_scale_factor(&mut self, factor: f64) {
        self.scale_factor = factor;
    }

    pub fn scale_factor(&self) -> f64 {
        self.scale_factor
    }
}
```

**Acceptance**:
- [ ] `ScaleFactorChanged` event handled
- [ ] Renderer stores scale factor
- [ ] Surface reconfigured on scale change

### Task 2.2.3: Visual Test Pattern

**File**: `canvas-desktop/src/app.rs`

Add a test element to the scene on startup:

```rust
impl CanvasDesktopApp {
    pub fn new(config: DesktopConfig) -> Self {
        let mut scene = Scene::new(config.width as f32, config.height as f32);

        // Add test element to verify rendering works
        scene.add_element(Element::new(
            ElementKind::Chart {
                chart_type: ChartType::Bar,
                data: vec![],
            },
            Transform::new(100.0, 100.0, 200.0, 150.0),
        ));

        Self { config, window: None, renderer: None, scene }
    }
}
```

**Alternative**: Change background to a visible color to prove rendering:

```rust
fn init_renderer(&mut self, window: Arc<Window>) -> Result<()> {
    let mut backend = WgpuBackend::from_window(window.clone())?;
    // Set a visible background color (not white)
    backend.set_background_color(0.1, 0.1, 0.15, 1.0); // Dark blue-gray
    // ...
}
```

**Acceptance**:
- [ ] Window shows non-white background OR visible element
- [ ] Visual confirmation that wgpu pipeline is working

### Task 2.2.4: Suspend/Resume Handling

**File**: `canvas-desktop/src/app.rs`

Handle `suspended()` callback in `ApplicationHandler`:

```rust
impl ApplicationHandler for CanvasDesktopApp {
    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        tracing::info!("App suspended");
        // Drop surface to free resources (will recreate on resume)
        if let Some(renderer) = &mut self.renderer {
            renderer.drop_surface();
        }
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // If we have a window but no renderer, recreate it
        if self.window.is_some() && self.renderer.is_none() {
            tracing::info!("Recreating renderer after resume");
            // ... recreate renderer
        }
        // ... existing resumed logic
    }
}
```

**File**: `canvas-renderer/src/backend/wgpu.rs`

Add surface drop method:

```rust
impl WgpuBackend {
    pub fn drop_surface(&mut self) {
        self.surface = None;
        self.surface_config = None;
        tracing::debug!("Surface dropped");
    }

    pub fn has_surface(&self) -> bool {
        self.surface.is_some()
    }
}
```

**Acceptance**:
- [ ] `suspended()` drops surface cleanly
- [ ] `resumed()` recreates surface if needed
- [ ] App survives suspend/resume cycle on macOS

## Verification

```bash
# Build and run
cargo build -p canvas-desktop
cargo run -p canvas-desktop

# Test scenarios:
# 1. Window shows visible background (not pure white)
# 2. Resize window - no crash, surface reconfigures
# 3. Minimize/restore - no crash
# 4. Test on Retina display - correct scaling
```

## Dependencies

None - uses existing wgpu/winit from workspace.

## Risks

- **Low**: Surface lifecycle varies by platform; macOS suspend may not trigger
- **Low**: Scale factor changes rare in practice

## Completion Criteria

- [ ] All 4 tasks implemented and tested
- [ ] No panics on surface errors
- [ ] Visual confirmation of working render pipeline
- [ ] ROADMAP.md Phase 2.2 marked DONE
