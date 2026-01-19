/**
 * WebRTC Signaling Manager for Saorsa Canvas
 *
 * Manages RTCPeerConnection lifecycle for peer-to-peer video streams.
 * Uses WebSocket relay through canvas-server for signaling.
 */

/**
 * ICE server configuration for WebRTC.
 * Uses public STUN servers for NAT traversal.
 */
const ICE_SERVERS = [
    { urls: 'stun:stun.l.google.com:19302' },
    { urls: 'stun:stun1.l.google.com:19302' },
    { urls: 'stun:stun2.l.google.com:19302' },
    { urls: 'stun:stun.cloudflare.com:3478' }
];

/**
 * Quality presets for video encoding.
 */
const QUALITY_PRESETS = {
    low: { maxBitrate: 400, maxWidth: 640, maxHeight: 360, maxFramerate: 15 },
    medium: { maxBitrate: 1200, maxWidth: 854, maxHeight: 480, maxFramerate: 24 },
    high: { maxBitrate: 2500, maxWidth: 1280, maxHeight: 720, maxFramerate: 30 },
    ultra: { maxBitrate: 5000, maxWidth: 1920, maxHeight: 1080, maxFramerate: 30 }
};

/**
 * Generate a unique peer ID.
 * @returns {string} Unique peer ID
 */
function generatePeerId() {
    const timestamp = Date.now().toString(36);
    const random = Math.random().toString(36).substring(2, 8);
    return `peer-${timestamp}-${random}`;
}

/**
 * Manages WebRTC signaling and peer connections.
 */
export class SignalingManager {
    /**
     * Create a new SignalingManager.
     * @param {VideoManager} videoManager - Video manager for handling streams
     */
    constructor(videoManager) {
        /** @type {VideoManager} */
        this.videoManager = videoManager;

        /** @type {Map<string, RTCPeerConnection>} Peer connections by peer ID */
        this.peerConnections = new Map();

        /** @type {Map<string, RTCIceCandidate[]>} Pending ICE candidates */
        this.pendingCandidates = new Map();

        /** @type {MediaStream|null} Local media stream */
        this.localStream = null;

        /** @type {string} This client's peer ID */
        this.peerId = generatePeerId();

        /** @type {Set<function>} Callbacks for call state changes */
        this.onCallStateChangeCallbacks = new Set();

        /** @type {boolean} Whether this manager has been cleaned up */
        this.disposed = false;

        console.log(`[SignalingManager] Initialized with peer ID: ${this.peerId}`);
    }

    /**
     * Handle peer ID assignment from server.
     * @param {string} peerId - Assigned peer ID
     */
    handlePeerAssigned(peerId) {
        console.log(`[SignalingManager] Server assigned peer ID: ${peerId}`);
        this.peerId = peerId;
    }

    /**
     * Get this client's peer ID.
     * @returns {string} Peer ID
     */
    getPeerId() {
        return this.peerId;
    }

    /**
     * Get or create an RTCPeerConnection for a peer.
     * @param {string} peerId - Remote peer ID
     * @returns {RTCPeerConnection} Peer connection
     */
    getOrCreateConnection(peerId) {
        if (this.peerConnections.has(peerId)) {
            return this.peerConnections.get(peerId);
        }

        console.log(`[SignalingManager] Creating connection to peer: ${peerId}`);

        const pc = new RTCPeerConnection({
            iceServers: ICE_SERVERS,
            iceCandidatePoolSize: 10
        });

        // Handle ICE candidates
        pc.onicecandidate = (event) => {
            if (event.candidate) {
                console.debug(`[SignalingManager] Sending ICE candidate to ${peerId}`);
                this._sendSignaling('ice_candidate', {
                    target_peer_id: peerId,
                    candidate: event.candidate.candidate,
                    sdp_mid: event.candidate.sdpMid,
                    sdp_m_line_index: event.candidate.sdpMLineIndex
                });
            }
        };

        // Handle ICE connection state changes
        pc.oniceconnectionstatechange = () => {
            console.log(`[SignalingManager] ICE connection state for ${peerId}: ${pc.iceConnectionState}`);
            if (pc.iceConnectionState === 'failed' || pc.iceConnectionState === 'disconnected') {
                this._notifyCallStateChange(peerId, 'disconnected');
            } else if (pc.iceConnectionState === 'connected') {
                this._notifyCallStateChange(peerId, 'connected');
            }
        };

        // Handle connection state changes
        pc.onconnectionstatechange = () => {
            console.log(`[SignalingManager] Connection state for ${peerId}: ${pc.connectionState}`);
            if (pc.connectionState === 'failed') {
                console.error(`[SignalingManager] Connection to ${peerId} failed`);
                this.endCall(peerId);
            }
        };

        // Handle incoming tracks
        pc.ontrack = (event) => {
            console.log(`[SignalingManager] Received track from ${peerId}:`, event.track.kind);
            if (event.streams && event.streams[0]) {
                this._handleRemoteStream(peerId, event.streams[0]);
            }
        };

        this.peerConnections.set(peerId, pc);
        this.pendingCandidates.set(peerId, []);

        return pc;
    }

