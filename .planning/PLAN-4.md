# Phase 1.4: Integration Tests

> End-to-end sync verification with real server connections

## Goal

Create a comprehensive integration test suite that verifies:
1. WebSocket connections and message round-trips
2. MCP tool calls propagating to WebSocket clients
3. Multi-client sync (changes broadcast to all subscribers)
4. Offline queue replay

## Current State

### Existing (`canvas-server/tests/`)
- `websocket_sync_tests.rs` - JSON format tests only (no real connections)
- Unit tests in `sync.rs`, `routes.rs`, `agui.rs`, `communitas.rs`

### Missing
- Real WebSocket client connections
- Server startup in test harness
- MCP → WebSocket broadcast verification
- Multi-client sync tests
- Offline queue replay tests

## Prerequisites

- [x] Phase 1.1 complete (SceneStore)
- [x] Phase 1.2 complete (WebSocket protocol)
- [x] Phase 1.3 complete (Communitas bridge)

---

## Tasks

<task type="auto" priority="p1">
  <n>Add test dependencies and test server harness</n>
  <files>
    canvas-server/Cargo.toml,
    canvas-server/tests/common/mod.rs,
    canvas-server/tests/common/server.rs
  </files>
  <action>
    Set up test infrastructure for integration tests:

    1. Add dev-dependencies to Cargo.toml:
       - tokio-tungstenite = "0.24" (WebSocket client)
       - futures-util = "0.3" (stream utilities)
       - portpicker = "0.1" (find available ports)

    2. Create tests/common/mod.rs:
       - pub mod server;

    3. Create tests/common/server.rs with TestServer struct:
       - start() -> TestServer - spawns server on random port
       - addr() -> SocketAddr - returns server address
       - ws_url() -> String - returns ws://addr/ws URL
       - mcp_url() -> String - returns http://addr/mcp URL
       - sync_state() -> SyncState - access to shared state
       - shutdown() - graceful stop

    4. TestServer::start implementation:
       - Use portpicker::pick_unused_port()
       - Create SyncState with SceneStore
       - Build router with routes::create_router()
       - Spawn axum server with graceful shutdown
       - Return TestServer with handle and addresses

    No unwrap/expect in production code.
    Use #[allow(dead_code)] sparingly for test utilities.
  </action>
  <verify>
    cargo fmt --all -- --check
    cargo clippy -p canvas-server -- -D warnings
    cargo build -p canvas-server --tests
  </verify>
  <done>
    - Test dependencies added
    - tests/common/mod.rs and server.rs exist
    - TestServer can start and stop cleanly
    - Code compiles without warnings
  </done>
</task>

<task type="auto" priority="p1">
  <n>WebSocket round-trip integration tests</n>
  <files>
    canvas-server/tests/websocket_integration.rs
  </files>
  <action>
    Create real WebSocket integration tests:

    1. Create tests/websocket_integration.rs:
       - mod common; for test harness

    2. Test: connect_and_receive_welcome
       - Start TestServer
       - Connect WebSocket client
       - Verify Welcome message received with version, session_id

    3. Test: subscribe_and_receive_scene
       - Connect, send subscribe message
       - Verify SceneUpdate received with empty scene

    4. Test: add_element_round_trip
       - Connect and subscribe
       - Send add_element message with text element
       - Verify Ack received with element ID
       - Verify SceneUpdate contains new element

    5. Test: remove_element_round_trip
       - Add element first
       - Send remove_element with element ID
       - Verify Ack received
       - Verify SceneUpdate shows element removed

    6. Test: ping_pong
       - Connect
       - Send ping message
       - Verify pong response with timestamp

    Use tokio_tungstenite::connect_async for WebSocket.
    Use futures_util::StreamExt and SinkExt for message handling.
    Set reasonable timeouts (5 seconds) with tokio::time::timeout.
  </action>
  <verify>
    cargo fmt --all -- --check
    cargo clippy -p canvas-server -- -D warnings
    cargo test -p canvas-server websocket_integration:: -- --test-threads=1
  </verify>
  <done>
    - 5+ WebSocket integration tests pass
    - Tests use real TCP connections
    - Welcome, subscribe, add, remove, ping all verified
  </done>
</task>

<task type="auto" priority="p1">
  <n>Multi-client sync and MCP broadcast tests</n>
  <files>
    canvas-server/tests/sync_broadcast.rs
  </files>
  <action>
    Test that changes propagate across clients:

    1. Create tests/sync_broadcast.rs:
       - mod common;

    2. Test: multi_client_broadcast
       - Start TestServer
       - Connect two WebSocket clients, both subscribe to "default"
       - Client 1 sends add_element
       - Verify Client 2 receives ElementAdded broadcast
       - Verify both clients see same scene state

    3. Test: mcp_changes_broadcast_to_websocket
       - Start TestServer
       - Connect WebSocket client and subscribe
       - Make HTTP POST to /mcp with canvas_add_element tool call
       - Verify WebSocket client receives SceneUpdate with new element

    4. Test: scene_isolation_by_session
       - Connect two clients to different sessions ("session-a", "session-b")
       - Add element to session-a
       - Verify session-b client does NOT receive the update

    5. Test: late_subscriber_gets_current_state
       - Client 1 connects, subscribes, adds element
       - Client 2 connects later and subscribes
       - Verify Client 2's initial SceneUpdate contains the element

    For MCP calls, use reqwest::Client with JSON-RPC 2.0 format.
    Include message_id tracking for ack verification.
  </action>
  <verify>
    cargo fmt --all -- --check
    cargo clippy -p canvas-server -- -D warnings
    cargo test -p canvas-server sync_broadcast:: -- --test-threads=1
  </verify>
  <done>
    - Multi-client broadcast verified
    - MCP → WebSocket propagation works
    - Session isolation confirmed
    - Late subscriber receives current state
  </done>
</task>

---

## Test Plan

| # | Test | Expected |
|---|------|----------|
| 1 | Connect to WebSocket | Welcome message received |
| 2 | Subscribe to session | SceneUpdate with current state |
| 3 | Add element | Ack + SceneUpdate with element |
| 4 | Remove element | Ack + SceneUpdate without element |
| 5 | Ping | Pong response |
| 6 | Multi-client broadcast | Second client sees first's changes |
| 7 | MCP → WebSocket | MCP tool calls broadcast to WS clients |
| 8 | Session isolation | Different sessions don't interfere |
| 9 | Late subscriber | Gets current state on subscribe |

## Files Created/Modified

- `canvas-server/Cargo.toml` - Add test dependencies
- `canvas-server/tests/common/mod.rs` - Test module
- `canvas-server/tests/common/server.rs` - Test server harness
- `canvas-server/tests/websocket_integration.rs` - WS round-trip tests
- `canvas-server/tests/sync_broadcast.rs` - Multi-client tests

## Dependencies

- tokio-tungstenite (WebSocket client)
- futures-util (stream utilities)
- portpicker (random port selection)
- reqwest (already present for MCP tests)

## Out of Scope

- Performance/load testing (future)
- Communitas bridge integration tests (wiremock issues on macOS)
- Offline queue replay (complex setup, defer to M2)
