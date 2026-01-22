# Phase 6: Holographic & Spatial

## Overview
Implement GPU-accelerated quilt rendering for Looking Glass holographic displays. Focus on simulation mode preview since no hardware is available for testing.

## Technical Decisions
- Breakdown approach: By layer (viewport → camera → scene → composite)
- Task size: Small (1 file, ~50 lines)
- Testing strategy: Unit tests for view cameras, integration test for quilt, visual regression tests
- Dependencies: Builds on existing spatial.rs, quilt.rs, and holographic.rs foundations
- Hardware: Simulation mode only

## Existing Foundation
- `spatial.rs` - Camera, Vec3, Mat4, HolographicConfig with 45-view calculations
- `quilt.rs` - Quilt data structures and layout mapping
- `holographic.rs` - HolographicRenderer base with placeholder gradients
- `web/looking-glass.js` - Complete JS integration with HoloPlay Service
- Web UI has "Enter Holographic" button already wired

## Tasks

<task type="auto" priority="p1">
  <n>Task 1: Multi-viewport support in wgpu backend</n>
  <files>
    canvas-renderer/src/backend/wgpu.rs
  </files>
  <depends></depends>
  <action>
    Add viewport rendering capability to WgpuBackend:

    1. Add viewport parameter to render methods
    2. Implement scissor test for viewport isolation
    3. Create `set_viewport(x, y, width, height)` method
    4. Ensure viewport changes work within single render pass

    The wgpu viewport API uses:
    ```rust
    render_pass.set_viewport(x, y, width, height, min_depth, max_depth);
    render_pass.set_scissor_rect(x, y, width, height);
    ```

    Requirements:
    - NO .unwrap() or .expect() in src/
    - Use existing error types from RenderError
    - Add viewport bounds validation
  </action>
  <verify>
    cargo fmt --all -- --check
    cargo clippy -p canvas-renderer -- -D warnings
    cargo test -p canvas-renderer
  </verify>
  <done>
    - set_viewport() method added to WgpuBackend
    - Scissor test enabled for viewport isolation
    - All tests pass, zero warnings
  </done>
</task>

<task type="auto" priority="p1">
  <n>Task 2: Camera projection integration</n>
  <files>
    canvas-renderer/src/backend/wgpu.rs
  </files>
  <depends>Task 1</depends>
  <action>
    Integrate Camera from spatial.rs with wgpu rendering:

    1. Add camera parameter to render method signature
    2. Create view-projection matrix from Camera
    3. Update QuadUniforms to include view-projection matrix
    4. Modify vertex shader to apply view-projection transform
    5. Add `render_with_camera(scene, camera, viewport)` method

    Camera matrices available from spatial.rs:
    - `camera.view_matrix()` -> Mat4
    - `camera.projection_matrix(aspect)` -> Mat4
    - Combined: projection * view

    Requirements:
    - NO .unwrap() or .expect() in src/
    - Maintain backward compatibility with 2D rendering
    - Camera can be None for orthographic 2D mode
  </action>
  <verify>
    cargo fmt --all -- --check
    cargo clippy -p canvas-renderer -- -D warnings
    cargo test -p canvas-renderer
  </verify>
  <done>
    - render_with_camera() method implemented
    - View-projection matrix passed to shader
    - 2D rendering still works (camera=None)
    - All tests pass, zero warnings
  </done>
</task>

<task type="auto" priority="p1">
  <n>Task 3: View camera unit tests</n>
  <files>
    canvas-renderer/src/spatial.rs
  </files>
  <depends></depends>
  <action>
    Add comprehensive unit tests for the existing camera_for_view function:

    1. Test center view (view 22 of 45) is at base camera position
    2. Test edge views (0 and 44) are at view cone extremes
    3. Test all 45 cameras point toward the focal target
    4. Test view cone angle coverage matches config
    5. Test different HolographicConfig presets (Portrait, 4K)
    6. Test camera distance from target is preserved

    The existing function:
    ```rust
    impl HolographicConfig {
        pub fn camera_for_view(&self, base: &Camera, view_index: u32) -> Camera
    }
    ```

    Requirements:
    - Use assert_relative_eq! for floating point comparisons
    - Add approx crate if not present for float comparison
    - Tests should be deterministic
  </action>
  <verify>
    cargo fmt --all -- --check
    cargo clippy -p canvas-renderer -- -D warnings
    cargo test -p canvas-renderer -- camera
  </verify>
  <done>
    - At least 6 unit tests for camera_for_view
    - Tests cover edge cases and presets
    - All tests pass, zero warnings
  </done>
</task>

<task type="auto" priority="p1">
  <n>Task 4: Per-view scene rendering</n>
  <files>
    canvas-renderer/src/holographic.rs
  </files>
  <depends>Task 1, Task 2</depends>
  <action>
    Replace placeholder gradient rendering with actual scene rendering:

    1. Update HolographicRenderer to use WgpuBackend
    2. Implement render_view(backend, scene, quilt_view) method
    3. Calculate viewport from QuiltView offset and dimensions
    4. Use camera from QuiltView for projection
    5. Render scene elements to the viewport

    Current placeholder in holographic.rs renders colored gradients.
    Replace with:
    ```rust
    pub fn render_view(
        &self,
        backend: &mut WgpuBackend,
        scene: &Scene,
        view: &QuiltView,
    ) -> Result<(), RenderError>
    ```

    Requirements:
    - NO .unwrap() or .expect() in src/
    - Use existing render_with_camera from Task 2
    - Maintain statistics tracking
  </action>
  <verify>
    cargo fmt --all -- --check
    cargo clippy -p canvas-renderer -- -D warnings
    cargo test -p canvas-renderer
  </verify>
  <done>
    - render_view() renders actual scene, not placeholders
    - Viewport and camera from QuiltView used correctly
    - Statistics still tracked
    - All tests pass, zero warnings
  </done>