    /**
     * Start a call to a peer (caller side).
     * @param {string} targetPeerId - Target peer ID to call
     * @returns {Promise<void>}
     */
    async startCall(targetPeerId) {
        console.log(`[SignalingManager] Starting call to ${targetPeerId}`);

        // Ensure we have local media
        await this._ensureLocalStream();

        const pc = this.getOrCreateConnection(targetPeerId);

        // Add local tracks to the connection
        if (this.localStream) {
            this.localStream.getTracks().forEach(track => {
                pc.addTrack(track, this.localStream);
            });
        }

        // Create and send offer
        try {
            const offer = await pc.createOffer({
                offerToReceiveAudio: false,
                offerToReceiveVideo: true
            });
            await pc.setLocalDescription(offer);

            this._sendSignaling('offer', {
                target_peer_id: targetPeerId,
                sdp: offer.sdp
            });

            this._notifyCallStateChange(targetPeerId, 'calling');
            console.log(`[SignalingManager] Sent offer to ${targetPeerId}`);
        } catch (error) {
            console.error(`[SignalingManager] Failed to create offer:`, error);
            this.endCall(targetPeerId);
            throw error;
        }
    }

    /**
     * Handle an incoming call notification.
     * @param {string} fromPeerId - Caller's peer ID
     * @param {string} sessionId - Session ID for the call
     */
    async handleIncomingCall(fromPeerId, sessionId) {
        console.log(`[SignalingManager] Incoming call from ${fromPeerId} in session ${sessionId}`);

        // Auto-accept for now (could add UI confirmation later)
        this._notifyCallStateChange(fromPeerId, 'incoming');

        // Prepare connection (actual answer happens when we receive the offer)
        this.getOrCreateConnection(fromPeerId);
    }

    /**
     * Handle received SDP offer.
     * @param {string} fromPeerId - Peer ID of the caller
     * @param {string} sdp - SDP offer string
     */
    async handleOffer(fromPeerId, sdp) {
        console.log(`[SignalingManager] Received offer from ${fromPeerId}`);

        try {
            // Ensure we have local media
            await this._ensureLocalStream();

            const pc = this.getOrCreateConnection(fromPeerId);

            // Set remote description
            await pc.setRemoteDescription({
                type: 'offer',
                sdp: sdp
            });

            // Add local tracks
            if (this.localStream) {
                this.localStream.getTracks().forEach(track => {
                    pc.addTrack(track, this.localStream);
                });
            }

            // Create and send answer
            const answer = await pc.createAnswer();
            await pc.setLocalDescription(answer);

            this._sendSignaling('answer', {
                target_peer_id: fromPeerId,
                sdp: answer.sdp
            });

            // Process any pending ICE candidates
            await this._processPendingCandidates(fromPeerId);

            this._notifyCallStateChange(fromPeerId, 'answering');
            console.log(`[SignalingManager] Sent answer to ${fromPeerId}`);
        } catch (error) {
            console.error(`[SignalingManager] Failed to handle offer:`, error);
            this.endCall(fromPeerId);
        }
    }

    /**
     * Handle received SDP answer.
     * @param {string} fromPeerId - Peer ID of the callee
     * @param {string} sdp - SDP answer string
     */
    async handleAnswer(fromPeerId, sdp) {
        console.log(`[SignalingManager] Received answer from ${fromPeerId}`);

        const pc = this.peerConnections.get(fromPeerId);
        if (!pc) {
            console.error(`[SignalingManager] No connection for ${fromPeerId}`);
            return;
        }

        try {
            await pc.setRemoteDescription({
                type: 'answer',
                sdp: sdp
            });

            // Process any pending ICE candidates
            await this._processPendingCandidates(fromPeerId);

            console.log(`[SignalingManager] Answer processed for ${fromPeerId}`);
        } catch (error) {
            console.error(`[SignalingManager] Failed to handle answer:`, error);
        }
    }

