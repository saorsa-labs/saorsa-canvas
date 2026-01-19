# Phase 3.2: Video Elements

> Goal: Render WebRTC peer video frames as textures on the canvas.

## Prerequisites

- [x] Phase 3.1 complete (WebRTC signaling)
- [x] VideoManager exists with getVideoFrame() API
- [x] ElementKind::Video defined in canvas-core
- [x] Peer streams flow to VideoManager via SignalingManager

## Overview

This phase connects video frame extraction to canvas rendering:

1. **Canvas2D Video Rendering** - Draw video frames using OffscreenCanvas
2. **Video Render Loop** - RequestAnimationFrame-based video updates
3. **Video Element Sync** - Auto-create video elements for connected peers

Architecture:
```
VideoManager                    Canvas Renderer
     |                               |
     | getVideoFrame(streamId)       |
     ▼                               |
 ImageData/Uint8Array ──────────────►| drawImage()
     |                               |
     | each frame (rAF)              |
     ▼                               ▼
Continuous Render Loop      Updated Canvas Display
```

---

<task type="auto" priority="p1">
  <n>Implement Canvas2D video frame rendering</n>
  <files>
    web/index.html,
    web/canvas-renderer.js
  </files>
  <action>
    Create canvas-renderer.js to handle video frame rendering:

    1. Create web/canvas-renderer.js with CanvasRenderer class:
       ```javascript
       export class CanvasRenderer {
           constructor(canvasElement, videoManager) {
               this.canvas = canvasElement;
               this.ctx = canvasElement.getContext('2d');
               this.videoManager = videoManager;
               this.scene = null;
               this.frameId = null;
               this.running = false;
           }

           // Set the scene to render
           setScene(scene) { ... }

           // Start the render loop
           start() { ... }

           // Stop the render loop
           stop() { ... }

           // Main render function called each frame
           render() {
               // Clear canvas
               // Sort elements by z-index
               // For each element:
               //   - Video: draw frame from VideoManager
               //   - Text: draw text
               //   - Image: draw image
               //   - Chart: placeholder
           }

           // Render a single video element
           renderVideoElement(element) {
               const { stream_id, mirror, crop } = element.kind;
               const frame = this.videoManager.getVideoFrame(stream_id, crop);
               if (frame) {
                   // Draw to canvas at element.transform position
                   // Handle mirroring via context transform
               }
           }
       }
       ```

    2. Implement renderVideoElement():
       - Get frame from videoManager.getVideoFrame(streamId, crop)
       - Create ImageBitmap from ImageData for performance
       - Apply transform (x, y, width, height, rotation)
       - Handle mirror flag with context.scale(-1, 1)
       - Draw with ctx.drawImage()

    3. Implement render loop with requestAnimationFrame:
       - Track running state to allow stop/start
       - Only request new frame if running
       - Call render() each frame

    4. Update web/index.html to:
       - Import CanvasRenderer
       - Create CanvasRenderer instance with canvas element and videoManager
       - Call renderer.start() when WebSocket connects
       - Update renderer.setScene() when scene changes
  </action>
  <verify>
    # Manual browser test:
    # 1. Open http://localhost:9473
    # 2. Add local camera via videoManager.addLocalCamera()
    # 3. Create video element pointing to 'local' stream
    # 4. Video should render on canvas
  </verify>
  <done>
    - CanvasRenderer class renders video frames at 60fps
    - Video elements display live video from VideoManager
    - Mirroring and cropping work correctly
    - Render loop can be started/stopped
  </done>
</task>

---

<task type="auto" priority="p1">
  <n>Auto-create video elements for peer streams</n>
  <files>
    web/index.html,
    web/signaling.js
  </files>
  <action>
    Automatically create video elements when peers connect:

    1. Add video element creation helper to index.html:
       ```javascript
       function createVideoElement(streamId, peerId) {
           return {
               id: '',
               kind: {
                   type: 'Video',
                   stream_id: streamId,
                   mirror: false,
                   crop: null
               },
               transform: {
                   x: calculatePeerPosition(peerId).x,
                   y: calculatePeerPosition(peerId).y,
                   width: 320,
                   height: 240,
                   rotation: 0,
                   z_index: 10
               }
           };
       }

       function calculatePeerPosition(peerId) {
           // Grid layout for multiple peers
           const existingPeers = getVideoPeerCount();
           const col = existingPeers % 3;
           const row = Math.floor(existingPeers / 3);
           return {
               x: 50 + col * 350,
               y: 300 + row * 270
           };
       }
       ```

    2. Hook into VideoManager stream changes:
       ```javascript
       videoManager.onStreamChange((action, streamId) => {
           if (action === 'added' && streamId.startsWith('peer-')) {
               const peerId = streamId.replace('peer-', '');
               const element = createVideoElement(streamId, peerId);
               sendMutation('add_element', { element });
           } else if (action === 'removed' && streamId.startsWith('peer-')) {
               // Find and remove the corresponding video element
               const elementId = findVideoElementId(streamId);
               if (elementId) {
                   sendMutation('remove_element', { id: elementId });
               }
           }
       });
       ```

    3. Track video element IDs for cleanup:
       - Map<streamId, elementId> to track created elements
       - Update map when add_element ack received
       - Clean up map when stream removed

    4. Add local camera toolbar button:
       - Toggle button to start/stop local camera
       - Creates/removes 'local' stream and video element
  </action>
  <verify>
    # Manual browser test with two windows:
    # Window A: Start local camera, start call to Window B
    # Window B: Should see Window A's video appear automatically
    # When call ends: Video element should be removed
  </verify>
  <done>
    - Peer video elements auto-created on call connect
    - Video elements auto-removed on call end
    - Grid layout positions multiple peer videos
    - Local camera toggle works
  </done>