</task>

<task type="auto" priority="p1">
  <n>Task 5: Quilt texture composition</n>
  <files>
    canvas-renderer/src/quilt.rs
  </files>
  <depends>Task 4</depends>
  <action>
    Implement full quilt rendering that composes all 45 views:

    1. Add render method to Quilt struct
    2. Create quilt texture at correct resolution (e.g., 4096x4096)
    3. Loop through all views, calling render_view for each
    4. Return QuiltRenderTarget with composed texture data

    ```rust
    impl Quilt {
        pub fn render(
            &self,
            backend: &mut WgpuBackend,
            scene: &Scene,
            renderer: &HolographicRenderer,
        ) -> Result<QuiltRenderTarget, RenderError>
    }
    ```

    Requirements:
    - NO .unwrap() or .expect() in src/
    - Handle view count from HolographicConfig
    - Texture format compatible with HoloPlay Service
  </action>
  <verify>
    cargo fmt --all -- --check
    cargo clippy -p canvas-renderer -- -D warnings
    cargo test -p canvas-renderer
  </verify>
  <done>
    - Quilt::render() composes all views into single texture
    - QuiltRenderTarget contains texture data
    - All tests pass, zero warnings
  </done>
</task>

<task type="auto" priority="p1">
  <n>Task 6: WASM bindings for holographic mode</n>
  <files>
    canvas-core/src/wasm.rs
  </files>
  <depends>Task 5</depends>
  <action>
    Expose holographic rendering to JavaScript:

    1. Add holographic mode flag to WasmCanvas
    2. Implement renderQuilt() method that returns quilt image data
    3. Implement getQuiltDimensions() -> (width, height, views)
    4. Implement setHolographicConfig(preset: &str) method

    ```rust
    #[wasm_bindgen]
    impl WasmCanvas {
        pub fn render_quilt(&mut self) -> Result<Vec<u8>, JsValue> {
            // Render quilt, return RGBA pixel data
        }

        pub fn get_quilt_dimensions(&self) -> JsValue {
            // Return { width, height, views } as JS object
        }

        pub fn set_holographic_config(&mut self, preset: &str) {
            // Set Portrait, 4K, etc.
        }
    }
    ```

    Requirements:
    - NO .unwrap() - use .ok_or() with JsValue errors
    - Return JS-friendly types
    - Lazy initialization of holographic renderer
  </action>
  <verify>
    cargo fmt --all -- --check
    cargo clippy -p canvas-core --features wasm -- -D warnings
    cargo test -p canvas-core
  </verify>
  <done>
    - renderQuilt() returns quilt pixel data
    - getQuiltDimensions() returns config info
    - setHolographicConfig() switches presets
    - All tests pass, zero warnings
  </done>
</task>

<task type="auto" priority="p1">
  <n>Task 7: Simulation mode preview</n>
  <files>
    web/index.html
    web/looking-glass.js
  </files>
  <depends>Task 6</depends>
  <action>
    Enable quilt preview in simulation mode without hardware:

    1. Add quilt preview canvas element
    2. Call WASM renderQuilt() when in holographic mode
    3. Display quilt grid with view numbers overlay
    4. Add toggle between "quilt view" and "single view" preview
    5. Show simulated 3D effect using mouse position for view selection

    Simulation features:
    - Quilt grid shows all 45 views tiled
    - Single view mode shows one view at a time
    - Mouse X position selects which view to show (simulates Looking Glass parallax)

    Requirements:
    - Graceful fallback when WASM not available
    - Performance: don't re-render quilt on every mouse move
    - Clear UI indication of simulation mode
  </action>
  <verify>
    # Manual test in browser:
    # 1. Click holographic button
    # 2. See quilt preview (45 views tiled)
    # 3. Toggle to single-view mode
    # 4. Move mouse to change view angle
  </verify>
  <done>
    - Quilt preview displays without Looking Glass hardware
    - Single-view simulation responds to mouse position
    - UI clearly indicates simulation mode
  </done>
</task>

<task type="auto" priority="p1">
  <n>Task 8: Integration tests and visual regression</n>
  <files>
    canvas-renderer/tests/holographic.rs
  </files>
  <depends>Task 5, Task 7</depends>
  <action>
    Create comprehensive tests for holographic rendering:

    1. Integration test: Scene -> Quilt -> Image data
    2. Test quilt dimensions match config
    3. Test each view viewport is correct size
    4. Visual regression: render test scene, compare to baseline
    5. Performance test: measure time to render 45 views

    Test scene should include:
    - Text element
    - Colored rectangle
    - Image (simple test pattern)

    For visual regression:
    - Save baseline images in tests/fixtures/
    - Use image comparison with tolerance for GPU variance

    Requirements:
    - Tests can run in CI (no GPU required - use software renderer)
    - Baselines committed to repo
    - Tolerance for minor rendering differences
  </action>
  <verify>
    cargo fmt --all -- --check
    cargo clippy -p canvas-renderer -- -D warnings
    cargo test -p canvas-renderer -- holographic --nocapture
  </verify>
  <done>
    - Integration test passes
    - Visual regression test with baseline images
    - Performance benchmark documented
    - All tests pass, zero warnings
  </done>
</task>

## Exit Criteria
- [ ] All 8 tasks complete
- [ ] All tests passing
- [ ] Zero clippy warnings
- [ ] Quilt preview works in browser simulation mode
- [ ] Code reviewed via /review