    /**
     * Handle received ICE candidate.
     * @param {string} fromPeerId - Peer ID sending the candidate
     * @param {Object} candidateData - ICE candidate data
     * @param {string} candidateData.candidate - Candidate string
     * @param {string} [candidateData.sdpMid] - SDP media ID
     * @param {number} [candidateData.sdpMLineIndex] - SDP media line index
     */
    async handleIceCandidate(fromPeerId, candidateData) {
        const pc = this.peerConnections.get(fromPeerId);

        const candidate = new RTCIceCandidate({
            candidate: candidateData.candidate,
            sdpMid: candidateData.sdpMid,
            sdpMLineIndex: candidateData.sdpMLineIndex
        });

        if (pc && pc.remoteDescription) {
            try {
                await pc.addIceCandidate(candidate);
                console.debug(`[SignalingManager] Added ICE candidate from ${fromPeerId}`);
            } catch (error) {
                console.error(`[SignalingManager] Failed to add ICE candidate:`, error);
            }
        } else {
            // Queue candidate for later
            console.debug(`[SignalingManager] Queuing ICE candidate from ${fromPeerId}`);
            const pending = this.pendingCandidates.get(fromPeerId) || [];
            pending.push(candidate);
            this.pendingCandidates.set(fromPeerId, pending);
        }
    }

    /**
     * Handle call ended notification.
     * @param {string} fromPeerId - Peer ID that ended the call
     * @param {string} reason - Reason for ending
     */
    handleCallEnded(fromPeerId, reason) {
        console.log(`[SignalingManager] Call ended by ${fromPeerId}: ${reason}`);
        this._cleanupConnection(fromPeerId);
        this._notifyCallStateChange(fromPeerId, 'ended');
    }

    /**
     * End a call with a peer.
     * @param {string} peerId - Peer ID to end call with
     */
    endCall(peerId) {
        console.log(`[SignalingManager] Ending call with ${peerId}`);

        this._sendSignaling('end_call', {
            target_peer_id: peerId
        });

        this._cleanupConnection(peerId);
        this._notifyCallStateChange(peerId, 'ended');
    }

    /**
     * Register a callback for call state changes.
     * @param {function(string, string)} callback - Callback(peerId, state)
     */
    onCallStateChange(callback) {
        this.onCallStateChangeCallbacks.add(callback);
    }

    /**
     * Unregister a call state change callback.
     * @param {function} callback - Callback to remove
     */
    offCallStateChange(callback) {
        this.onCallStateChangeCallbacks.delete(callback);
    }

    /**
     * Get list of active peer IDs.
     * @returns {string[]} Active peer IDs
     */
    getActivePeers() {
        return Array.from(this.peerConnections.keys());
    }

    // ============================================================
    // Quality Control Methods
    // ============================================================

    /**
     * Set video quality for a peer connection.
     * @param {string} peerId - Target peer ID
     * @param {Object} config - Quality configuration
     * @param {number} [config.maxBitrate] - Max bitrate in kbps
     * @param {number} [config.maxWidth] - Max width
     * @param {number} [config.maxHeight] - Max height
     * @param {number} [config.maxFramerate] - Max FPS
     * @returns {Promise<boolean>} Success status
     */
    async setQuality(peerId, config) {
        const pc = this.peerConnections.get(peerId);
        if (!pc) {
            console.warn(`[SignalingManager] No connection for ${peerId}`);
            return false;
        }

        const senders = pc.getSenders();
        const videoSender = senders.find(s => s.track?.kind === 'video');
        if (!videoSender) {
            console.warn(`[SignalingManager] No video sender for ${peerId}`);
            return false;
        }

        try {
            const params = videoSender.getParameters();
            if (!params.encodings || params.encodings.length === 0) {
                params.encodings = [{}];
            }

            const encoding = params.encodings[0];

            if (config.maxBitrate) {
                encoding.maxBitrate = config.maxBitrate * 1000; // kbps to bps
            }
            if (config.maxWidth && config.maxHeight) {
                // Scale down from 1080p base resolution
                encoding.scaleResolutionDownBy = Math.max(1, 1920 / config.maxWidth);
            }
            if (config.maxFramerate) {
                encoding.maxFramerate = config.maxFramerate;
            }

            await videoSender.setParameters(params);
            console.log(`[SignalingManager] Quality set for ${peerId}:`, config);
            return true;
        } catch (error) {
            console.error(`[SignalingManager] Failed to set quality for ${peerId}:`, error);
            return false;
        }
    }

    /**
     * Set quality preset for a peer.
     * @param {string} peerId - Target peer ID
     * @param {string} preset - 'low', 'medium', 'high', or 'ultra'
     * @returns {Promise<boolean>} Success status
     */
    async setQualityPreset(peerId, preset) {
        const config = QUALITY_PRESETS[preset];
        if (!config) {
            console.warn(`[SignalingManager] Unknown quality preset: ${preset}`);
            return false;
        }
        return this.setQuality(peerId, config);
    }

