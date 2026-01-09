/**
 * Video Manager for Saorsa Canvas
 *
 * Handles WebRTC video feeds, local camera capture, and video frame extraction
 * for compositing into the canvas.
 */

/**
 * Video stream metadata.
 * @typedef {Object} VideoStreamInfo
 * @property {string} id - Stream identifier
 * @property {number} width - Video width in pixels
 * @property {number} height - Video height in pixels
 * @property {boolean} isLocal - Whether this is a local camera stream
 * @property {boolean} mirror - Whether to mirror the video
 */

/**
 * Manages video streams for canvas compositing.
 */
export class VideoManager {
    constructor() {
        /** @type {Map<string, HTMLVideoElement>} */
        this.streams = new Map();

        /** @type {Map<string, VideoStreamInfo>} */
        this.streamInfo = new Map();

        /** @type {OffscreenCanvas|null} */
        this.scratchCanvas = null;

        /** @type {OffscreenCanvasRenderingContext2D|null} */
        this.scratchCtx = null;

        /** @type {Set<function>} */
        this.onStreamChangeCallbacks = new Set();
    }

    /**
     * Add a local camera stream.
     * @param {Object} options - Camera options
     * @param {boolean} [options.mirror=true] - Whether to mirror the video
     * @param {string} [options.facingMode='user'] - 'user' for front camera, 'environment' for back
     * @param {number} [options.width=1280] - Preferred width
     * @param {number} [options.height=720] - Preferred height
     * @returns {Promise<string>} Stream ID
     */
    async addLocalCamera(options = {}) {
        const {
            mirror = true,
            facingMode = 'user',
            width = 1280,
            height = 720
        } = options;

        try {
            const stream = await navigator.mediaDevices.getUserMedia({
                video: {
                    facingMode,
                    width: { ideal: width },
                    height: { ideal: height }
                },
                audio: false
            });

            const video = document.createElement('video');
            video.srcObject = stream;
            video.playsInline = true;
            video.muted = true;

            await video.play();

            const streamId = 'local';
            const track = stream.getVideoTracks()[0];
            const settings = track.getSettings();

            this.streams.set(streamId, video);
            this.streamInfo.set(streamId, {
                id: streamId,
                width: settings.width || video.videoWidth,
                height: settings.height || video.videoHeight,
                isLocal: true,
                mirror
            });

            this._notifyStreamChange('added', streamId);
            console.log(`[VideoManager] Local camera added: ${settings.width}x${settings.height}`);

            return streamId;
        } catch (error) {
            console.error('[VideoManager] Failed to access camera:', error);
            throw error;
        }
    }

    /**
     * Add a video stream from a URL (for testing or recorded video).
     * @param {string} url - Video URL
     * @param {string} [streamId] - Optional stream ID (defaults to URL hash)
     * @returns {Promise<string>} Stream ID
     */
    async addVideoUrl(url, streamId = null) {
        const id = streamId || `video-${this._hashCode(url)}`;

        const video = document.createElement('video');
        video.src = url;
        video.playsInline = true;
        video.muted = true;
        video.loop = true;
        video.crossOrigin = 'anonymous';

        await new Promise((resolve, reject) => {
            video.onloadedmetadata = resolve;
            video.onerror = reject;
        });

        await video.play();

        this.streams.set(id, video);
        this.streamInfo.set(id, {
            id,
            width: video.videoWidth,
            height: video.videoHeight,
            isLocal: false,
            mirror: false
        });

        this._notifyStreamChange('added', id);
        console.log(`[VideoManager] Video URL added: ${id} (${video.videoWidth}x${video.videoHeight})`);

        return id;
    }

    /**
     * Add a WebRTC peer stream.
     * @param {MediaStream} mediaStream - WebRTC MediaStream
     * @param {string} peerId - Peer identifier
     * @returns {Promise<string>} Stream ID
     */
    async addPeerStream(mediaStream, peerId) {
        const streamId = `peer-${peerId}`;

        const video = document.createElement('video');
        video.srcObject = mediaStream;
        video.playsInline = true;
        video.muted = true;

        await video.play();

        const track = mediaStream.getVideoTracks()[0];
        const settings = track?.getSettings() || {};

        this.streams.set(streamId, video);
        this.streamInfo.set(streamId, {
            id: streamId,
            width: settings.width || video.videoWidth,
            height: settings.height || video.videoHeight,
            isLocal: false,
            mirror: false
        });

        this._notifyStreamChange('added', streamId);
        console.log(`[VideoManager] Peer stream added: ${streamId}`);

        return streamId;
    }

