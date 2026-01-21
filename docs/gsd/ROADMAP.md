# Saorsa Canvas Roadmap

## Milestone 1: Core Canvas Implementation (COMPLETE)
*Phases 1.1-1.4 complete - Scene graph, rendering, WebSocket sync*

## Milestone 2: Communitas/saorsa-webrtc Migration

### Phase 1: Server-Side Call API
- **Status**: Pending
- **Tasks**:
  1. Add client message handlers for Communitas calls (`start_communitas_call`, `join_communitas_call`, `leave_communitas_call`)
  2. Conditional `set_communitas_client` only after successful `network_start`
  3. Auto-fallback to legacy signaling if Communitas fails

### Phase 2: Upstream Participant Registration
- **Status**: Pending
- **Tasks**:
  1. Wire `SyncState::add_call_participant` to `CommunitasMcpClient::join_call`
  2. Wire `SyncState::remove_call_participant` to `CommunitasMcpClient::end_call`
  3. Propagate call errors to WebSocket clients
  4. Handle MCP client reconnection for active calls

### Phase 3: Web UI Call Controls
- **Status**: Pending
- **Tasks**:
  1. Create Communitas call control component (start/join/leave buttons)
  2. Display active call state from server-pushed updates
  3. Gate legacy SignalingManager behind `DEV_MODE` flag
  4. Add call participant list UI

## Milestone 3: Beta Distribution
*Make saorsa-canvas a downloadable, installable application for Claude Code beta testing*

### Phase 1: Release Workflow
- **Status**: In Progress
- **Tasks**:
  1. Create GitHub Actions release workflow (builds on v* tags)
  2. Asset bundling script for cross-platform releases
  3. Update Cargo.toml workspace metadata
  4. Test release workflow with v0.2.0-alpha.1
  5. Add RELEASE.md documentation

### Phase 2: Install Script
- **Status**: Pending
- **Tasks**:
  1. Create install.sh with OS/arch detection
  2. Implement launchd plist for macOS daemon
  3. Implement systemd unit for Linux daemon
  4. Add uninstall functionality

### Phase 3: MCP Integration
- **Status**: Pending
- **Tasks**:
  1. Auto-configure Claude Code MCP settings
  2. Tool discovery and registration
  3. Session management for MCP calls

### Phase 4: Documentation
- **Status**: Pending
- **Tasks**:
  1. Quick Start README (3-step install)
  2. Usage guide for Claude Code integration
  3. Troubleshooting guide

### Phase 5: Beta Release
- **Status**: Pending
- **Tasks**:
  1. Cut v0.2.0-beta.1 release
  2. End-to-end testing on macOS and Linux
  3. Gather beta tester feedback

## Milestone 4: Production Hardening (Future)
- Error recovery and reconnection
- Call quality metrics
- Multi-device support

## Completed
- **M1 Phase 1.1**: Scene graph foundation
- **M1 Phase 1.2**: Canvas2D rendering
- **M1 Phase 1.3**: WebSocket sync protocol
- **M1 Phase 1.4**: Call state management basics
