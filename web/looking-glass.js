/**
 * Looking Glass Holographic Display Integration
 *
 * Provides WebXR and HoloPlay Service integration for
 * displaying Saorsa Canvas content on Looking Glass devices.
 *
 * @module looking-glass
 */

/**
 * Looking Glass display configuration
 */
const LookingGlassPresets = {
    Portrait: {
        name: 'Looking Glass Portrait',
        numViews: 45,
        quiltColumns: 5,
        quiltRows: 9,
        viewWidth: 420,
        viewHeight: 560,
        viewCone: 40 * (Math.PI / 180), // 40 degrees in radians
        focalDistance: 2.0,
        displayWidth: 1536,
        displayHeight: 2048
    },
    LG16: {
        name: 'Looking Glass 16"',
        numViews: 45,
        quiltColumns: 5,
        quiltRows: 9,
        viewWidth: 768,
        viewHeight: 432,
        viewCone: 40 * (Math.PI / 180),
        focalDistance: 3.0,
        displayWidth: 3840,
        displayHeight: 2160
    },
    LG32: {
        name: 'Looking Glass 32"',
        numViews: 45,
        quiltColumns: 5,
        quiltRows: 9,
        viewWidth: 1536,
        viewHeight: 864,
        viewCone: 40 * (Math.PI / 180),
        focalDistance: 4.0,
        displayWidth: 7680,
        displayHeight: 4320
    },
    Go: {
        name: 'Looking Glass Go',
        numViews: 45,
        quiltColumns: 5,
        quiltRows: 9,
        viewWidth: 288,
        viewHeight: 512,
        viewCone: 35 * (Math.PI / 180),
        focalDistance: 2.0,
        displayWidth: 1440,
        displayHeight: 2560
    }
};

/**
 * Vector3 utility class for 3D math
 */
class Vec3 {
    constructor(x = 0, y = 0, z = 0) {
        this.x = x;
        this.y = y;
        this.z = z;
    }

    add(other) {
        return new Vec3(this.x + other.x, this.y + other.y, this.z + other.z);
    }

    sub(other) {
        return new Vec3(this.x - other.x, this.y - other.y, this.z - other.z);
    }

    scale(s) {
        return new Vec3(this.x * s, this.y * s, this.z * s);
    }

    length() {
        return Math.sqrt(this.x * this.x + this.y * this.y + this.z * this.z);
    }

    normalize() {
        const len = this.length();
        if (len === 0) return new Vec3(0, 0, 0);
        return this.scale(1 / len);
    }

    dot(other) {
        return this.x * other.x + this.y * other.y + this.z * other.z;
    }

    cross(other) {
        return new Vec3(
            this.y * other.z - this.z * other.y,
            this.z * other.x - this.x * other.z,
            this.x * other.y - this.y * other.x
        );
    }
}

/**
 * Camera class for 3D view calculations
 */
class Camera {
    constructor(position = new Vec3(0, 0, 5), target = new Vec3(0, 0, 0), up = new Vec3(0, 1, 0)) {
        this.position = position;
        this.target = target;
        this.up = up;
        this.fov = 45 * (Math.PI / 180); // 45 degrees
        this.near = 0.1;
        this.far = 100;
    }

    /**
     * Calculate camera for a specific view in the quilt
     * @param {object} config - Looking Glass configuration
     * @param {number} viewIndex - View index (0 to numViews-1)
     * @returns {Camera} Camera for this view
     */
    forView(config, viewIndex) {
        if (config.numViews <= 1) {
            return this;
        }

        // Calculate angle offset for this view
        const t = viewIndex / (config.numViews - 1);
        const angle = (t - 0.5) * config.viewCone;

        // Calculate new camera position by rotating around target
        const dir = this.position.sub(this.target);
        const distance = dir.length();

        // Rotate direction around Y axis
        const cosA = Math.cos(angle);
        const sinA = Math.sin(angle);
        const newDir = new Vec3(
            dir.x * cosA + dir.z * sinA,
            dir.y,
            -dir.x * sinA + dir.z * cosA
        );

        const camera = new Camera();
        camera.position = this.target.add(newDir.normalize().scale(distance));
        camera.target = this.target;
        camera.up = this.up;
        camera.fov = this.fov;
        camera.near = this.near;
        camera.far = this.far;
        return camera;
    }