    /**
     * Remove a video stream.
     * @param {string} streamId - Stream ID to remove
     */
    removeStream(streamId) {
        const video = this.streams.get(streamId);
        if (video) {
            // Stop all tracks if it's a MediaStream
            if (video.srcObject instanceof MediaStream) {
                video.srcObject.getTracks().forEach(track => track.stop());
            }
            video.pause();
            video.srcObject = null;
            video.src = '';

            this.streams.delete(streamId);
            this.streamInfo.delete(streamId);

            this._notifyStreamChange('removed', streamId);
            console.log(`[VideoManager] Stream removed: ${streamId}`);
        }
    }

    /**
     * Get video frame as ImageData.
     * @param {string} streamId - Stream ID
     * @param {Object} [crop] - Optional crop region (normalized 0-1)
     * @param {number} crop.x - Left edge
     * @param {number} crop.y - Top edge
     * @param {number} crop.width - Width
     * @param {number} crop.height - Height
     * @returns {ImageData|null} Video frame as ImageData, or null if not available
     */
    getVideoFrame(streamId, crop = null) {
        const video = this.streams.get(streamId);
        const info = this.streamInfo.get(streamId);

        if (!video || video.readyState < 2) {
            return null;
        }

        // Determine source and destination dimensions
        let srcX = 0, srcY = 0, srcW = video.videoWidth, srcH = video.videoHeight;

        if (crop) {
            srcX = Math.floor(crop.x * video.videoWidth);
            srcY = Math.floor(crop.y * video.videoHeight);
            srcW = Math.floor(crop.width * video.videoWidth);
            srcH = Math.floor(crop.height * video.videoHeight);
        }

        // Ensure scratch canvas exists and is sized correctly
        if (!this.scratchCanvas ||
            this.scratchCanvas.width !== srcW ||
            this.scratchCanvas.height !== srcH) {
            this.scratchCanvas = new OffscreenCanvas(srcW, srcH);
            this.scratchCtx = this.scratchCanvas.getContext('2d');
        }

        // Draw video frame
        this.scratchCtx.save();

        // Apply mirroring if needed
        if (info?.mirror) {
            this.scratchCtx.translate(srcW, 0);
            this.scratchCtx.scale(-1, 1);
        }

        this.scratchCtx.drawImage(
            video,
            srcX, srcY, srcW, srcH,  // Source rect
            0, 0, srcW, srcH          // Dest rect
        );

        this.scratchCtx.restore();

        return this.scratchCtx.getImageData(0, 0, srcW, srcH);
    }

    /**
     * Get video frame as Uint8Array (RGBA).
     * @param {string} streamId - Stream ID
     * @param {Object} [crop] - Optional crop region
     * @returns {Uint8Array|null} Video frame as RGBA bytes
     */
    getVideoFrameBytes(streamId, crop = null) {
        const imageData = this.getVideoFrame(streamId, crop);
        return imageData ? new Uint8Array(imageData.data.buffer) : null;
    }

    /**
     * Get all active stream IDs.
     * @returns {string[]} Array of stream IDs
     */
    getStreamIds() {
        return Array.from(this.streams.keys());
    }

    /**
     * Get stream info.
     * @param {string} streamId - Stream ID
     * @returns {VideoStreamInfo|null} Stream info or null
     */
    getStreamInfo(streamId) {
        return this.streamInfo.get(streamId) || null;
    }

    /**
     * Check if a stream is ready for frame extraction.
     * @param {string} streamId - Stream ID
     * @returns {boolean} True if ready
     */
    isStreamReady(streamId) {
        const video = this.streams.get(streamId);
        return video && video.readyState >= 2;
    }

    /**
     * Register a callback for stream changes.
     * @param {function(string, string)} callback - Callback(action, streamId)
     */
    onStreamChange(callback) {
        this.onStreamChangeCallbacks.add(callback);
    }

    /**
     * Unregister a stream change callback.
     * @param {function} callback - Callback to remove
     */
    offStreamChange(callback) {
        this.onStreamChangeCallbacks.delete(callback);
    }

    /**
     * Clean up all streams.
     */
    dispose() {
        for (const streamId of this.streams.keys()) {
            this.removeStream(streamId);
        }
        this.onStreamChangeCallbacks.clear();
        this.scratchCanvas = null;
        this.scratchCtx = null;
    }

    // Private methods

    _notifyStreamChange(action, streamId) {
        for (const callback of this.onStreamChangeCallbacks) {
            try {
                callback(action, streamId);
            } catch (error) {
                console.error('[VideoManager] Stream change callback error:', error);
            }
        }
    }

    _hashCode(str) {
        let hash = 0;
        for (let i = 0; i < str.length; i++) {
            const char = str.charCodeAt(i);
            hash = ((hash << 5) - hash) + char;
            hash = hash & hash;
        }
        return Math.abs(hash).toString(16).slice(0, 8);
    }
}

// Export a singleton instance for convenience
export const videoManager = new VideoManager();
