# Phase 1: Core Rendering Pipeline - Fix and Verify

## Overview
Phase 1 core implementation (WgpuBackend, shaders, WASM bindings) is 90% complete.
This plan focuses on fixing the WASM build and verifying end-to-end rendering works.

## Technical Decisions
- Breakdown approach: By layer (deps -> build -> integrate -> test)
- Task size: Small (~50 lines, 1 file)
- Testing strategy: Unit tests for WASM + Integration tests
- Dependencies: Independent (self-contained)
- Ordering: Foundation first

## Tasks

<task type="auto" priority="p1">
  <n>Task 1: Fix WASM dependencies</n>
  <files>
    canvas-core/Cargo.toml
  </files>
  <depends></depends>
  <action>
    Fix the uuid crate configuration for WASM builds.

    The WASM build fails because uuid needs the `js` feature to generate random
    UUIDs in browser environments using `crypto.getRandomValues()`.

    Steps:
    1. Update uuid dependency to include `js` feature when building for wasm
    2. Use target-specific dependencies or feature flags

    Requirements:
    - NO .unwrap() or .expect() in src/
    - Use conditional compilation if needed
    - Maintain native build compatibility
  </action>
  <verify>
    cargo build -p canvas-core --target wasm32-unknown-unknown --features wasm
    cargo check --workspace
    cargo test -p canvas-core
  </verify>
  <done>
    - WASM build succeeds for canvas-core
    - Native build still works
    - All existing tests pass
    - Zero warnings
  </done>
</task>

<task type="auto" priority="p1">
  <n>Task 2: Build and verify WASM output</n>
  <files>
    Cargo.toml
    canvas-core/Cargo.toml
  </files>
  <depends>Task 1</depends>
  <action>
    Verify the complete WASM build pipeline works.

    Steps:
    1. Build canvas-core with wasm feature
    2. Verify output file exists and has reasonable size
    3. Check for any missing wasm-bindgen annotations
    4. Ensure WasmCanvas is properly exported

    Requirements:
    - Build must produce valid .wasm file
    - No panicking code paths in WASM build
    - Reasonable WASM size (< 5MB uncompressed)
  </action>
  <verify>
    cargo build -p canvas-core --release --target wasm32-unknown-unknown --features wasm
    ls -la target/wasm32-unknown-unknown/release/*.wasm
  </verify>
  <done>
    - Release WASM builds successfully
    - File size is reasonable
    - No build warnings
  </done>
</task>

<task type="auto" priority="p1">
  <n>Task 3: Generate WASM bindings with wasm-bindgen</n>
  <files>
    web/pkg/ (generated output)
  </files>
  <depends>Task 2</depends>
  <action>
    Run wasm-bindgen to generate JavaScript bindings.

    Steps:
    1. Run wasm-bindgen CLI on the compiled .wasm file
    2. Generate web target bindings in web/pkg/
    3. Verify generated .js files export WasmCanvas
    4. Check that TypeScript definitions are generated (if enabled)

    Requirements:
    - Use --target web for ES modules
    - Output to web/pkg/ directory
    - Include .js glue code and .wasm binary
  </action>
  <verify>
    wasm-bindgen --out-dir web/pkg --target web target/wasm32-unknown-unknown/release/canvas_core.wasm
    ls -la web/pkg/
    grep -l "WasmCanvas" web/pkg/*.js || echo "Check exports"
  </verify>
  <done>
    - web/pkg/ contains .wasm and .js files
    - WasmCanvas is exported in JavaScript
    - No binding generation errors
  </done>
</task>

<task type="auto" priority="p1">
  <n>Task 4: Update web frontend to load WASM</n>
  <files>
    web/index.html
    web/main.js (if exists, or create)
  </files>
  <depends>Task 3</depends>
  <action>
    Update the web frontend to properly load and use the WASM module.

    Steps:
    1. Add script tags to load the generated ES module
    2. Initialize WASM with proper async/await
    3. Create WasmCanvas instance
    4. Connect existing touch handlers to WasmCanvas
    5. Set up render loop using requestAnimationFrame

    Requirements:
    - Use dynamic import or ES modules
    - Handle initialization errors gracefully
    - Connect to existing canvas element
    - Preserve existing touch handling code
  </action>
  <verify>
    # Manual verification: Open web/index.html in browser
    # Check console for errors
    # Verify WasmCanvas is instantiated
  </verify>
  <done>
    - Browser loads WASM without errors
    - WasmCanvas instance created
    - Console shows successful initialization
    - No JavaScript errors
  </done>
</task>

<task type="auto" priority="p1">
  <n>Task 5: Add WASM unit tests</n>
  <files>
    canvas-core/src/wasm.rs
    canvas-core/tests/ (if adding test file)
  </files>
  <depends>Task 1</depends>
  <action>
    Add unit tests for WasmCanvas functionality.

    Steps:
    1. Add tests for WasmCanvas::new()
    2. Test scene JSON serialization/deserialization
    3. Test apply_scene_document()
    4. Test connection status tracking
    5. Use wasm-bindgen-test if testing in browser context

    Requirements:
    - Tests must pass with cargo test
    - Cover core WasmCanvas methods
    - Test error paths
  </action>
  <verify>
    cargo test -p canvas-core
    cargo test -p canvas-core --features wasm
  </verify>
  <done>
    - Unit tests for WasmCanvas methods
    - Tests pass in native build
    - Tests cover happy path and error cases
  </done>
</task>

<task type="auto" priority="p1">
  <n>Task 6: Integration test - end-to-end verification</n>
  <files>
    docs/DEVELOPMENT_PLAN.md (update status)
  </files>
  <depends>Task 4, Task 5</depends>
  <action>
    Verify the complete touch -> scene -> render pipeline works.

    Steps:
    1. Open web frontend in browser
    2. Verify canvas renders (even if just background)
    3. Test touch/mouse events are captured
    4. Verify scene state updates
    5. Document any issues found

    Requirements:
    - Must work in Chrome and Firefox
    - Touch events must reach WasmCanvas
    - Scene state must update correctly
    - Update DEVELOPMENT_PLAN.md with Phase 1 completion status
  </action>
  <verify>
    # Manual verification in browser
    # Document results in commit message
  </verify>
  <done>
    - Browser renders canvas
    - Touch events flow through
    - Scene updates correctly
    - Phase 1 marked complete in docs
  </done>
</task>

## Exit Criteria
- [ ] All 6 tasks complete
- [ ] WASM builds without errors
- [ ] Web frontend loads WASM
- [ ] Tests pass
- [ ] Zero clippy warnings
- [ ] Code reviewed via /review