    /**
     * Get the view matrix for this camera
     * @returns {Float32Array} 4x4 view matrix in column-major order
     */
    viewMatrix() {
        const forward = this.target.sub(this.position).normalize();
        const right = forward.cross(this.up).normalize();
        const up = right.cross(forward);

        return new Float32Array([
            right.x, up.x, -forward.x, 0,
            right.y, up.y, -forward.y, 0,
            right.z, up.z, -forward.z, 0,
            -right.dot(this.position), -up.dot(this.position), forward.dot(this.position), 1
        ]);
    }

    /**
     * Get the projection matrix for this camera
     * @param {number} aspect - Aspect ratio
     * @returns {Float32Array} 4x4 projection matrix in column-major order
     */
    projectionMatrix(aspect) {
        const f = 1 / Math.tan(this.fov / 2);
        const rangeInv = 1 / (this.near - this.far);

        return new Float32Array([
            f / aspect, 0, 0, 0,
            0, f, 0, 0,
            0, 0, (this.far + this.near) * rangeInv, -1,
            0, 0, 2 * this.far * this.near * rangeInv, 0
        ]);
    }
}

/**
 * Quilt renderer for Looking Glass displays
 */
class QuiltRenderer {
    constructor(config = LookingGlassPresets.Portrait) {
        this.config = config;
        this.canvas = null;
        this.gl = null;
        this.initialized = false;
    }

    /**
     * Initialize the renderer with a canvas
     * @param {HTMLCanvasElement} canvas - Canvas to render to
     */
    init(canvas) {
        this.canvas = canvas;
        this.gl = canvas.getContext('webgl2') || canvas.getContext('webgl');

        if (!this.gl) {
            throw new Error('WebGL not supported');
        }

        this.initialized = true;
        this.resize();
    }

    /**
     * Resize canvas to quilt dimensions
     */
    resize() {
        if (!this.canvas) return;

        this.canvas.width = this.config.quiltColumns * this.config.viewWidth;
        this.canvas.height = this.config.quiltRows * this.config.viewHeight;

        if (this.gl) {
            this.gl.viewport(0, 0, this.canvas.width, this.canvas.height);
        }
    }

    /**
     * Get quilt dimensions
     * @returns {{width: number, height: number}} Quilt dimensions
     */
    getQuiltDimensions() {
        return {
            width: this.config.quiltColumns * this.config.viewWidth,
            height: this.config.quiltRows * this.config.viewHeight
        };
    }

    /**
     * Get view offset in the quilt texture
     * @param {number} viewIndex - View index
     * @returns {{x: number, y: number}} Pixel offset
     */
    getViewOffset(viewIndex) {
        const col = viewIndex % this.config.quiltColumns;
        const row = Math.floor(viewIndex / this.config.quiltColumns);
        return {
            x: col * this.config.viewWidth,
            y: row * this.config.viewHeight
        };
    }

    /**
     * Render all views to the quilt
     * @param {Camera} baseCamera - Base camera position
     * @param {Function} renderCallback - Callback to render each view: (camera, viewIndex) => void
     */
    renderQuilt(baseCamera, renderCallback) {
        if (!this.initialized || !this.gl) {
            console.warn('QuiltRenderer not initialized');
            return;
        }

        const gl = this.gl;

        for (let i = 0; i < this.config.numViews; i++) {
            const camera = baseCamera.forView(this.config, i);
            const offset = this.getViewOffset(i);

            // Set viewport for this view
            gl.viewport(
                offset.x,
                offset.y,
                this.config.viewWidth,
                this.config.viewHeight
            );

            // Enable scissor test to constrain rendering
            gl.enable(gl.SCISSOR_TEST);
            gl.scissor(
                offset.x,
                offset.y,
                this.config.viewWidth,
                this.config.viewHeight
            );

            // Call the render callback for this view
            renderCallback(camera, i);
        }

        // Disable scissor test
        gl.disable(gl.SCISSOR_TEST);
    }

    /**
     * Generate a test pattern quilt (for debugging)
     * @returns {ImageData} Quilt image data
     */
    generateTestPattern() {
        const dims = this.getQuiltDimensions();
        const imageData = new ImageData(dims.width, dims.height);
        const data = imageData.data;

        for (let i = 0; i < this.config.numViews; i++) {
            const offset = this.getViewOffset(i);
            const t = i / (this.config.numViews - 1);

            // Color gradient: blue (left) to red (right)
            const r = Math.floor(t * 255);
            const g = 100;
            const b = Math.floor((1 - t) * 255);

            for (let y = 0; y < this.config.viewHeight; y++) {
                for (let x = 0; x < this.config.viewWidth; x++) {
                    const px = offset.x + x;
                    const py = offset.y + y;
                    const idx = (py * dims.width + px) * 4;

                    // Border
                    const isBorder = x < 2 || x >= this.config.viewWidth - 2 ||
                                   y < 2 || y >= this.config.viewHeight - 2;

                    if (isBorder) {
                        data[idx] = 255;
                        data[idx + 1] = 255;
                        data[idx + 2] = 255;
                        data[idx + 3] = 128;
                    } else {
                        data[idx] = r;
                        data[idx + 1] = g;
                        data[idx + 2] = b;
                        data[idx + 3] = 255;
                    }
                }
            }
        }

        return imageData;
    }
}

