# Phase 1.3: Communitas Bridge

> Resilient bidirectional sync with Communitas MCP server

## Goal

Make the Communitas bridge robust against network failures:
1. Retry failed requests with exponential backoff
2. Reconnect automatically when connection drops
3. Pull remote changes periodically to stay in sync
4. Track connection health for UI feedback

## Current State

### Existing (`communitas.rs`)
- `CommunitasMcpClient` - HTTP/JSON-RPC client
- `fetch_scene()` / `push_scene()` - Scene sync methods
- `spawn_scene_bridge()` - Watches local changes and pushes upstream

### Missing
- No retry on HTTP failure (single attempt, then gives up)
- No reconnect when bridge task fails
- No periodic pull (one-way push only)
- No connection state tracking

## Prerequisites

- [x] Phase 1.1 complete (SceneStore)
- [x] Phase 1.2 complete (WebSocket protocol)
- [x] `spawn_scene_bridge()` exists

---

## Tasks

<task type="auto" priority="p1">
  <n>Add retry with exponential backoff</n>
  <files>
    canvas-server/src/communitas.rs
  </files>
  <action>
    Create a retry helper that wraps async operations:

    1. Add RetryConfig struct:
       - max_attempts: u32 (default 5)
       - initial_delay_ms: u64 (default 100)
       - max_delay_ms: u64 (default 10000)
       - multiplier: f64 (default 2.0)

    2. Create `retry_with_backoff` async function:
       - Takes operation closure and config
       - Retries on CommunitasError::Http only (not RPC errors)
       - Uses exponential backoff with jitter
       - Returns Result with last error on exhaustion

    3. Update `send_rpc` to use retry internally:
       - Wrap the HTTP send/receive in retry_with_backoff
       - Log each retry attempt with tracing::warn

    Use thiserror for errors. No unwrap/expect.
  </action>
  <verify>
    cargo fmt --all -- --check
    cargo clippy -p canvas-server -- -D warnings
    cargo test -p canvas-server sync::
  </verify>
  <done>
    - RetryConfig struct exists with defaults
    - retry_with_backoff function handles HTTP errors
    - send_rpc retries transient failures
    - Tests pass
  </done>
</task>

<task type="auto" priority="p1">
  <n>Add connection state and reconnect logic</n>
  <files>
    canvas-server/src/communitas.rs
  </files>
  <action>
    Add connection health tracking and auto-reconnect:

    1. Add ConnectionState enum:
       - Connected
       - Disconnected { since: Instant, reason: String }
       - Reconnecting { attempt: u32 }

    2. Add BridgeHandle struct:
       - join_handle: JoinHandle<()>
       - state: Arc<RwLock<ConnectionState>>
       - shutdown_tx: oneshot::Sender<()>

    3. Modify spawn_scene_bridge to:
       - Accept shutdown receiver for graceful stop
       - Track connection state
       - On push failure: mark Disconnected, wait with backoff, retry
       - On success after failure: mark Connected
       - Return BridgeHandle instead of JoinHandle

    4. Add BridgeHandle methods:
       - state() -> ConnectionState
       - shutdown() -> stops bridge gracefully

    No unwrap/expect. Use tokio::select for shutdown.
  </action>
  <verify>
    cargo fmt --all -- --check
    cargo clippy -p canvas-server -- -D warnings
    cargo test -p canvas-server communitas::
  </verify>
  <done>
    - ConnectionState enum tracks health
    - BridgeHandle provides state access
    - Bridge reconnects automatically on failure
    - Graceful shutdown works
  </done>
</task>

<task type="auto" priority="p1">
  <n>Add periodic scene pull</n>
  <files>
    canvas-server/src/communitas.rs
  </files>
  <action>
    Add background task to periodically fetch remote scene:

    1. Add PullConfig struct:
       - interval_secs: u64 (default 30)
       - enabled: bool (default true)

    2. Create spawn_scene_pull function:
       - Takes SyncState, CommunitasMcpClient, PullConfig
       - Every interval: fetch_scene() for each known session
       - Compare with local: if remote timestamp > local, apply
       - Use SyncOrigin::Remote when replacing
       - Respect shutdown signal
       - Returns JoinHandle

    3. Add to BridgeHandle:
       - Include pull handle
       - shutdown() stops both push and pull tasks

    4. Create spawn_full_bridge function:
       - Spawns both push and pull tasks
       - Returns unified BridgeHandle

    No unwrap/expect. Handle fetch errors gracefully (log, continue).
  </action>
  <verify>
    cargo fmt --all -- --check
    cargo clippy -p canvas-server -- -D warnings
    cargo test -p canvas-server communitas::
  </verify>
  <done>
    - PullConfig struct exists
    - spawn_scene_pull fetches periodically
    - Remote changes applied with SyncOrigin::Remote
    - spawn_full_bridge creates complete bridge
  </done>
</task>

---

## Test Plan

| # | Test | Expected |
|---|------|----------|
| 1 | Push fails, then succeeds | Retry with backoff, eventually succeeds |
| 2 | Push fails permanently | Marks Disconnected, keeps retrying |
| 3 | Pull interval fires | Fetches remote scene, applies if newer |
| 4 | Call shutdown() | Both tasks stop gracefully |
| 5 | Connection state query | Returns current health status |

## Files Modified

- `canvas-server/src/communitas.rs` - All retry/reconnect/pull logic

## Dependencies

- tokio (async runtime, channels, timers)
- tracing (logging)
- thiserror (error types)

## Out of Scope

- WebSocket-based push from Communitas (future - requires Communitas changes)
- Conflict resolution beyond last-write-wins (future)
- Offline queue replay (Phase 1.4)
