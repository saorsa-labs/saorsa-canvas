# Phase 5: A2UI and AG-UI Integration

## Overview
Accept A2UI component trees from agents and stream updates via AG-UI Server-Sent Events.

## Technical Decisions
- Breakdown approach: By layer (Types → Logic → Integration)
- Task size: Medium (1-2 files, ~100 lines per task)
- Testing strategy: Unit tests for types, integration tests for streaming
- Dependencies: Builds on Phase 3 MCP foundation

## Tasks

<task type="auto" priority="p1">
  <n>Task 1: Define A2UI types and parsing</n>
  <files>
    canvas-core/src/a2ui.rs
    canvas-core/src/lib.rs
  </files>
  <depends></depends>
  <action>
    Create A2UI component types following Google's A2UI spec:

    1. Create `canvas-core/src/a2ui.rs` with:
       - `A2UITree` struct with root node and data_model
       - `A2UINode` enum with variants:
         - Container { children, layout }
         - Text { content, style }
         - Image { src, alt }
         - Button { label, action }
         - Chart { chart_type, data }
         - VideoFeed { stream_id } (Saorsa extension)
       - `A2UIStyle` struct for optional styling
       - `to_scene_elements()` method to convert A2UI tree to Scene elements

    2. Add module to `canvas-core/src/lib.rs`

    Requirements:
    - NO .unwrap() or .expect() in src/
    - Use serde for JSON serialization
    - Document all public types with rustdoc
    - Handle nested containers recursively
  </action>
  <verify>
    cargo fmt --all -- --check
    cargo clippy -p canvas-core -- -D warnings
    cargo test -p canvas-core
  </verify>
  <done>
    - A2UITree and A2UINode types compile
    - Can deserialize A2UI JSON
    - to_scene_elements() converts tree to Elements
    - Unit tests pass for conversion
  </done>
</task>

<task type="auto" priority="p1">
  <n>Task 2: A2UI layout calculation</n>
  <files>
    canvas-core/src/a2ui.rs
  </files>
  <depends>Task 1</depends>
  <action>
    Implement layout calculation for A2UI containers:

    1. Add layout types:
       - `Layout` enum: Row, Column, Grid, Stack
       - Position calculation for children based on layout

    2. Update `convert_node()` to:
       - Calculate child positions based on parent layout
       - Handle nested containers
       - Apply style properties (padding, margin, alignment)

    3. Add layout tests with various configurations

    Requirements:
    - NO .unwrap() or .expect() in src/
    - Support basic flex-like layouts
  </action>
  <verify>
    cargo fmt --all -- --check
    cargo clippy -p canvas-core -- -D warnings
    cargo test -p canvas-core
  </verify>
  <done>
    - Containers lay out children correctly
    - Nested layouts work
    - Row/Column/Stack layouts implemented
    - Tests verify positioning
  </done>
</task>

<task type="auto" priority="p1">
  <n>Task 3: AG-UI SSE endpoint</n>
  <files>
    canvas-server/src/agui.rs
    canvas-server/src/main.rs
  </files>
  <depends>Task 1</depends>
  <action>
    Create AG-UI Server-Sent Events streaming endpoint:

    1. Create `canvas-server/src/agui.rs` with:
       - SceneUpdate event type
       - InteractionEvent type
       - SSE stream handler

    2. Implement SSE endpoint:
       - Subscribe to scene changes
       - Stream JSON events to clients
       - Handle client disconnection gracefully

    3. Add route `/agui/events` to main.rs

    Requirements:
    - Use axum's Sse type
    - Handle backpressure appropriately
    - NO .unwrap() or .expect() in src/
  </action>
  <verify>
    cargo fmt --all -- --check
    cargo clippy -p canvas-server -- -D warnings
    cargo test -p canvas-server
  </verify>
  <done>
    - `/agui/events` endpoint returns SSE stream
    - Scene updates stream to connected clients
    - Multiple clients can subscribe
    - Tests verify event format
  </done>
</task>

<task type="auto" priority="p1">
  <n>Task 4: MCP A2UI render tool</n>
  <files>
    canvas-mcp/src/server.rs
    canvas-mcp/src/tools.rs
  </files>
  <depends>Task 1, Task 3</depends>
  <action>
    Add A2UI rendering support to MCP tools:

    1. Create `canvas_render_a2ui` tool:
       - Accept A2UI JSON tree
       - Convert to scene elements
       - Replace or merge with existing scene

    2. Add tool definition with JSON schema

    3. Wire tool to scene update broadcast

    Requirements:
    - Validate A2UI structure before conversion
    - Return element IDs of created elements
    - NO .unwrap() or .expect() in src/
  </action>
  <verify>
    cargo fmt --all -- --check
    cargo clippy -p canvas-mcp -- -D warnings
    cargo test -p canvas-mcp
  </verify>
  <done>
    - `canvas_render_a2ui` tool works via MCP
    - A2UI tree renders to canvas
    - SSE clients receive scene updates
    - Tests verify round-trip
  </done>
</task>

<task type="auto" priority="p1">
  <n>Task 5: Interaction events to AG-UI</n>
  <files>
    canvas-server/src/agui.rs
    canvas-server/src/websocket.rs
  </files>
  <depends>Task 3</depends>
  <action>
    Stream user interactions through AG-UI:

    1. Define interaction event types:
       - TouchEvent (tap, drag, pinch)
       - ButtonClick (element_id, action)
       - FormInput (element_id, value)

    2. Connect WebSocket touch events to AG-UI stream

    3. Format events per AG-UI protocol spec

    Requirements:
    - Include element_id in all events
    - Include timestamp
    - NO .unwrap() or .expect() in src/
  </action>
  <verify>
    cargo fmt --all -- --check
    cargo clippy -p canvas-server -- -D warnings
    cargo test -p canvas-server
  </verify>
  <done>
    - Touch events stream via AG-UI
    - Button clicks include action identifier
    - Events have proper timestamps
    - Tests verify event format
  </done>
</task>

<task type="auto" priority="p1">
  <n>Task 6: Integration tests and documentation</n>
  <files>
    canvas-core/src/a2ui.rs
    canvas-server/tests/agui_integration.rs
    docs/A2UI.md
  </files>
  <depends>Task 1, Task 2, Task 3, Task 4, Task 5</depends>
  <action>
    Complete integration tests and documentation:

    1. Add comprehensive unit tests for A2UI conversion

    2. Create integration test that:
       - Sends A2UI via MCP
       - Verifies SSE receives scene update
       - Simulates touch, verifies interaction event

    3. Document A2UI support in docs/A2UI.md:
       - Supported components
       - Layout options
       - Extension components (VideoFeed)
       - Examples

    Requirements:
    - Test edge cases (empty tree, deep nesting)
    - Document all public APIs
  </action>
  <verify>
    cargo fmt --all -- --check
    cargo clippy --workspace -- -D warnings
    cargo test --workspace
  </verify>
  <done>
    - All A2UI tests pass
    - Integration test demonstrates round-trip
    - Documentation complete
    - Zero warnings
  </done>
</task>

## Exit Criteria
- [ ] All 6 tasks complete
- [ ] A2UI JSON converts to canvas elements
- [ ] AG-UI SSE streams scene updates
- [ ] Interactions flow back through AG-UI
- [ ] All tests passing
- [ ] Zero clippy warnings
- [ ] Code reviewed via /review