/**
 * HoloPlay Service connection manager
 */
class HoloPlayService {
    constructor() {
        this.connected = false;
        this.displays = [];
        this.activeDisplay = null;
        this.ws = null;
    }

    /**
     * Attempt to connect to HoloPlay Service
     * @returns {Promise<boolean>} True if connected
     */
    async connect() {
        return new Promise((resolve) => {
            try {
                // HoloPlay Service runs on localhost:11222
                this.ws = new WebSocket('ws://localhost:11222');

                this.ws.onopen = () => {
                    this.connected = true;
                    this.queryDisplays();
                    resolve(true);
                };

                this.ws.onerror = () => {
                    this.connected = false;
                    resolve(false);
                };

                this.ws.onclose = () => {
                    this.connected = false;
                };

                this.ws.onmessage = (event) => {
                    this.handleMessage(JSON.parse(event.data));
                };

                // Timeout after 2 seconds
                setTimeout(() => {
                    if (!this.connected) {
                        resolve(false);
                    }
                }, 2000);
            } catch (e) {
                this.connected = false;
                resolve(false);
            }
        });
    }

    /**
     * Query available displays
     */
    queryDisplays() {
        if (this.ws && this.connected) {
            this.ws.send(JSON.stringify({
                cmd: 'info'
            }));
        }
    }

    /**
     * Handle incoming messages from HoloPlay Service
     * @param {object} message - Parsed JSON message
     */
    handleMessage(message) {
        if (message.devices) {
            this.displays = message.devices;
            if (this.displays.length > 0) {
                this.activeDisplay = this.displays[0];
            }

            // Dispatch custom event
            window.dispatchEvent(new CustomEvent('holoplay-displays', {
                detail: { displays: this.displays }
            }));
        }
    }

    /**
     * Send a quilt to the Looking Glass display
     * @param {HTMLCanvasElement} quiltCanvas - Canvas containing the quilt
     */
    sendQuilt(quiltCanvas) {
        if (!this.connected || !this.activeDisplay) {
            console.warn('Not connected to HoloPlay Service or no display available');
            return;
        }

        // Get quilt as data URL
        const dataUrl = quiltCanvas.toDataURL('image/png');

        // Send to HoloPlay Service
        this.ws.send(JSON.stringify({
            cmd: 'show',
            source: 'saorsa-canvas',
            quilt: {
                data: dataUrl,
                settings: {
                    columns: this.activeDisplay.calibration?.quiltX || 5,
                    rows: this.activeDisplay.calibration?.quiltY || 9,
                    viewCount: this.activeDisplay.calibration?.viewCount || 45
                }
            }
        }));
    }

    /**
     * Disconnect from HoloPlay Service
     */
    disconnect() {
        if (this.ws) {
            this.ws.close();
            this.ws = null;
        }
        this.connected = false;
        this.displays = [];
        this.activeDisplay = null;
    }

    /**
     * Get information about connected displays
     * @returns {Array} List of display info objects
     */
    getDisplayInfo() {
        return this.displays.map(d => ({
            name: d.name || 'Looking Glass',
            serial: d.serial || 'unknown',
            width: d.screenW || 0,
            height: d.screenH || 0,
            viewCone: d.calibration?.viewCone || 40
        }));
    }
}

/**
 * Looking Glass WebXR integration
 */
class LookingGlassXR {
    constructor() {
        this.xrSession = null;
        this.xrRefSpace = null;
        this.isSupported = false;
    }

    /**
     * Check if WebXR is supported
     * @returns {Promise<boolean>} True if supported
     */
    async checkSupport() {
        if (!navigator.xr) {
            this.isSupported = false;
            return false;
        }

        try {
            this.isSupported = await navigator.xr.isSessionSupported('immersive-vr');
            return this.isSupported;
        } catch (e) {
            this.isSupported = false;
            return false;
        }
    }