    /**
     * Get available quality presets.
     * @returns {Object} Quality presets
     */
    getQualityPresets() {
        return { ...QUALITY_PRESETS };
    }

    /**
     * Enable adaptive quality for a peer connection.
     * Automatically adjusts quality based on network conditions.
     * @param {string} peerId - Target peer ID
     */
    enableAdaptiveQuality(peerId) {
        // Initialize adaptive intervals map if not exists
        if (!this._adaptiveIntervals) {
            this._adaptiveIntervals = new Map();
        }

        // Clear existing interval if any
        if (this._adaptiveIntervals.has(peerId)) {
            clearInterval(this._adaptiveIntervals.get(peerId));
        }

        let currentPreset = 'high';
        let stableCount = 0;

        // Check stats every 5 seconds
        const intervalId = setInterval(async () => {
            const pc = this.peerConnections.get(peerId);
            if (!pc) {
                this.disableAdaptiveQuality(peerId);
                return;
            }

            const stats = await this.getStats(peerId);
            if (!stats) {
                return;
            }

            const packetLoss = stats.packetLossPercent || 0;
            const rtt = stats.rttMs || 0;

            // Downgrade if packet loss > 5% or RTT > 300ms
            if (packetLoss > 5 || rtt > 300) {
                stableCount = 0;
                if (currentPreset !== 'low') {
                    const newPreset = currentPreset === 'ultra' ? 'high' :
                                      currentPreset === 'high' ? 'medium' : 'low';
                    console.log(`[SignalingManager] Adaptive: downgrading ${peerId} to ${newPreset} ` +
                                `(loss: ${packetLoss.toFixed(1)}%, RTT: ${rtt.toFixed(0)}ms)`);
                    await this.setQualityPreset(peerId, newPreset);
                    currentPreset = newPreset;
                }
            }
            // Upgrade if conditions are good for 3 consecutive checks
            else if (packetLoss < 1 && rtt < 100) {
                stableCount++;
                if (stableCount >= 3 && currentPreset !== 'high') {
                    const newPreset = currentPreset === 'low' ? 'medium' :
                                      currentPreset === 'medium' ? 'high' : 'high';
                    console.log(`[SignalingManager] Adaptive: upgrading ${peerId} to ${newPreset}`);
                    await this.setQualityPreset(peerId, newPreset);
                    currentPreset = newPreset;
                    stableCount = 0;
                }
            } else {
                stableCount = 0;
            }
        }, 5000);

        this._adaptiveIntervals.set(peerId, intervalId);
        console.log(`[SignalingManager] Adaptive quality enabled for ${peerId}`);
    }

    /**
     * Disable adaptive quality for a peer.
     * @param {string} peerId - Target peer ID
     */
    disableAdaptiveQuality(peerId) {
        if (this._adaptiveIntervals?.has(peerId)) {
            clearInterval(this._adaptiveIntervals.get(peerId));
            this._adaptiveIntervals.delete(peerId);
            console.log(`[SignalingManager] Adaptive quality disabled for ${peerId}`);
        }
    }

