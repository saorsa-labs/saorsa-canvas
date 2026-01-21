# Phase 3.3: Media Schema

> Goal: Add canonical media configuration (bitrate, resolution, latency) to SceneDocument and enable dynamic quality control.

## Prerequisites

- [x] Phase 3.2 complete (Video Elements rendering)
- [x] VideoManager exists with getVideoFrame() API
- [x] SignalingManager manages RTCPeerConnection lifecycle
- [x] Video elements render peer streams at 60fps

## Overview

This phase adds media schema support for quality configuration and monitoring:

1. **Media Schema Types** - Add bitrate, resolution, quality to element types
2. **Quality Control API** - Dynamic bitrate/resolution adjustment
3. **Stats Monitoring** - Track latency, packet loss, FPS metrics
4. **Audio Support** - Optional audio track configuration

Architecture:
```
ElementKind::Video                    RTCPeerConnection
     │                                      │
     │ MediaConfig                          │ getStats()
     ▼                                      ▼
 { bitrate, resolution,           MediaStats { rtt, jitter,
   quality_preset }               packetLoss, fps, bitrate }
     │                                      │
     ▼                                      ▼
 setEncoderParams()               UI Stats Display
     │                                      │
     ▼                                      ▼
 RTCRtpSender.setParameters()     Debug Overlay Updates
```

---

<task type="auto" priority="p1">
  <n>Add media schema types to canvas-core</n>
  <files>
    canvas-core/src/element.rs,
    canvas-core/src/lib.rs
  </files>
  <action>
    Extend ElementKind::Video with media configuration:

    1. Add MediaConfig struct to element.rs:
       ```rust
       /// Configuration for video stream quality.
       #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
       pub struct MediaConfig {
           /// Target bitrate in kbps (e.g., 1500 for 720p)
           pub bitrate_kbps: Option<u32>,
           /// Max resolution constraint
           pub max_resolution: Option<Resolution>,
           /// Quality preset (overrides specific settings)
           pub quality_preset: QualityPreset,
           /// Target framerate (default 30)
           pub target_fps: Option<u8>,
           /// Enable audio track
           pub audio_enabled: bool,
       }

       impl Default for MediaConfig {
           fn default() -> Self {
               Self {
                   bitrate_kbps: None,
                   max_resolution: None,
                   quality_preset: QualityPreset::Auto,
                   target_fps: None,
                   audio_enabled: false,
               }
           }
       }
       ```

    2. Add Resolution enum:
       ```rust
       /// Video resolution presets.
       #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
       #[serde(rename_all = "lowercase")]
       pub enum Resolution {
           /// 426x240 (very low bandwidth)
           R240p,
           /// 640x360
           R360p,
           /// 854x480
           R480p,
           /// 1280x720
           R720p,
           /// 1920x1080
           R1080p,
       }

       impl Resolution {
           /// Get width x height tuple
           pub fn dimensions(&self) -> (u32, u32) {
               match self {
                   Self::R240p => (426, 240),
                   Self::R360p => (640, 360),
                   Self::R480p => (854, 480),
                   Self::R720p => (1280, 720),
                   Self::R1080p => (1920, 1080),
               }
           }

           /// Suggested bitrate for this resolution (kbps)
           pub fn suggested_bitrate(&self) -> u32 {
               match self {
                   Self::R240p => 400,
                   Self::R360p => 800,
                   Self::R480p => 1200,
                   Self::R720p => 2500,
                   Self::R1080p => 5000,
               }
           }
       }
       ```

    3. Add QualityPreset enum:
       ```rust
       /// Quality presets for automatic configuration.
       #[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
       #[serde(rename_all = "lowercase")]
       pub enum QualityPreset {
           /// Automatic adaptation based on network
           #[default]
           Auto,
           /// Low bandwidth mode (240p-360p)
           Low,
           /// Medium quality (480p)
           Medium,
           /// High quality (720p)
           High,
           /// Maximum quality (1080p)
           Ultra,
       }

       impl QualityPreset {
           /// Get resolution for this preset
           pub fn resolution(&self) -> Resolution {
               match self {
                   Self::Auto => Resolution::R720p,
                   Self::Low => Resolution::R360p,
                   Self::Medium => Resolution::R480p,
                   Self::High => Resolution::R720p,
                   Self::Ultra => Resolution::R1080p,
               }
           }
       }
       ```

    4. Add MediaStats struct for monitoring:
       ```rust
       /// Real-time media statistics.
       #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
       pub struct MediaStats {
           /// Round-trip time in milliseconds
           pub rtt_ms: Option<f64>,
           /// Jitter in milliseconds
           pub jitter_ms: Option<f64>,
           /// Packet loss percentage (0.0 - 100.0)
           pub packet_loss_percent: Option<f64>,
           /// Current framerate
           pub fps: Option<f64>,
           /// Current bitrate in kbps
           pub bitrate_kbps: Option<f64>,
           /// Timestamp of last update (unix millis)
           pub timestamp: u64,
       }
       ```

    5. Update ElementKind::Video to include media_config:
       ```rust
       Video {
           stream_id: String,
           is_live: bool,
           mirror: bool,
           crop: Option<CropRect>,
           /// Media quality configuration
           media_config: Option<MediaConfig>,
       }
       ```

    6. Export new types from lib.rs
  </action>
  <verify>
    cargo fmt --all -- --check
    cargo clippy -p canvas-core --all-features -- -D warnings
    cargo test -p canvas-core
  </verify>
  <done>
    - MediaConfig struct with bitrate, resolution, fps, audio settings
    - Resolution enum with common presets and helper methods
    - QualityPreset enum for automatic configuration
    - MediaStats struct for real-time monitoring
    - ElementKind::Video updated with optional media_config field
    - All types serializable for JSON transport
    - All tests pass
  </done>
