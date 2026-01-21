# GSD-Hybrid State - Saorsa Canvas

> Cross-session memory for current work context

## Current Position

| Field | Value |
|-------|-------|
| **Milestone** | M1: Sync & State Unification |
| **Phase** | 1.4 Integration Tests - **PLANNING** |
| **Task** | Plan created, awaiting execution |
| **Last Updated** | 2026-01-19 |
| **Plan File** | `.planning/PLAN-4.md` |

## Interview Decisions

| Topic | Decision | Rationale |
|-------|----------|-----------|
| **Priority** | Sync unification first | Collapse duplicate state stores before adding features |
| **State Model** | SyncState as source of truth | MCP server, WebSocket, AG-UI all read/write through SyncState |
| **Testing** | Integration tests first | Mock HTTP/WebSocket/MCP for Communitas bridge, then unit tests |
| **Agent Autonomy** | Task-isolated with approval | Fresh context per task, ask before major decisions |
| **WebRTC** | Communitas signaling only | saorsa-webrtc-core for signaling, browser handles media |
| **Desktop** | macOS first | winit + wgpu on macOS, other platforms later |
| **Holographic** | Deferred | Structure in place, implement when device available |

## Codebase Foundation

### Implemented & Working
- **canvas-core**: Scene graph, elements, transforms, events, offline queue
- **canvas-renderer**: wgpu backend, Canvas2D fallback, chart rendering, image loading
- **canvas-server**: Axum HTTP/WebSocket, MCP endpoint, AG-UI SSE
- **canvas-mcp**: 8 MCP tools implemented (render, interact, export, clear, add/remove/update/get)
- **canvas-app**: WASM bindings (WasmCanvas class)
- **web/**: PWA shell with touch handling

### Incomplete / Stubbed
- **canvas-desktop**: Minimal lib.rs placeholder (no app yet)
- **WebRTC video**: ElementKind::VideoFeed defined, no frame handling
- **Holographic**: QuiltRenderSettings structure, no render execution
- **Integration tests**: canvas-server/tests/ empty

## Session History

### Session: 2026-01-19 (Phase 1.3 Complete)
- **Phase 1.3**: Communitas Bridge - Resilient sync
- **Task 1**: Added RetryConfig with exponential backoff and jitter
- **Task 2**: Added ConnectionState enum and BridgeHandle for health tracking
- **Task 3**: Added PullConfig and spawn_scene_pull for periodic remote fetching
- Created spawn_full_bridge for bidirectional push/pull sync
- Updated shutdown() to gracefully stop both tasks
- **Result**: Communitas bridge now has retry, reconnect, and periodic pull
- Note: wiremock tests fail on macOS due to sandbox (not code issue)
- Next: Phase 1.4 (Integration Tests)

### Session: 2026-01-19 (Phase 1.2 Complete)
- **Phase 1.2**: WebSocket Protocol implementation
- **Task 1**: Removed sendEvent() restriction that blocked mutation messages
- **Task 2**: Created sendMutation() helper with acknowledgment handling
- **Task 3**: Wired toolbar actions (add-bar, add-pie, etc.) to WebSocket
- **Task 4**: Updated ack/error handlers to resolve pending callbacks
- **Task 5**: Added error toast UI for user feedback
- **Result**: UI mutations now persist to server and sync across tabs
- Next: Phase 1.3 (Communitas Bridge)

### Session: 2026-01-19 (Phase 1.1 Complete)
- Explored codebase structure
- Conducted interview (7 decisions recorded)
- Created .planning/ structure
- Created Phase 1 plan (PLAN-1.md)
- Fixed canvas-desktop build blocker (added lib.rs)
- **Task 1**: Created SceneStore in canvas-core (248 lines, 14 tests)
- **Task 2**: Refactored SyncState to use SceneStore
- **Task 3**: Refactored CanvasMcpServer to use shared SceneStore
- Fixed clippy warning in canvas-app
- **Result**: MCP and WebSocket now share same SceneStore instance

## Blockers

1. ~~**canvas-desktop missing src/**: Prevents `cargo build --workspace`~~
   - **RESOLVED**: Added minimal lib.rs placeholder

## Handoff Context

Ready to execute Phase 1.4: Integration Tests. Create mock services and
end-to-end sync tests for the unified state system.

Key files to create/modify:
- `canvas-server/tests/` - Integration test suite
- Mock Communitas MCP server
- WebSocket sync round-trip tests
- MCP tool → WebSocket broadcast tests

Phase 1.3 Communitas Bridge is complete with:
- `RetryConfig` - Exponential backoff with jitter
- `PullConfig` - Periodic scene fetching
- `ConnectionState` - Health tracking enum
- `BridgeHandle` - Unified push/pull management
- `spawn_full_bridge()` - Creates bidirectional bridge
