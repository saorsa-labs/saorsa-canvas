# Phase 4: WebRTC Video Compositing

## Overview

Phase 4 adds live video rendering to the canvas, enabling WebRTC video feeds to be composited with other elements and annotations overlaid.

## Status: IN PROGRESS

## Technical Decisions

- **Video capture**: JavaScript MediaDevices API → OffscreenCanvas → WASM
- **Frame transfer**: Pass RGBA bytes via typed array to avoid copies
- **Texture updates**: Per-frame GPU upload via texture_cache
- **Rendering**: Existing textured_pipeline with video-specific sampler

## Prerequisites

From previous phases:
- ✅ ElementKind::Video defined (canvas-core/src/element.rs:75-87)
- ✅ WebRTC signaling protocol (canvas-server/src/sync.rs)
- ✅ TextureCache with LRU eviction (canvas-renderer/src/texture_cache.rs)
- ✅ WgpuBackend with textured_pipeline (canvas-renderer/src/backend/wgpu.rs)
- ✅ WASM bindings scaffold (canvas-core/src/wasm.rs)

## Tasks

<task type="auto" priority="p1">
  <n>Task 1: Video texture rendering in wgpu backend</n>
  <files>
    canvas-renderer/src/backend/wgpu.rs
    canvas-renderer/src/video.rs (new)
    canvas-renderer/src/lib.rs
  </files>
  <depends></depends>
  <action>
    1. Create canvas-renderer/src/video.rs with:
       - VideoFrameData struct (width, height, rgba_data)
       - VideoTextureManager to track active video textures
       - update_video_texture() method for per-frame GPU upload

    2. In wgpu.rs:
       - Add video_textures: HashMap<String, CachedTexture>
       - Add render_video_element() method
       - In render(), detect Video elements and render with textured_pipeline

    3. Export video module in lib.rs

    Requirements:
    - NO .unwrap() or .expect() in src/
    - Use thiserror for errors
    - Handle missing video streams gracefully (render placeholder)
  </action>
  <verify>
    cargo fmt --all -- --check
    cargo clippy -p canvas-renderer --all-features -- -D warnings
    cargo test -p canvas-renderer --all-features
  </verify>
  <done>
    - VideoTextureManager implemented
    - render_video_element() handles Video elements
    - Missing streams show placeholder (not crash)
    - Tests pass, zero warnings
  </done>
</task>

<task type="auto" priority="p1">
  <n>Task 2: JavaScript video capture module</n>
  <files>
    web/video.js (new)
    web/index.html
  </files>
  <depends>Task 1</depends>
  <action>
    1. Create web/video.js with VideoManager class:
       - addLocalCamera() - getUserMedia, create HTMLVideoElement
       - addRemoteStream(streamId, stream) - for WebRTC peer streams
       - getFrame(streamId) - draw to OffscreenCanvas, return Uint8Array
       - removeStream(streamId)
       - Active stream tracking via Map

    2. Update web/index.html:
       - Import video.js module
       - Create global VideoManager instance
       - Wire to requestAnimationFrame loop for frame updates

    Requirements:
    - Handle getUserMedia permission denial gracefully
    - Clean up video elements on stream removal
    - Log errors to console for debugging
  </action>
  <verify>
    # Manual: Open in browser, check console for errors
    # Manual: Camera permission flow works
    cd web && npx serve . -p 8080
  </verify>
  <done>
    - VideoManager class captures camera
    - getFrame() returns RGBA bytes
    - No console errors on permission deny
    - Cleanup happens on removeStream()
  </done>
</task>

<task type="auto" priority="p1">
  <n>Task 3: WASM video stream bindings</n>
  <files>
    canvas-core/src/wasm.rs
    canvas-core/Cargo.toml
  </files>
  <depends>Task 1, Task 2</depends>
  <action>
    1. Add web-sys features to Cargo.toml:
       - Uint8Array, ImageData (for frame transfer)

    2. In wasm.rs, add to WasmCanvas:
       - register_video_stream(stream_id: &str) - track available streams
       - update_video_frame(stream_id: &str, width: u32, height: u32, data: &[u8])
       - unregister_video_stream(stream_id: &str)
       - get_active_video_streams() -> Vec<String>

    3. Internal storage:
       - video_frames: HashMap<String, VideoFrameData>

    Requirements:
    - Use JsValue for errors (better JS interop)
    - Validate frame dimensions (width*height*4 == data.len())
    - Handle missing stream gracefully
  </action>
  <verify>
    cargo fmt --all -- --check
    cargo clippy -p canvas-core --features wasm -- -D warnings
    cargo build -p canvas-core --target wasm32-unknown-unknown --features wasm
    wasm-bindgen --out-dir web/pkg --target web target/wasm32-unknown-unknown/release/canvas_core.wasm
  </verify>
  <done>
    - WASM bindings compile
    - Frame data flows from JS to WASM
    - Validation catches bad inputs
    - wasm-bindgen generates correct JS types
  </done>
