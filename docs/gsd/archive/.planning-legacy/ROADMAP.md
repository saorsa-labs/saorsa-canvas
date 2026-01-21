# GSD-Hybrid Roadmap - Saorsa Canvas

> High-level milestone and phase tracking

## Milestones Overview

| # | Milestone | Status | Target |
|---|-----------|--------|--------|
| M1 | Sync & State Unification | **COMPLETE** | - |
| M2 | Native Desktop & Rendering | **COMPLETE** | - |
| M3 | WebRTC & Media Integration | **COMPLETE** | - |
| M4 | Production Readiness | **COMPLETE** | - |

---

## M1: Sync & State Unification

**Goal**: Single source of truth for all scene state across MCP, WebSocket, AG-UI, and HTTP.

### Phases

| Phase | Name | Status | Tasks |
|-------|------|--------|-------|
| 1.1 | State Consolidation | **DONE** | SceneStore shared between MCP & WebSocket |
| 1.2 | WebSocket Protocol | **DONE** | Toolbar actions wired to WebSocket mutations |
| 1.3 | Communitas Bridge | **DONE** | Retry, reconnect, periodic pull |
| 1.4 | Integration Tests | **DONE** | Mock services, end-to-end sync tests |

### Phase 1.1: State Consolidation (COMPLETE - 2026-01-19)
- [x] Audit current state stores (CanvasMcpServer scene vs SyncState)
- [x] Create shared SceneStore type wrapping Arc<RwLock<Scene>>
- [x] Refactor canvas-mcp tools to read/write through SceneStore
- [x] Refactor WebSocket handlers to use SceneStore
- [x] Refactor AG-UI endpoint to read from SceneStore
- [x] Remove duplicate scene storage from CanvasMcpServer

### Phase 1.2: WebSocket Protocol (COMPLETE - 2026-01-19)
- [x] Define message types for add/update/remove from UI (server already supported)
- [x] Implement client-to-server message handling (sendMutation helper)
- [x] Wire toolbar actions in web/index.html to WebSocket
- [x] Add message acknowledgment flow (pendingAcks map + callbacks)
- [x] Handle conflicts (last-write-wins via server)

### Phase 1.3: Communitas Bridge (COMPLETE - 2026-01-19)
- [x] Add retry logic with exponential backoff (RetryConfig)
- [x] Implement reconnect on disconnect (ConnectionState, BridgeHandle)
- [x] Add upstream scene push after local changes (spawn_scene_bridge)
- [x] Add periodic scene pull from Communitas (spawn_scene_pull, PullConfig)
- [x] Add spawn_full_bridge for bidirectional sync

