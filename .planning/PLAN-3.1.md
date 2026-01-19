# Phase 3.1: WebRTC Signaling Bridge

> Goal: Enable peer-to-peer video streams via Communitas signaling relay.

## Prerequisites

- [x] Phase 2.4 complete (Communitas CLI)
- [x] Video element defined in scene graph (ElementKind::Video)
- [x] VideoManager exists in web/video.js
- [x] WebSocket protocol supports element mutations
- [x] Communitas bridge exists for control messages

## Overview

This phase adds WebRTC signaling to enable peer-to-peer video streams:

1. **Signaling Message Types** - Define SDP/ICE candidate message formats
2. **Browser Signaling Handler** - RTCPeerConnection management in JavaScript
3. **Server Relay** - WebSocket relay for signaling messages via Communitas
4. **Stream Integration** - Connect established streams to VideoManager

Architecture:
```
Peer A (Browser)                    Peer B (Browser)
     │                                    │
     │ WebSocket                          │ WebSocket
     ▼                                    ▼
 canvas-server ◄──── Communitas ────► canvas-server
     │                (relay)             │
     │ signaling                          │ signaling
     ▼                                    ▼
RTCPeerConnection ◄──── P2P Media ────► RTCPeerConnection
     │                                    │
     ▼                                    ▼
VideoManager                        VideoManager
     │                                    │
     ▼                                    ▼
Canvas Element                      Canvas Element
```

---

<task type="auto" priority="p1">
  <n>Define signaling message types in sync protocol</n>
  <files>
    canvas-server/src/sync.rs,
    web/index.html
  </files>
  <action>
    Add WebRTC signaling message types to the WebSocket protocol:

    1. Add signaling variants to ClientMessage in sync.rs:
       ```rust
       /// Start a call to a peer
       StartCall {
           target_peer_id: String,
           session_id: String,
       },
       /// SDP offer from caller
       Offer {
           target_peer_id: String,
           sdp: String,
       },
       /// SDP answer from callee
       Answer {
           target_peer_id: String,
           sdp: String,
       },
       /// ICE candidate exchange
       IceCandidate {
           target_peer_id: String,
           candidate: String,
           sdp_mid: Option<String>,
           sdp_m_line_index: Option<u16>,
       },
       /// End a call
       EndCall {
           target_peer_id: String,
       },
       ```

    2. Add signaling variants to ServerMessage:
       ```rust
       /// Incoming call notification
       IncomingCall {
           from_peer_id: String,
           session_id: String,
       },
       /// Relay SDP offer
       RelayOffer {
           from_peer_id: String,
           sdp: String,
       },
       /// Relay SDP answer
       RelayAnswer {
           from_peer_id: String,
           sdp: String,
       },
       /// Relay ICE candidate
       RelayIceCandidate {
           from_peer_id: String,
           candidate: String,
           sdp_mid: Option<String>,
           sdp_m_line_index: Option<u16>,
       },
       /// Call ended notification
       CallEnded {
           from_peer_id: String,
           reason: String,
       },
       ```

    3. Update JavaScript message handling in web/index.html:
       - Add cases for RelayOffer, RelayAnswer, RelayIceCandidate, IncomingCall, CallEnded
       - Forward to new signaling.js module (created in next task)

    4. Update sendMutation helper or add new sendSignaling helper for signaling messages
  </action>
  <verify>
    cargo fmt --all -- --check
    cargo clippy -p canvas-server --all-features -- -D warnings
    cargo test -p canvas-server
  </verify>
  <done>
    - ClientMessage and ServerMessage include signaling variants
    - JSON serialization works for all new message types
    - JavaScript can send/receive signaling messages
    - All tests pass
  </done>
</task>

---