</task>

---

<task type="auto" priority="p1">
  <n>Implement quality control in SignalingManager</n>
  <files>
    web/signaling.js,
    web/index.html
  </files>
  <action>
    Add bitrate/resolution control via RTCRtpSender:

    1. Add setQuality method to SignalingManager:
       ```javascript
       /**
        * Set video quality for a peer connection.
        * @param {string} peerId - Target peer ID
        * @param {Object} config - Quality configuration
        * @param {number} [config.maxBitrate] - Max bitrate in kbps
        * @param {number} [config.maxWidth] - Max width
        * @param {number} [config.maxHeight] - Max height
        * @param {number} [config.maxFramerate] - Max FPS
        */
       async setQuality(peerId, config) {
           const pc = this.peerConnections.get(peerId);
           if (!pc) return;

           const senders = pc.getSenders();
           const videoSender = senders.find(s => s.track?.kind === 'video');
           if (!videoSender) return;

           const params = videoSender.getParameters();
           if (!params.encodings || params.encodings.length === 0) {
               params.encodings = [{}];
           }

           const encoding = params.encodings[0];
           if (config.maxBitrate) {
               encoding.maxBitrate = config.maxBitrate * 1000; // kbps to bps
           }
           if (config.maxWidth && config.maxHeight) {
               encoding.scaleResolutionDownBy = Math.max(
                   1,
                   1920 / config.maxWidth
               );
           }
           if (config.maxFramerate) {
               encoding.maxFramerate = config.maxFramerate;
           }

           await videoSender.setParameters(params);
           console.log(`[SignalingManager] Quality set for ${peerId}:`, config);
       }
       ```

    2. Add quality presets:
       ```javascript
       const QUALITY_PRESETS = {
           low: { maxBitrate: 400, maxWidth: 640, maxHeight: 360, maxFramerate: 15 },
           medium: { maxBitrate: 1200, maxWidth: 854, maxHeight: 480, maxFramerate: 24 },
           high: { maxBitrate: 2500, maxWidth: 1280, maxHeight: 720, maxFramerate: 30 },
           ultra: { maxBitrate: 5000, maxWidth: 1920, maxHeight: 1080, maxFramerate: 30 }
       };

       /**
        * Set quality preset for a peer.
        * @param {string} peerId - Target peer ID
        * @param {string} preset - 'low', 'medium', 'high', or 'ultra'
        */
       async setQualityPreset(peerId, preset) {
           const config = QUALITY_PRESETS[preset];
           if (config) {
               await this.setQuality(peerId, config);
           }
       }
       ```

    3. Add adaptive quality based on stats:
       ```javascript
       /**
        * Enable adaptive quality for a peer connection.
        * @param {string} peerId - Target peer ID
        */
       enableAdaptiveQuality(peerId) {
           // Check stats every 5 seconds
           const intervalId = setInterval(async () => {
               const stats = await this.getStats(peerId);
               if (!stats) {
                   clearInterval(intervalId);
                   return;
               }

               // Downgrade if packet loss > 5%
               if (stats.packetLossPercent > 5) {
                   console.log(`[SignalingManager] High packet loss (${stats.packetLossPercent}%), reducing quality`);
                   await this.setQualityPreset(peerId, 'low');
               } else if (stats.packetLossPercent < 1 && stats.rttMs < 100) {
                   // Upgrade if conditions are good
                   await this.setQualityPreset(peerId, 'high');
               }
           }, 5000);

           this._adaptiveIntervals = this._adaptiveIntervals || new Map();
           this._adaptiveIntervals.set(peerId, intervalId);
       }
       ```

    4. Update web/index.html with quality controls:
       - Add quality selector dropdown in toolbar
       - Wire to signalingManager.setQualityPreset()
       - Show current quality in debug overlay

    5. Clean up adaptive intervals on connection close
  </action>
  <verify>
    # Manual browser test:
    # 1. Start call between two windows
    # 2. Use console: signalingManager.setQualityPreset('peer-xxx', 'low')
    # 3. Observe quality change in debug overlay
    # 4. Test adaptive quality via throttling
  </verify>
  <done>
    - setQuality() method for fine-grained control
    - setQualityPreset() for easy presets
    - enableAdaptiveQuality() for automatic adaptation
    - Quality dropdown in UI
    - Adaptive intervals cleaned up on disconnect
  </done>
