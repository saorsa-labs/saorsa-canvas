/**
 * Canvas Renderer for Saorsa Canvas
 *
 * Handles rendering of scene elements including video frames.
 * Uses Canvas2D API with requestAnimationFrame for smooth rendering.
 */

/**
 * Renders scene elements to a canvas, including video frames.
 */
export class CanvasRenderer {
    /**
     * Create a new CanvasRenderer.
     * @param {HTMLCanvasElement} canvasElement - The canvas to render to
     * @param {VideoManager} videoManager - Video manager for frame extraction
     */
    constructor(canvasElement, videoManager) {
        /** @type {HTMLCanvasElement} */
        this.canvas = canvasElement;

        /** @type {CanvasRenderingContext2D} */
        this.ctx = canvasElement.getContext('2d');

        /** @type {VideoManager} */
        this.videoManager = videoManager;

        /** @type {Object|null} Current scene data */
        this.scene = null;

        /** @type {number|null} Animation frame request ID */
        this.frameId = null;

        /** @type {boolean} Whether the render loop is running */
        this.running = false;

        /** @type {boolean} Whether to show debug overlay */
        this.debugMode = false;

        /** @type {number[]} Frame timestamps for FPS calculation */
        this.frameTimes = [];

        /** @type {Map<string, ImageBitmap>} Cached image bitmaps */
        this.imageCache = new Map();

        /** @type {Map<string, Object>|null} Media stats by peer ID */
        this.mediaStats = null;

        console.log('[CanvasRenderer] Initialized');
    }

    /**
     * Set media stats for display in debug overlay.
     * @param {Map<string, Object>} stats - Map of peer ID to stats object
     */
    setMediaStats(stats) {
        this.mediaStats = stats;
    }

    /**
     * Set the scene to render.
     * @param {Object} scene - Scene data with elements array
     */
    setScene(scene) {
        this.scene = scene;
    }

    /**
     * Start the render loop.
     */
    start() {
        if (this.running) {
            return;
        }

        this.running = true;
        this.frameTimes = [];
        this.renderLoop();
        console.log('[CanvasRenderer] Render loop started');
    }

    /**
     * Stop the render loop.
     */
    stop() {
        this.running = false;
        if (this.frameId !== null) {
            cancelAnimationFrame(this.frameId);
            this.frameId = null;
        }
        console.log('[CanvasRenderer] Render loop stopped');
    }

    /**
     * Toggle debug overlay.
     * @returns {boolean} New debug mode state
     */
    toggleDebug() {
        this.debugMode = !this.debugMode;
        console.log(`[CanvasRenderer] Debug mode: ${this.debugMode}`);
        return this.debugMode;
    }

    /**
     * Main render loop using requestAnimationFrame.
     * @private
     */
    renderLoop() {
        if (!this.running) {
            return;
        }

        this.render();
        this.frameId = requestAnimationFrame(() => this.renderLoop());
    }

    /**
     * Render a single frame.
     */
    render() {
        const now = performance.now();
        this.frameTimes.push(now);

        // Keep only last 60 frame times for FPS calculation
        if (this.frameTimes.length > 60) {
            this.frameTimes.shift();
        }

        // Clear canvas
        this.ctx.fillStyle = '#1a1a2e';
        this.ctx.fillRect(0, 0, this.canvas.width, this.canvas.height);

        // Render scene elements if available
        if (this.scene && this.scene.elements) {
            // Sort by z-index
            const elements = [...this.scene.elements].sort(
                (a, b) => (a.transform?.z_index || 0) - (b.transform?.z_index || 0)
            );

            for (const element of elements) {
                this.renderElement(element);
            }
        }

        // Render debug overlay if enabled
        if (this.debugMode) {
            this.renderDebugOverlay();
        }
    }

    /**
     * Render a single element.
     * @param {Object} element - Element to render
     * @private
     */
    renderElement(element) {
        const kind = element.kind;
        const transform = element.transform || {};

        if (!kind || !kind.type) {
            return;
        }

        switch (kind.type) {
            case 'Video':
                this.renderVideoElement(element);
                break;
            case 'Text':
                this.renderTextElement(element);
                break;
            case 'Image':
                this.renderImageElement(element);
                break;
            case 'Chart':
                this.renderChartPlaceholder(element);
                break;
            default:
                this.renderPlaceholder(element);
        }
    }

