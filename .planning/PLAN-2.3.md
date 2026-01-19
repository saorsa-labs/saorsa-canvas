# Phase 2.3: Scene Rendering

> Goal: Render scene elements as textured quads using existing chart/image utilities.

## Prerequisites

- [x] Phase 2.2 complete (GPU surface robustness)
- [x] Chart rendering to RGBA buffers exists (`chart.rs`)
- [x] Image loading utilities exist (`image.rs`)
- [x] Texture cache exists (`texture_cache.rs`)

## Overview

Currently elements render as solid colored quads. This phase connects existing rendering utilities to the GPU pipeline:

1. **Texture Management** - Upload RGBA buffers to GPU textures
2. **Textured Quad Shader** - Render elements using textures
3. **Element Rendering** - Charts render using plotters, images load from source

Text rendering is deferred to a later phase (requires cosmic-text or similar).

---

<task type="auto" priority="p1">
  <n>Add GPU texture management to WgpuBackend</n>
  <files>
    canvas-renderer/src/backend/wgpu.rs
  </files>
  <action>
    Add texture creation and management to WgpuBackend:

    1. Add texture cache field to WgpuBackend:
       ```rust
       texture_cache: HashMap<String, wgpu::Texture>,
       texture_bind_group_layout: wgpu::BindGroupLayout,
       textured_pipeline: wgpu::RenderPipeline,
       ```

    2. Create `texture_from_rgba()` method:
       ```rust
       fn texture_from_rgba(&self, data: &[u8], width: u32, height: u32)
           -> RenderResult<wgpu::Texture>
       ```
       - Create wgpu::Texture with RGBA8UnormSrgb format
       - Write data using queue.write_texture()
       - Return texture handle

    3. Create `get_or_create_texture()` for caching:
       ```rust
       fn get_or_create_texture(&mut self, key: &str, data: &[u8], width: u32, height: u32)
           -> RenderResult<&wgpu::Texture>
       ```

    4. Create texture bind group layout (for textured rendering):
       - Uniform buffer (transform, canvas_size)
       - Texture
       - Sampler

    5. Initialize textured_pipeline in constructors
  </action>
  <verify>
    cargo fmt --all -- --check
    cargo clippy -p canvas-renderer --all-features -- -D warnings
    cargo test -p canvas-renderer
  </verify>
  <done>
    - texture_from_rgba() creates GPU textures from RGBA data
    - Textures cached by element ID
    - Bind group layout supports texture + sampler
    - All clippy/tests pass
  </done>
</task>

---

<task type="auto" priority="p1">
  <n>Create textured quad shader</n>
  <files>
    canvas-renderer/src/shaders/textured_quad.wgsl
  </files>
  <action>
    Create WGSL shader for textured quad rendering:

    1. Vertex shader (same as quad.wgsl):
       - Input: position, uv
       - Uniforms: transform, canvas_size
       - Output: clip_position, tex_coords
       - Transform position from element coords to NDC

    2. Fragment shader:
       - Input: tex_coords
       - Uniforms: texture, sampler
       - Sample texture at tex_coords
       - Return sampled color

    3. Struct definitions:
       ```wgsl
       struct VertexInput {
           @location(0) position: vec2<f32>,
           @location(1) uv: vec2<f32>,
       }

       struct VertexOutput {
           @builtin(position) clip_position: vec4<f32>,
           @location(0) tex_coords: vec2<f32>,
       }

       @group(0) @binding(0) var<uniform> uniforms: QuadUniforms;
       @group(0) @binding(1) var t_texture: texture_2d<f32>;
       @group(0) @binding(2) var s_sampler: sampler;
       ```
  </action>
  <verify>
    cargo build -p canvas-renderer --features gpu
  </verify>
  <done>
    - textured_quad.wgsl exists
    - Shader compiles (validated by wgpu at runtime)
    - Shader samples texture correctly
  </done>
</task>

---

<task type="auto" priority="p1">
  <n>Integrate chart and image rendering</n>
  <files>
    canvas-renderer/src/backend/wgpu.rs,
    canvas-desktop/src/app.rs
  </files>
  <action>
    Connect chart/image utilities to GPU rendering:

    1. Update `render_element_quad()` to detect element kind:
       - For Chart: Call chart::render_chart_to_buffer(), upload as texture
       - For Image: Call image::load_image_from_data_uri(), upload as texture
       - For other types: Use existing colored quad fallback

    2. Add `render_textured_element()` method:
       ```rust
       fn render_textured_element(
           &mut self,
           encoder: &mut wgpu::CommandEncoder,
           view: &wgpu::TextureView,
           element: &Element,
           texture: &wgpu::Texture,
           is_first: bool,
       )
       ```

    3. Create bind group with element's texture:
       - Create texture view
       - Create sampler (linear filtering)
       - Create bind group with uniform buffer + texture + sampler

    4. Update desktop app test scene:
       - Create a chart with real data
       - Verify it renders as an actual chart, not a colored box

    5. Handle element updates:
       - Invalidate cached texture when element changes
       - Re-render chart/reload image on update
  </action>
  <verify>
    cargo fmt --all -- --check
    cargo clippy -p canvas-renderer -p canvas-desktop --all-features -- -D warnings
    cargo test -p canvas-renderer
    cargo run -p canvas-desktop  # Visual verification
  </verify>
  <done>
    - Chart elements render as actual charts (not colored boxes)
    - Image elements render textures
    - Test scene shows visible chart
    - All tests pass
  </done>
</task>

---

## Verification

```bash
# Full verification
cargo fmt --all -- --check
cargo clippy --workspace --all-features -- -D warnings
cargo test --workspace

# Visual test
cargo run -p canvas-desktop
# Should show:
# - Dark background
# - Rendered bar chart (not a blue box)
# - Text element (colored quad until text rendering is added)
```

## Risks

- **Medium**: Chart rendering performance - may need async rendering for large charts
- **Low**: Texture memory usage - mitigated by existing texture cache

## Notes

- Text rendering deferred to Phase 2.4 or later (requires cosmic-text/glyphon)
- Video rendering deferred to M3 (WebRTC integration)
- Group elements render as containers (children rendered recursively)

## Exit Criteria

- [x] Chart elements render as actual charts
- [x] Image elements render from src (data URI or file)
- [x] Texture cache prevents redundant GPU uploads
- [x] Test scene visually confirms chart rendering
- [x] All clippy warnings resolved
- [x] ROADMAP.md updated with Phase 2.3 DONE
