# Phase 1: Server-Side Call API Restoration

**Milestone**: M2 - Communitas/saorsa-webrtc Migration
**Status**: In Progress
**Started**: 2026-01-20

## Objective
Restore client ability to initiate/join calls when `legacy_signaling=false` by:
1. Adding WebSocket message handlers that map to Communitas call APIs
2. Making legacy signaling disable conditional on successful `network_start`

## Task 1: Client Call Request Handlers

### Implementation Plan

**File**: `canvas-server/src/sync.rs`

Add new client message types:
```rust
// New client messages
"start_communitas_call" -> start new call via MCP
"join_communitas_call"  -> join existing call
"leave_communitas_call" -> leave current call
```

**Steps**:
1. Add `CommunitasCallRequest` enum variants to `ClientMessage`
2. Add handler in `handle_sync_socket` that:
   - Validates client is connected
   - Calls appropriate `CommunitasMcpClient` method
   - Returns `call_state` update or error
3. Add `CommunitasCallResponse` server messages

**Key Code Changes**:
- `ClientMessage::StartCommunitasCall { session_id }`
- `ClientMessage::JoinCommunitasCall { call_id }`
- `ClientMessage::LeaveCommunitasCall { call_id }`
- Handler dispatches to `SyncState::start_communitas_call()`, etc.

## Task 2: Conditional Legacy Signaling Disable

### Implementation Plan

**File**: `canvas-server/src/main.rs`

Current (problematic):
```rust
// Always sets client, disabling legacy signaling
sync_state.set_communitas_client(client.clone());
```

Target:
```rust
// Only set client if network_start succeeded
if network_ok {
    sync_state.set_communitas_client(client.clone());
} else {
    // Leave legacy signaling enabled
    tracing::warn!("Communitas network failed, legacy signaling active");
}
```

**Steps**:
1. Capture `network_start()` result
2. Only call `set_communitas_client` on success
3. Add `SyncState::clear_communitas_client()` for re-enabling legacy on failure
4. Add reconnection logic that re-enables Communitas if it recovers

**File**: `canvas-server/src/sync.rs`

Add:
```rust
/// Re-enable legacy signaling by clearing the Communitas client reference
pub fn clear_communitas_client(&self) { ... }

/// Check if Communitas is active
pub fn has_communitas_client(&self) -> bool { ... }
```

## Acceptance Criteria

- [ ] Client can send `start_communitas_call` and receive `call_state` with new call_id
- [ ] Client can send `join_communitas_call` and be added to participants
- [ ] Client can send `leave_communitas_call` and be removed from call
- [ ] If `network_start` fails, `legacy_signaling=true` in welcome message
- [ ] If MCP client drops, legacy signaling auto-re-enables
- [ ] All existing tests pass
- [ ] `cargo clippy` clean

## Files Modified
- `canvas-server/src/sync.rs` - New message handlers and state methods
- `canvas-server/src/main.rs` - Conditional client initialization

## Risks & Mitigations
- **Risk**: Breaking existing call tests → Run full test suite after each change
- **Risk**: Race condition on client enable/disable → Use RwLock properly
