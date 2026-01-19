# GSD-Hybrid Issues - Saorsa Canvas

> Deferred work backlog by priority

## P0: Blockers (Immediate)

### ~~ISSUE-001: canvas-desktop build failure~~
- **Component**: canvas-desktop
- **Status**: **RESOLVED** (2026-01-19)
- **Description**: Cargo.toml exists but no src/lib.rs or src/main.rs
- **Impact**: Prevents `cargo build --workspace`
- **Fix**: Added minimal lib.rs placeholder
- **Resolution**: Created `canvas-desktop/src/lib.rs` with DesktopConfig struct

---

## P1: Next Phase

### ~~ISSUE-002: Duplicate state stores~~
- **Component**: canvas-mcp, canvas-server
- **Status**: **RESOLVED** (2026-01-19)
- **Description**: CanvasMcpServer maintains its own scene separate from SyncState
- **Impact**: MCP changes don't reflect in WebSocket; state diverges
- **Fix**: Refactor to shared SceneStore
- **Resolution**: Created SceneStore in canvas-core, both MCP and SyncState now share it

### ~~ISSUE-003: UI mutations not synced upstream~~
- **Component**: web/index.html, canvas-server
- **Status**: **RESOLVED** (2026-01-19)
- **Description**: Toolbar actions modify local state only
- **Impact**: Changes lost on refresh, not shared with peers
- **Fix**: Wire to WebSocket add/update/remove messages
- **Resolution**: Created sendMutation() helper with ack handling, wired all toolbar actions

---

## P2: This Milestone (M1)

### ~~ISSUE-004: Communitas reconnect logic missing~~
- **Component**: canvas-server/src/communitas.rs
- **Status**: **RESOLVED** (2026-01-19)
- **Description**: No retry on connection failure, no reconnect on disconnect
- **Impact**: Temporary network issues break sync permanently
- **Fix**: Add exponential backoff retry, reconnect handler
- **Resolution**: Added RetryConfig, ConnectionState, BridgeHandle, PullConfig, spawn_full_bridge

### ISSUE-005: Missing integration tests
- **Component**: canvas-server/tests/
- **Status**: Open
- **Description**: Tests directory exists but empty
- **Impact**: No automated verification of sync behavior
- **Fix**: Add mock services and integration test suite
- **Assignee**: M1 Phase 1.4

---

## P3: Future Milestones

### ISSUE-006: Holographic render execution
- **Component**: canvas-renderer/src/holographic.rs
- **Status**: Deferred
- **Description**: QuiltRenderSettings defined but no multi-view rendering
- **Impact**: Looking Glass devices not supported
- **Fix**: Implement render loop when device available
- **Assignee**: Future (deferred per interview)

### ISSUE-007: Voice input integration
- **Component**: canvas-core/src/fusion.rs, web/
- **Status**: Deferred
- **Description**: InputFusion struct exists but no Web Speech API connection
- **Impact**: Voice commands not available
- **Fix**: Add speech recognition in frontend, WebSocket voice events
- **Assignee**: Future

### ISSUE-008: Cross-platform desktop builds
- **Component**: canvas-desktop
- **Status**: Deferred
- **Description**: macOS first, Linux/Windows later
- **Impact**: Desktop app only on macOS initially
- **Fix**: Add platform-specific surface configuration
- **Assignee**: After M2 completion

---

## Resolved

(None yet)