    /**
     * Get connection stats for a peer.
     * @param {string} peerId - Target peer ID
     * @returns {Promise<Object|null>} Stats object or null
     */
    async getStats(peerId) {
        const pc = this.peerConnections.get(peerId);
        if (!pc) {
            return null;
        }

        try {
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
                packetsReceived: 0,
                bytesSent: 0
            };

            stats.forEach(report => {
                // Get RTT from candidate pair
                if (report.type === 'candidate-pair' && report.state === 'succeeded') {
                    if (report.currentRoundTripTime !== undefined) {
                        result.rttMs = report.currentRoundTripTime * 1000;
                    }
                }
                // Get inbound video stats
                if (report.type === 'inbound-rtp' && report.kind === 'video') {
                    result.fps = report.framesPerSecond;
                    if (report.jitter !== undefined) {
                        result.jitterMs = report.jitter * 1000;
                    }
                    result.bytesReceived = report.bytesReceived || 0;
                    result.packetsLost = report.packetsLost || 0;
                    result.packetsReceived = report.packetsReceived || 0;
                }
                // Get outbound video stats for bitrate
                if (report.type === 'outbound-rtp' && report.kind === 'video') {
                    result.bytesSent = report.bytesSent || 0;
                    if (report.framesPerSecond !== undefined) {
                        result.fps = result.fps || report.framesPerSecond;
                    }
                }
            });

            // Calculate packet loss percentage
            const totalPackets = result.packetsReceived + result.packetsLost;
            if (totalPackets > 0) {
                result.packetLossPercent = (result.packetsLost / totalPackets) * 100;
            }

            return result;
        } catch (error) {
            console.error(`[SignalingManager] Failed to get stats for ${peerId}:`, error);
            return null;
        }
    }

    /**
     * Start collecting stats for all connections.
     * @param {function(Map<string, Object>)} callback - Called with stats for each peer
     * @param {number} [intervalMs=1000] - Collection interval
     * @returns {number} Interval ID (use with clearInterval to stop)
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
            if (allStats.size > 0) {
                callback(allStats);
            }
        }, intervalMs);
    }

    /**
     * Clean up all connections and resources.
     */
    cleanup() {
        console.log('[SignalingManager] Cleaning up all connections');
        this.disposed = true;

        // Clear all adaptive quality intervals
        if (this._adaptiveIntervals) {
            for (const intervalId of this._adaptiveIntervals.values()) {
                clearInterval(intervalId);
            }
            this._adaptiveIntervals.clear();
        }

        // End all calls
        for (const peerId of this.peerConnections.keys()) {
            this._cleanupConnection(peerId);
        }

        // Stop local stream
        if (this.localStream) {
            this.localStream.getTracks().forEach(track => track.stop());
            this.localStream = null;
        }

        this.onCallStateChangeCallbacks.clear();
    }

    // Private methods

    /**
     * Ensure local media stream is available.
     * @private
     */
    async _ensureLocalStream() {
        if (this.localStream) {
            return;
        }

        try {
            this.localStream = await navigator.mediaDevices.getUserMedia({
                video: {
                    width: { ideal: 1280 },
                    height: { ideal: 720 },
                    facingMode: 'user'
                },
                audio: false
            });
            console.log('[SignalingManager] Local stream acquired');
        } catch (error) {
            console.error('[SignalingManager] Failed to get local media:', error);
            throw error;
        }
    }

    /**
     * Handle incoming remote stream.
     * @param {string} peerId - Remote peer ID
     * @param {MediaStream} stream - Remote media stream
     * @private
     */
    async _handleRemoteStream(peerId, stream) {
        console.log(`[SignalingManager] Adding remote stream from ${peerId} to VideoManager`);
        try {
            await this.videoManager.addPeerStream(stream, peerId);
        } catch (error) {
            console.error(`[SignalingManager] Failed to add peer stream:`, error);
        }
    }

    /**
     * Process pending ICE candidates for a peer.
     * @param {string} peerId - Peer ID
     * @private
     */
    async _processPendingCandidates(peerId) {
        const pc = this.peerConnections.get(peerId);
        const pending = this.pendingCandidates.get(peerId) || [];

        console.debug(`[SignalingManager] Processing ${pending.length} pending candidates for ${peerId}`);

        for (const candidate of pending) {
            try {
                await pc.addIceCandidate(candidate);
            } catch (error) {
                console.error(`[SignalingManager] Failed to add pending candidate:`, error);
            }
        }

        this.pendingCandidates.set(peerId, []);
    }

    /**
     * Clean up a peer connection.
     * @param {string} peerId - Peer ID to clean up
     * @private
     */
    _cleanupConnection(peerId) {
        const pc = this.peerConnections.get(peerId);
        if (pc) {
            pc.onicecandidate = null;
            pc.oniceconnectionstatechange = null;
            pc.onconnectionstatechange = null;
            pc.ontrack = null;
            pc.close();
            this.peerConnections.delete(peerId);
        }

        this.pendingCandidates.delete(peerId);

        // Remove stream from VideoManager
        const streamId = `peer-${peerId}`;
        if (this.videoManager.getStreamInfo(streamId)) {
            this.videoManager.removeStream(streamId);
        }
    }

    /**
     * Send a signaling message through WebSocket.
     * @param {string} type - Message type
     * @param {Object} payload - Message payload
     * @private
     */
    _sendSignaling(type, payload) {
        if (typeof window.sendSignaling === 'function') {
            window.sendSignaling(type, payload);
        } else {
            console.error('[SignalingManager] sendSignaling not available');
        }
    }

    /**
     * Notify call state change callbacks.
     * @param {string} peerId - Peer ID
     * @param {string} state - New state
     * @private
     */
    _notifyCallStateChange(peerId, state) {
        for (const callback of this.onCallStateChangeCallbacks) {
            try {
                callback(peerId, state);
            } catch (error) {
                console.error('[SignalingManager] Call state callback error:', error);
            }
        }
    }
}

// Export for ES modules
export default SignalingManager;