</task>

<task type="auto" priority="p1">
  <n>Task 4: Wire video capture to renderer</n>
  <files>
    web/index.html
    web/video.js
  </files>
  <depends>Task 3</depends>
  <action>
    1. In web/index.html render loop:
       - For each active video stream, get frame from VideoManager
       - Call wasm.update_video_frame() with RGBA data
       - Maintain 30fps cap (skip frames if behind)

    2. Add video element creation:
       - When user clicks "Add Camera", call VideoManager.addLocalCamera()
       - Create Video element in scene with stream_id="local"
       - Render loop picks up frame updates

    3. Add UI controls:
       - "Start Camera" button
       - "Stop Camera" button
       - Visual indicator when camera is active

    Requirements:
    - Don't block render loop on slow frame capture
    - Handle camera stop/restart cleanly
  </action>
  <verify>
    # Manual test in browser:
    # 1. Click "Start Camera"
    # 2. Grant permission
    # 3. Video renders in canvas
    # 4. Click "Stop Camera"
    # 5. Video element shows placeholder
  </verify>
  <done>
    - Camera renders in canvas at ~30fps
    - Start/Stop buttons work
    - Clean state transitions
    - No memory leaks (check DevTools)
  </done>
</task>

<task type="auto" priority="p1">
  <n>Task 5: OverlayLayer rendering</n>
  <files>
    canvas-renderer/src/backend/wgpu.rs
    canvas-renderer/src/lib.rs
  </files>
  <depends>Task 4</depends>
  <action>
    1. In wgpu.rs render():
       - Detect OverlayLayer elements
       - Render children on top of video with alpha blending
       - Respect opacity property

    2. Add to textured_pipeline:
       - Alpha blending mode for overlay
       - Depth ordering (overlays always on top of their parent video)

    3. Test case:
       - Add Text element as child of OverlayLayer
       - Parent OverlayLayer to Video element
       - Text renders on top of video

    Requirements:
    - Handle nested overlays (overlay within overlay)
    - Opacity affects all children
  </action>
  <verify>
    cargo fmt --all -- --check
    cargo clippy -p canvas-renderer --all-features -- -D warnings
    cargo test -p canvas-renderer --all-features
  </verify>
  <done>
    - OverlayLayer renders children on top
    - Opacity works correctly
    - Nesting works
    - Tests pass
  </done>
</task>

<task type="auto" priority="p2">
  <n>Task 6: Unit tests for video pipeline</n>
  <files>
    canvas-renderer/src/video.rs
    canvas-core/src/wasm.rs
  </files>
  <depends>Task 5</depends>
  <action>
    1. In video.rs tests:
       - test_video_frame_data_creation()
       - test_video_texture_manager_update()
       - test_missing_stream_returns_placeholder()
       - test_invalid_frame_dimensions()

    2. In wasm.rs tests (run via wasm-bindgen-test):
       - test_register_unregister_stream()
       - test_update_frame_validation()
       - test_get_active_streams()

    Requirements:
    - All tests must pass
    - Test edge cases (empty frame, zero dimensions)
  </action>
  <verify>
    cargo test -p canvas-renderer --all-features
    # WASM tests require wasm-pack:
    # wasm-pack test --headless --firefox canvas-core
  </verify>
  <done>
    - 5+ video tests in video.rs
    - 3+ WASM tests for bindings
    - All pass
  </done>
</task>

## Exit Criteria

- [ ] Local camera feed renders as a canvas Video element
- [ ] Video frames update at 30fps without blocking rendering
- [ ] OverlayLayer elements render on top of video
- [ ] Start/Stop camera works cleanly
- [ ] All tests pass (cargo test + WASM tests)
- [ ] Zero clippy warnings
- [ ] Code reviewed via /review

## Files Modified

| File | Changes |
|------|---------|
| canvas-renderer/src/video.rs | New - Video frame management |
| canvas-renderer/src/backend/wgpu.rs | Video rendering, overlay support |
| canvas-renderer/src/lib.rs | Export video module |
| canvas-core/src/wasm.rs | Video stream bindings |
| canvas-core/Cargo.toml | web-sys features |
| web/video.js | New - JavaScript video capture |
| web/index.html | Video UI controls, frame loop |

## Test Summary

Target: 10+ new tests
- video.rs: 5 tests (frame data, texture manager, placeholders)
- wasm.rs: 3 tests (registration, validation)
- Integration: 2 tests (end-to-end video rendering)