    /**
     * Request an XR session
     * @param {object} options - Session options
     * @returns {Promise<XRSession>} XR session
     */
    async requestSession(options = {}) {
        if (!this.isSupported) {
            throw new Error('WebXR not supported');
        }

        this.xrSession = await navigator.xr.requestSession('immersive-vr', {
            requiredFeatures: ['local'],
            optionalFeatures: ['bounded-floor', 'hand-tracking'],
            ...options
        });

        this.xrRefSpace = await this.xrSession.requestReferenceSpace('local');

        return this.xrSession;
    }

    /**
     * End the current XR session
     */
    async endSession() {
        if (this.xrSession) {
            await this.xrSession.end();
            this.xrSession = null;
            this.xrRefSpace = null;
        }
    }
}

/**
 * Main Looking Glass manager
 */
class LookingGlassManager {
    constructor() {
        this.holoPlayService = new HoloPlayService();
        this.quiltRenderer = null;
        this.xr = new LookingGlassXR();
        this.config = LookingGlassPresets.Portrait;
        this.isHolographicMode = false;
        this.onDisplayChange = null;
    }

    /**
     * Initialize the Looking Glass integration
     * @param {HTMLCanvasElement} canvas - Canvas element for rendering
     * @param {string} preset - Preset name (Portrait, LG16, LG32, Go)
     */
    async init(canvas, preset = 'Portrait') {
        this.config = LookingGlassPresets[preset] || LookingGlassPresets.Portrait;
        this.quiltRenderer = new QuiltRenderer(this.config);
        this.quiltRenderer.init(canvas);

        // Try to connect to HoloPlay Service
        const connected = await this.holoPlayService.connect();
        if (connected) {
            console.log('Connected to HoloPlay Service');
            window.addEventListener('holoplay-displays', (e) => {
                if (this.onDisplayChange) {
                    this.onDisplayChange(e.detail.displays);
                }
            });
        } else {
            console.log('HoloPlay Service not available - running in simulation mode');
        }

        // Check WebXR support
        await this.xr.checkSupport();

        return connected;
    }

    /**
     * Set the display preset
     * @param {string} preset - Preset name
     */
    setPreset(preset) {
        const newConfig = LookingGlassPresets[preset];
        if (newConfig) {
            this.config = newConfig;
            if (this.quiltRenderer) {
                this.quiltRenderer.config = newConfig;
                this.quiltRenderer.resize();
            }
        }
    }

    /**
     * Enter holographic mode
     */
    enterHolographicMode() {
        this.isHolographicMode = true;
        if (this.quiltRenderer) {
            this.quiltRenderer.resize();
        }
    }

    /**
     * Exit holographic mode
     */
    exitHolographicMode() {
        this.isHolographicMode = false;
    }

    /**
     * Render a frame to the Looking Glass
     * @param {Camera} camera - Base camera
     * @param {Function} renderCallback - Render callback
     */
    render(camera, renderCallback) {
        if (!this.isHolographicMode || !this.quiltRenderer) {
            return;
        }

        this.quiltRenderer.renderQuilt(camera, renderCallback);

        // If connected to HoloPlay, send the quilt
        if (this.holoPlayService.connected && this.quiltRenderer.canvas) {
            this.holoPlayService.sendQuilt(this.quiltRenderer.canvas);
        }
    }

    /**
     * Get available display presets
     * @returns {Array} List of preset names
     */
    getPresets() {
        return Object.keys(LookingGlassPresets);
    }

    /**
     * Get current configuration
     * @returns {object} Current config
     */
    getConfig() {
        return { ...this.config };
    }

    /**
     * Check if connected to HoloPlay Service
     * @returns {boolean} Connection status
     */
    isConnected() {
        return this.holoPlayService.connected;
    }

    /**
     * Get connected display information
     * @returns {Array} Display info
     */
    getDisplays() {
        return this.holoPlayService.getDisplayInfo();
    }

    /**
     * Cleanup resources
     */
    destroy() {
        this.holoPlayService.disconnect();
        this.xr.endSession();
        this.quiltRenderer = null;
        this.isHolographicMode = false;
    }
}

// Export for use as module
if (typeof module !== 'undefined' && module.exports) {
    module.exports = {
        LookingGlassManager,
        LookingGlassPresets,
        QuiltRenderer,
        HoloPlayService,
        LookingGlassXR,
        Camera,
        Vec3
    };
}

// Also export as globals for browser usage
window.LookingGlass = {
    Manager: LookingGlassManager,
    Presets: LookingGlassPresets,
    QuiltRenderer: QuiltRenderer,
    HoloPlayService: HoloPlayService,
    XR: LookingGlassXR,
    Camera: Camera,
    Vec3: Vec3
};