</task>

---

<task type="auto" priority="p1">
  <n>Add stats monitoring and debug display</n>
  <files>
    web/signaling.js,
    web/canvas-renderer.js,
    web/index.html
  </files>
  <action>
    Implement WebRTC stats collection and display:

    1. Add getStats method to SignalingManager:
       ```javascript
       /**
        * Get connection stats for a peer.
        * @param {string} peerId - Target peer ID
        * @returns {Promise<Object|null>} Stats object or null
        */
       async getStats(peerId) {
           const pc = this.peerConnections.get(peerId);
           if (!pc) return null;

           const stats = await pc.getStats();
           const result = {
               timestamp: Date.now(),
               rttMs: null,
               jitterMs: null,
               packetLossPercent: null,
               fps: null,
               bitrateKbps: null,
               bytesReceived: 0,
               packetsLost: 0,
               packetsReceived: 0
           };

           stats.forEach(report => {
               if (report.type === 'candidate-pair' && report.state === 'succeeded') {
                   result.rttMs = report.currentRoundTripTime * 1000;
               }
               if (report.type === 'inbound-rtp' && report.kind === 'video') {
                   result.fps = report.framesPerSecond;
                   result.jitterMs = report.jitter * 1000;
                   result.bytesReceived = report.bytesReceived;
                   result.packetsLost = report.packetsLost;
                   result.packetsReceived = report.packetsReceived;
               }
               if (report.type === 'outbound-rtp' && report.kind === 'video') {
                   // For outbound, calculate bitrate from bytes sent
                   result.bytesSent = report.bytesSent;
               }
           });

           // Calculate packet loss percentage
           if (result.packetsReceived > 0) {
               result.packetLossPercent =
                   (result.packetsLost / (result.packetsReceived + result.packetsLost)) * 100;
           }

           return result;
       }
       ```

    2. Add periodic stats collection:
       ```javascript
       /**
        * Start collecting stats for all connections.
        * @param {function} callback - Called with stats for each peer
        * @param {number} [intervalMs=1000] - Collection interval
        * @returns {number} Interval ID
        */
       startStatsCollection(callback, intervalMs = 1000) {
           return setInterval(async () => {
               const allStats = new Map();
               for (const peerId of this.peerConnections.keys()) {
                   const stats = await this.getStats(peerId);
                   if (stats) {
                       allStats.set(peerId, stats);
                   }
               }
               callback(allStats);
           }, intervalMs);
       }
       ```

    3. Update CanvasRenderer debug overlay to show stats:
       ```javascript
       // Add mediaStats property
       setMediaStats(stats) {
           this.mediaStats = stats;
       }

       renderDebugOverlay() {
           // ... existing debug code ...

           // Add media stats section
           y += lineHeight;
           this.ctx.fillText('--- Media Stats ---', padding, y);

           if (this.mediaStats) {
               for (const [peerId, stats] of this.mediaStats) {
                   y += lineHeight;
                   const shortId = peerId.substring(0, 12);
                   const rtt = stats.rttMs?.toFixed(0) || '?';
                   const loss = stats.packetLossPercent?.toFixed(1) || '?';
                   const fps = stats.fps?.toFixed(0) || '?';
                   this.ctx.fillText(
                       `${shortId}: ${rtt}ms RTT, ${loss}% loss, ${fps}fps`,
                       padding, y
                   );
               }
           }
       }
       ```

    4. Wire stats collection in index.html:
       ```javascript
       // In ws.onopen handler, after setupVideoStreamListener():
       signalingManager.startStatsCollection((stats) => {
           if (canvasRenderer) {
               canvasRenderer.setMediaStats(stats);
           }
       });
       ```

    5. Add stats panel to UI (optional):
       - Floating panel showing per-peer stats
       - Toggle with 'S' key
       - Color-coded quality indicators (green/yellow/red)
  </action>
  <verify>
    # Manual browser test:
    # 1. Open two windows, establish call
    # 2. Press 'D' for debug overlay
    # 3. Stats should show RTT, packet loss, FPS
    # 4. Throttle network in DevTools
    # 5. Observe stats change in debug overlay
  </verify>
  <done>
    - getStats() returns RTT, jitter, packet loss, FPS
    - startStatsCollection() for periodic updates
    - Debug overlay shows per-peer media stats
    - Stats update in real-time
    - Optional floating stats panel
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