    /**
     * Render a video element.
     * @param {Object} element - Video element
     * @private
     */
    renderVideoElement(element) {
        const { stream_id, mirror, crop } = element.kind;
        const transform = element.transform || {};
        const x = transform.x || 0;
        const y = transform.y || 0;
        const width = transform.width || 320;
        const height = transform.height || 240;
        const rotation = transform.rotation || 0;

        this.ctx.save();

        // Apply rotation around element center
        if (rotation !== 0) {
            const cx = x + width / 2;
            const cy = y + height / 2;
            this.ctx.translate(cx, cy);
            this.ctx.rotate((rotation * Math.PI) / 180);
            this.ctx.translate(-cx, -cy);
        }

        // Check if stream is ready
        if (!this.videoManager.isStreamReady(stream_id)) {
            // Render placeholder for unavailable stream
            this.renderStreamPlaceholder(x, y, width, height, stream_id);
            this.ctx.restore();
            return;
        }

        // Get video frame
        const frame = this.videoManager.getVideoFrame(stream_id, crop);
        if (!frame) {
            this.renderStreamPlaceholder(x, y, width, height, stream_id);
            this.ctx.restore();
            return;
        }

        // Apply mirroring if needed
        if (mirror) {
            this.ctx.translate(x + width, y);
            this.ctx.scale(-1, 1);
            this.ctx.translate(-x, -y);
        }

        // Draw the video frame
        // Create a temporary canvas to draw ImageData
        const tempCanvas = new OffscreenCanvas(frame.width, frame.height);
        const tempCtx = tempCanvas.getContext('2d');
        tempCtx.putImageData(frame, 0, 0);

        // Draw to main canvas scaled to element size
        this.ctx.drawImage(tempCanvas, x, y, width, height);

        // Draw border
        this.ctx.strokeStyle = '#4a4a6a';
        this.ctx.lineWidth = 2;
        this.ctx.strokeRect(x, y, width, height);

        this.ctx.restore();
    }

    /**
     * Render a placeholder for unavailable video stream.
     * @param {number} x - X position
     * @param {number} y - Y position
     * @param {number} width - Width
     * @param {number} height - Height
     * @param {string} streamId - Stream identifier
     * @private
     */
    renderStreamPlaceholder(x, y, width, height, streamId) {
        // Dark background
        this.ctx.fillStyle = '#2a2a3e';
        this.ctx.fillRect(x, y, width, height);

        // Border
        this.ctx.strokeStyle = '#4a4a6a';
        this.ctx.lineWidth = 2;
        this.ctx.strokeRect(x, y, width, height);

        // Camera icon placeholder (simple)
        this.ctx.fillStyle = '#6a6a8a';
        const iconSize = Math.min(width, height) * 0.3;
        const iconX = x + (width - iconSize) / 2;
        const iconY = y + (height - iconSize) / 2;
        this.ctx.beginPath();
        this.ctx.arc(iconX + iconSize / 2, iconY + iconSize / 2, iconSize / 2, 0, Math.PI * 2);
        this.ctx.fill();

        // Stream ID text
        this.ctx.fillStyle = '#8a8aaa';
        this.ctx.font = '12px monospace';
        this.ctx.textAlign = 'center';
        this.ctx.fillText(streamId || 'No stream', x + width / 2, y + height - 10);
        this.ctx.textAlign = 'left';
    }

    /**
     * Render a text element.
     * @param {Object} element - Text element
     * @private
     */
    renderTextElement(element) {
        const { content, font_size, color } = element.kind;
        const transform = element.transform || {};
        const x = transform.x || 0;
        const y = transform.y || 0;

        this.ctx.save();
        this.ctx.fillStyle = color || '#ffffff';
        this.ctx.font = `${font_size || 16}px sans-serif`;
        this.ctx.fillText(content || '', x, y + (font_size || 16));
        this.ctx.restore();
    }

    /**
     * Render an image element.
     * @param {Object} element - Image element
     * @private
     */
    renderImageElement(element) {
        const { src } = element.kind;
        const transform = element.transform || {};
        const x = transform.x || 0;
        const y = transform.y || 0;
        const width = transform.width || 100;
        const height = transform.height || 100;

        // For now, render as placeholder with image info
        this.ctx.save();
        this.ctx.fillStyle = '#3a3a4e';
        this.ctx.fillRect(x, y, width, height);
        this.ctx.strokeStyle = '#5a5a7a';
        this.ctx.strokeRect(x, y, width, height);

        this.ctx.fillStyle = '#8a8aaa';
        this.ctx.font = '12px monospace';
        this.ctx.textAlign = 'center';
        this.ctx.fillText('Image', x + width / 2, y + height / 2);
        this.ctx.textAlign = 'left';
        this.ctx.restore();
    }

    /**
     * Render a chart placeholder.
     * @param {Object} element - Chart element
     * @private
     */
    renderChartPlaceholder(element) {
        const transform = element.transform || {};
        const x = transform.x || 0;
        const y = transform.y || 0;
        const width = transform.width || 200;
        const height = transform.height || 150;

        this.ctx.save();
        this.ctx.fillStyle = '#2a3a4e';
        this.ctx.fillRect(x, y, width, height);
        this.ctx.strokeStyle = '#4a5a7a';
        this.ctx.strokeRect(x, y, width, height);

        this.ctx.fillStyle = '#7a8aaa';
        this.ctx.font = '14px sans-serif';
        this.ctx.textAlign = 'center';
        this.ctx.fillText('Chart', x + width / 2, y + height / 2);
        this.ctx.textAlign = 'left';
        this.ctx.restore();
    }