<task type="auto" priority="p1">
  <n>Create RTCPeerConnection handler in JavaScript</n>
  <files>
    web/signaling.js,
    web/index.html
  </files>
  <action>
    Create signaling.js to manage RTCPeerConnection lifecycle:

    1. Create web/signaling.js with SignalingManager class:
       ```javascript
       class SignalingManager {
           constructor(websocket, videoManager) {
               this.ws = websocket;
               this.videoManager = videoManager;
               this.peerConnections = new Map(); // peerId -> RTCPeerConnection
               this.localStream = null;
               this.peerId = generatePeerId(); // unique ID for this client
           }

           // Get or create RTCPeerConnection for a peer
           getOrCreateConnection(peerId) { ... }

           // Start a call (caller creates offer)
           async startCall(targetPeerId) { ... }

           // Handle incoming call (callee creates answer)
           async handleIncomingCall(fromPeerId) { ... }

           // Handle SDP offer
           async handleOffer(fromPeerId, sdp) { ... }

           // Handle SDP answer
           async handleAnswer(fromPeerId, sdp) { ... }

           // Handle ICE candidate
           async handleIceCandidate(fromPeerId, candidate) { ... }

           // End a call
           endCall(peerId) { ... }

           // Cleanup all connections
           cleanup() { ... }
       }
       ```

    2. RTCPeerConnection configuration:
       - Use public STUN servers (Google, Twilio fallback)
       - Optional TURN configuration via environment
       - Set up onicecandidate handler to relay candidates
       - Set up ontrack handler to add streams to VideoManager

    3. Integrate with VideoManager:
       - On track received: videoManager.addPeerStream(stream, peerId)
       - On track ended: videoManager.removeStream(peerId)

    4. Update web/index.html:
       - Import signaling.js
       - Create SignalingManager instance after WebSocket connects
       - Wire up message handlers
       - Add UI controls for starting/ending calls (optional, can use console API)
  </action>
  <verify>
    # Manual browser testing required
    # Open two browser windows, start call from console:
    # Window 1: signalingManager.startCall('peer-2')
    # Window 2: Should receive IncomingCall, video should appear
  </verify>
  <done>
    - SignalingManager class created with full RTCPeerConnection lifecycle
    - STUN servers configured
    - Peer streams flow to VideoManager
    - Call start/end works between two browser windows
  </done>
</task>

---

<task type="auto" priority="p1">
  <n>Implement signaling relay in canvas-server</n>
  <files>
    canvas-server/src/sync.rs,
    canvas-server/src/routes.rs
  </files>
  <action>
    Add server-side relay for signaling messages:

    1. Add peer_id tracking to WebSocket connections in sync.rs:
       - Store peer_id in connection state
       - Track which peers are in which sessions
       - Create peer_connections: HashMap<String, Arc<PeerState>>

    2. Handle signaling messages in handle_client_message:
       ```rust
       ClientMessage::Offer { target_peer_id, sdp } => {
           // Find target peer's WebSocket
           // Send RelayOffer { from_peer_id: sender.peer_id, sdp }
       }
       ClientMessage::Answer { target_peer_id, sdp } => {
           // Send RelayAnswer to target
       }
       ClientMessage::IceCandidate { target_peer_id, candidate, .. } => {
           // Send RelayIceCandidate to target
       }
       ClientMessage::StartCall { target_peer_id, session_id } => {
           // Verify target is in same session
           // Send IncomingCall to target
       }
       ClientMessage::EndCall { target_peer_id } => {
           // Send CallEnded to target
       }
       ```

    3. Add peer ID assignment on subscribe:
       - Generate unique peer ID if not provided
       - Store in connection state
       - Broadcast peer join/leave to session

    4. Add peer list query (optional):
       - ClientMessage::ListPeers { session_id }
       - ServerMessage::PeerList { peers: Vec<PeerInfo> }
  </action>
  <verify>
    cargo fmt --all -- --check
    cargo clippy -p canvas-server --all-features -- -D warnings
    cargo test -p canvas-server
  </verify>
  <done>
    - Signaling messages relay correctly between peers
    - Peer IDs assigned and tracked per session
    - Peers can only signal to others in same session
    - No signaling messages cross session boundaries
  </done>
</task>

---

## Verification

```bash
# Full verification
cargo fmt --all -- --check
cargo clippy --workspace --all-features -- -D warnings
cargo test --workspace

# Manual WebRTC test
cargo run -p canvas-server &
# Open http://localhost:9473 in two browser windows
# In browser console of Window 1:
#   signalingManager.startCall('peer-xyz')
# In browser console of Window 2:
#   // Should see incoming call, accept, video appears
```

## Risks

- **Medium**: STUN/TURN configuration may need customization for production
- **Medium**: ICE candidate trickle timing - may need buffering
- **Low**: Peer ID collisions - mitigated by UUID generation

## Notes

- Communitas integration for external signaling deferred (current implementation is local server only)
- TURN server configuration is environment-specific
- Screen sharing and audio deferred to Phase 3.2
- Bitrate/latency monitoring deferred to Phase 3.3

## Exit Criteria

- [x] Signaling message types defined in sync protocol
- [x] RTCPeerConnection lifecycle managed in browser
- [x] Server relays signaling between peers in same session
- [x] Video streams appear as canvas elements after call connects
- [x] Call teardown cleans up connections and streams
- [x] All clippy warnings resolved
- [x] ROADMAP.md updated with Phase 3.1 progress