### Phase 1.4: Integration Tests (COMPLETE - 2026-01-19)
- [x] Create TestServer harness for integration tests
- [x] Test WebSocket sync round-trip (add/remove element)
- [x] Test MCP tool calls update WebSocket clients (ignored due to reqwest env issue)
- [x] Multi-client broadcast tests (2-3 clients receive same updates)
- [x] Session isolation tests (clients in different sessions don't see each other)
- [x] Add canvas-server/tests/ integration test suite (websocket_integration.rs, sync_broadcast.rs)

---

## M2: Native Desktop & Rendering

**Goal**: Working canvas-desktop app with full GPU rendering on macOS.

### Phases

| Phase | Name | Status | Tasks |
|-------|------|--------|-------|
| 2.1 | Desktop Scaffold | **DONE** | Fix canvas-desktop build, winit setup |
| 2.2 | GPU Surface | **DONE** | Error recovery, DPI, visual feedback |
| 2.3 | Scene Rendering | **DONE** | Textured quads, chart/image rendering |
| 2.4 | Communitas CLI | **DONE** | CLI flags for MCP connection |

### Phase 2.1: Desktop Scaffold (COMPLETE - 2026-01-19)
- [x] Update canvas-desktop to use workspace winit (0.30)
- [x] Create main.rs with event loop and tracing
- [x] Create app.rs with ApplicationHandler trait implementation
- [x] Add WgpuBackend::from_window() for proper surface initialization
- [x] Test window opens on macOS with white background
- [x] Verify resize and close events work correctly

### Phase 2.2: GPU Surface Robustness (COMPLETE - 2026-01-19)
- [x] Add surface error recovery (Lost/Outdated/Timeout handling)
- [x] Handle ScaleFactorChanged events for Retina displays
- [x] Add visual test pattern (non-white background + test elements)
- [x] Implement suspend/resume lifecycle handling

### Phase 2.3: Scene Rendering (COMPLETE - 2026-01-19)
- [x] Add GPU texture management to WgpuBackend
- [x] Create textured quad shader (textured.wgsl already existed)
- [x] Integrate chart rendering (plotters -> texture -> GPU)
- [x] Integrate image loading (data URI -> texture -> GPU)
- [x] Visual verification with real chart data

### Phase 2.4: Communitas CLI (COMPLETE - 2026-01-19)
- [x] Add CLI argument parsing with clap (--mcp-url, --session, --token, --width, --height)
- [x] Add Communitas MCP client to canvas-desktop (DesktopMcpClient)
- [x] Integrate scene sync on startup (fetch initial scene)
- [x] Graceful fallback to test scene on connection failure

---

## M3: WebRTC & Media Integration

**Goal**: Remote peer video as canvas elements via Communitas signaling.

### Phases

| Phase | Name | Status | Tasks |
|-------|------|--------|-------|
| 3.1 | Signaling Bridge | **DONE** | WebRTC signaling via WebSocket relay |
| 3.2 | Video Elements | **DONE** | Render video frames as textures |
| 3.3 | Media Schema | **DONE** | Canonical bitrate/latency in SceneDocument |

### Phase 3.1: Signaling Bridge (COMPLETE - 2026-01-19)
- [x] Define signaling message types in sync protocol (Offer/Answer/ICE)
- [x] Create RTCPeerConnection handler in JavaScript (signaling.js)
- [x] Implement signaling relay in canvas-server
- [x] Connect peer streams to VideoManager and canvas elements

### Phase 3.2: Video Elements (COMPLETE - 2026-01-19)
- [x] Create CanvasRenderer class for video frame rendering (web/canvas-renderer.js)
- [x] Implement requestAnimationFrame render loop at 60fps
- [x] Render video frames from VideoManager via OffscreenCanvas
- [x] Auto-create video elements when peer streams connect
- [x] Auto-remove video elements when peer streams disconnect
- [x] Grid layout positioning for multiple peer videos
- [x] Local camera toggle with visual button state
- [x] Debug overlay showing FPS, stream count, element count
- [x] Keyboard shortcut 'D' for debug toggle
- [x] Console API: window.toggleDebug()

### Phase 3.3: Media Schema (COMPLETE - 2026-01-19)
- [x] Add MediaConfig struct to canvas-core (bitrate, resolution, fps, audio)
- [x] Add Resolution enum (R240p, R360p, R480p, R720p, R1080p)
- [x] Add QualityPreset enum (Auto, Low, Medium, High, Ultra)
- [x] Add MediaStats struct for real-time monitoring (RTT, jitter, packet loss, FPS)
- [x] Update ElementKind::Video with optional media_config field
- [x] Implement setQuality() for fine-grained control via RTCRtpSender.setParameters()
- [x] Implement setQualityPreset() for preset selection
- [x] Implement enableAdaptiveQuality() for automatic quality adjustment
- [x] Add getStats() returning RTT, jitter, packet loss, FPS
- [x] Add startStatsCollection() for periodic stats updates
- [x] Enhanced debug overlay with per-peer media stats display
- [x] Color-coded quality indicators (green/yellow/red)

---

## M4: Production Readiness

**Goal**: Observability, security, documentation for deployment.

### Phases

| Phase | Name | Status | Tasks |
|-------|------|--------|-------|
| 4.1 | Observability | **DONE** | Metrics, structured tracing, health checks |
| 4.2 | Security | **DONE** | Input validation, CORS restriction, rate limiting |
| 4.3 | Documentation | **DONE** | API docs, env vars, deployment guide |

### Phase 4.1: Observability (COMPLETE - 2026-01-19)
- [x] Add structured tracing with tower-http TraceLayer
- [x] Add request ID propagation for distributed tracing correlation
- [x] Add JSON log format support via RUST_LOG_FORMAT=json
- [x] Add #[tracing::instrument] to key handlers
- [x] Add Prometheus metrics endpoint at /metrics
- [x] Add metrics: HTTP requests, WebSocket connections, MCP tools, signaling
- [x] Add Kubernetes liveness probe at /health/live
- [x] Add Kubernetes readiness probe at /health/ready with component checks
- [x] Add backward-compatible /health endpoint

### Phase 4.2: Security (COMPLETE - 2026-01-19)
- [x] Add input validation module (canvas-server/src/validation.rs)
- [x] Validate session_id, element_id, peer_id with safe character patterns
- [x] Validate SDP and ICE candidates for WebRTC signaling
- [x] Integrate validation into HTTP routes with error responses
- [x] Add validation failure metrics for monitoring
- [x] Restrict CORS to localhost origins only (security fix)
- [x] Allow common dev server ports (3000, 5173, 8080)
- [x] Add token bucket rate limiter for WebSocket messages
- [x] Configurable burst (100) and sustained (10/s) rates via env vars
- [x] Return retry-after hints when rate limited
- [x] Add rate limiting metrics for Prometheus

### Phase 4.3: Documentation (COMPLETE - 2026-01-19)
- [x] Create comprehensive API reference (docs/API.md)
- [x] Document all 12 HTTP endpoints with curl examples
- [x] Document all 8 MCP tools with JSON examples
- [x] Document complete WebSocket protocol (scene sync + WebRTC signaling)
- [x] Add TypeScript interfaces for all types
- [x] Create configuration guide (docs/CONFIGURATION.md)
- [x] Document all 7 environment variables with defaults
- [x] Include development and production examples
- [x] Create deployment guide (docs/DEPLOYMENT.md)
- [x] Add multi-stage Dockerfile example
- [x] Add docker-compose.yml example
- [x] Add complete Kubernetes manifests (Deployment, Service, ConfigMap, Secret)
- [x] Document security considerations
- [x] Document monitoring setup with Prometheus
- [x] Add troubleshooting guide
- [x] Update README.md with links to new documentation

---

## Completed Milestones

### M1: Sync & State Unification (2026-01-19)
- Unified scene state across MCP, WebSocket, AG-UI, and HTTP
- Implemented bidirectional Communitas bridge with retry/reconnect
- Added integration test suite

### M2: Native Desktop & Rendering (2026-01-19)
- Working canvas-desktop app with winit + wgpu
- GPU accelerated rendering with textured quads
- Chart and image element support
- Communitas MCP connection via CLI flags

### M3: WebRTC & Media Integration (2026-01-19)
- WebRTC signaling via WebSocket relay
- Peer video rendering as canvas elements at 60fps
- Media schema with quality presets (Resolution, QualityPreset, MediaConfig)
- Real-time stats monitoring (RTT, jitter, packet loss, FPS)
- Adaptive quality control via RTCRtpSender.setParameters()
- Debug overlay with per-peer media stats

### M4: Production Readiness (2026-01-19)
- Prometheus metrics endpoint with HTTP, WebSocket, MCP, and security metrics
- Structured tracing with request ID propagation and JSON log format
- Kubernetes health probes (liveness + readiness)
- Input validation for session IDs, element IDs, peer IDs, SDP, and ICE candidates
- CORS restricted to localhost origins only
- Token bucket rate limiting for WebSocket (100 burst, 10/sec sustained)
- Comprehensive API documentation (docs/API.md)
- Configuration guide with all environment variables (docs/CONFIGURATION.md)
- Deployment guide with Docker and Kubernetes examples (docs/DEPLOYMENT.md)