# Test 1: Quality presets work
# Open http://localhost:9473 in two windows
# Start call between them
# Console: signalingManager.setQualityPreset('peer-xxx', 'low')
# Verify quality changes (visible in debug overlay)

# Test 2: Stats collection works
# With call active, press 'D' for debug overlay
# Verify RTT, packet loss, FPS shown
# Throttle network in DevTools
# Verify stats reflect degraded conditions

# Test 3: Adaptive quality works
# Console: signalingManager.enableAdaptiveQuality('peer-xxx')
# Throttle network
# Observe automatic quality reduction
# Remove throttling
# Observe quality restoration
```

## Risks

- **Medium**: Browser codec differences may affect encoding params
- **Medium**: getStats() API varies slightly between browsers
- **Low**: Adaptive quality may oscillate without hysteresis

## Notes

- Audio track support defined in schema but not wired (future phase)
- Simulcast (multiple quality layers) deferred for complexity
- TURN server configuration remains environment-specific
- Screen sharing deferred to future phase

## Exit Criteria

- [x] MediaConfig, Resolution, QualityPreset types in canvas-core
- [x] MediaStats type for monitoring
- [x] ElementKind::Video has optional media_config field
- [x] setQuality() and setQualityPreset() work in SignalingManager
- [x] getStats() returns RTT, packet loss, FPS
- [x] Debug overlay shows media stats
- [x] Adaptive quality reduces bitrate on packet loss
- [x] All clippy warnings resolved
- [x] ROADMAP.md updated with Phase 3.3 progress