    /**
     * Render a generic placeholder for unknown elements.
     * @param {Object} element - Element to render
     * @private
     */
    renderPlaceholder(element) {
        const transform = element.transform || {};
        const x = transform.x || 0;
        const y = transform.y || 0;
        const width = transform.width || 100;
        const height = transform.height || 100;

        this.ctx.save();
        this.ctx.fillStyle = '#3a3a3a';
        this.ctx.fillRect(x, y, width, height);
        this.ctx.strokeStyle = '#5a5a5a';
        this.ctx.strokeRect(x, y, width, height);

        this.ctx.fillStyle = '#8a8a8a';
        this.ctx.font = '10px monospace';
        this.ctx.textAlign = 'center';
        this.ctx.fillText(element.kind?.type || 'Unknown', x + width / 2, y + height / 2);
        this.ctx.textAlign = 'left';
        this.ctx.restore();
    }

    /**
     * Render debug overlay with stats.
     * @private
     */
    renderDebugOverlay() {
        const padding = 10;
        const lineHeight = 16;
        let y = padding + lineHeight;

        // Calculate height based on content
        const baseLines = 5; // FPS, Streams, Elements, Canvas, separator
        const statsLines = this.mediaStats ? this.mediaStats.size * 2 + 1 : 0;
        const totalHeight = (baseLines + statsLines) * lineHeight + 10;

        // Semi-transparent background
        this.ctx.fillStyle = 'rgba(0, 0, 0, 0.8)';
        this.ctx.fillRect(padding - 5, padding - 5, 280, totalHeight);

        this.ctx.font = '13px monospace';
        this.ctx.fillStyle = '#00ff00';

        // FPS
        const fps = this.calculateFPS();
        this.ctx.fillText(`FPS: ${fps.toFixed(1)}`, padding, y);
        y += lineHeight;

        // Active streams
        const streams = this.videoManager.getStreamIds();
        this.ctx.fillText(`Streams: ${streams.length}`, padding, y);
        y += lineHeight;

        // Element count
        const elements = this.scene?.elements?.length || 0;
        this.ctx.fillText(`Elements: ${elements}`, padding, y);
        y += lineHeight;

        // Canvas size
        this.ctx.fillText(`Canvas: ${this.canvas.width}x${this.canvas.height}`, padding, y);
        y += lineHeight;

        // Media stats section
        if (this.mediaStats && this.mediaStats.size > 0) {
            y += lineHeight / 2;
            this.ctx.fillStyle = '#ffff00';
            this.ctx.fillText('─── Media Stats ───', padding, y);
            y += lineHeight;

            for (const [peerId, stats] of this.mediaStats) {
                // Get short peer ID (first 12 chars)
                const shortId = peerId.length > 12 ? peerId.substring(0, 12) + '…' : peerId;

                // Determine quality color based on stats
                const color = this._getQualityColor(stats);
                this.ctx.fillStyle = color;

                // Format stats
                const rtt = stats.rttMs !== null ? `${stats.rttMs.toFixed(0)}ms` : '?';
                const loss = stats.packetLossPercent !== null ? `${stats.packetLossPercent.toFixed(1)}%` : '?';
                const fps = stats.fps !== null ? `${stats.fps.toFixed(0)}fps` : '?';

                this.ctx.fillText(`${shortId}:`, padding, y);
                y += lineHeight;

                this.ctx.fillText(`  RTT:${rtt} Loss:${loss} ${fps}`, padding, y);
                y += lineHeight;
            }
        }
    }

    /**
     * Get color based on connection quality.
     * @param {Object} stats - Stats object
     * @returns {string} CSS color
     * @private
     */
    _getQualityColor(stats) {
        const loss = stats.packetLossPercent || 0;
        const rtt = stats.rttMs || 0;

        // Red: bad quality (high loss or very high RTT)
        if (loss > 5 || rtt > 300) {
            return '#ff4444';
        }
        // Yellow: moderate quality
        if (loss > 2 || rtt > 150) {
            return '#ffaa00';
        }
        // Green: good quality
        return '#44ff44';
    }

    /**
     * Calculate current FPS from frame times.
     * @returns {number} Current FPS
     * @private
     */
    calculateFPS() {
        if (this.frameTimes.length < 2) {
            return 0;
        }

        const oldest = this.frameTimes[0];
        const newest = this.frameTimes[this.frameTimes.length - 1];
        const elapsed = newest - oldest;

        if (elapsed === 0) {
            return 0;
        }

        return ((this.frameTimes.length - 1) * 1000) / elapsed;
    }

    /**
     * Resize the canvas.
     * @param {number} width - New width
     * @param {number} height - New height
     */
    resize(width, height) {
        this.canvas.width = width;
        this.canvas.height = height;
        console.log(`[CanvasRenderer] Resized to ${width}x${height}`);
    }

    /**
     * Clean up resources.
     */
    cleanup() {
        this.stop();
        this.imageCache.clear();
        this.scene = null;
        console.log('[CanvasRenderer] Cleaned up');
    }
}

// Export for ES modules
export default CanvasRenderer;