</task>

---

<task type="auto" priority="p1">
  <n>Add video rendering tests and debug overlay</n>
  <files>
    web/index.html,
    web/canvas-renderer.js
  </files>
  <action>
    Add debugging tools and verify integration:

    1. Add debug overlay to CanvasRenderer:
       ```javascript
       renderDebugOverlay() {
           if (!this.debugMode) return;

           this.ctx.font = '12px monospace';
           this.ctx.fillStyle = '#00ff00';

           // Show FPS
           const fps = this.calculateFPS();
           this.ctx.fillText(`FPS: ${fps}`, 10, 20);

           // Show active streams
           const streams = this.videoManager.getStreamIds();
           this.ctx.fillText(`Streams: ${streams.length}`, 10, 35);

           // Show element count
           const elements = this.scene?.elements?.length || 0;
           this.ctx.fillText(`Elements: ${elements}`, 10, 50);
       }
       ```

    2. Add keyboard shortcut for debug mode:
       - Press 'D' to toggle debug overlay
       - Expose as window.toggleDebug() for console access

    3. Add video test pattern fallback:
       - When stream not ready, render colored placeholder
       - Show stream ID text on placeholder
       - Helps identify missing/broken streams

    4. Verify complete flow works:
       - Start canvas-server
       - Open two browser windows
       - Start calls between them
       - Verify video renders in both directions
       - Verify cleanup on disconnect

    5. Update console API documentation in index.html:
       ```javascript
       // Console API for testing:
       // videoManager.addLocalCamera() - Start local camera
       // signalingManager.startCall('peer-id') - Call a peer
       // signalingManager.endCall('peer-id') - End a call
       // window.toggleDebug() - Toggle debug overlay
       ```
  </action>
  <verify>
    cargo run -p canvas-server &
    # Open http://localhost:9473 in two windows
    # Window A console: await videoManager.addLocalCamera()
    # Window A console: signalingManager.startCall(window.prompt('Enter peer ID from Window B'))
    # Both windows should show video
    # Press 'D' for debug overlay showing FPS and stream count
  </verify>
  <done>
    - Debug overlay shows FPS, stream count, element count
    - Keyboard shortcut 'D' toggles debug mode
    - Test pattern renders for unavailable streams
    - Complete peer-to-peer video works end-to-end
    - Console API documented for manual testing
  </done>
</task>

---

## Verification

```bash
# Server-side verification
cargo fmt --all -- --check
cargo clippy --workspace --all-features -- -D warnings
cargo test --workspace

# Manual browser verification
cargo run -p canvas-server &

# Test 1: Local camera renders
# Open http://localhost:9473
# Console: await videoManager.addLocalCamera()
# Add video element via UI or console
# Verify local camera shows on canvas

# Test 2: Peer video renders
# Open second browser window
# Note peer ID from both windows
# Call from one to the other
# Verify video streams in both directions

# Test 3: Cleanup works
# End call
# Verify video elements removed
# Verify no memory leaks (streams stopped)
```

## Risks

- **Medium**: Browser codec differences may cause stream compatibility issues
- **Medium**: Performance with multiple video streams (may need resolution limits)
- **Low**: Canvas2D vs WebGL performance gap (WebGL texture path deferred)

## Notes

- WASM/wgpu video texture rendering deferred (requires SharedArrayBuffer)
- Audio rendering deferred to Phase 3.3
- Resolution/bitrate controls deferred to Phase 3.3
- Screen sharing deferred to future phase

## Exit Criteria

- [x] CanvasRenderer renders video frames at smooth framerate
- [x] Video elements auto-created when peers connect
- [x] Video elements auto-removed when peers disconnect
- [x] Local camera toggle works from UI
- [x] Debug overlay shows rendering stats
- [x] End-to-end peer video works in browser test
- [x] ROADMAP.md updated with Phase 3.2 progress
