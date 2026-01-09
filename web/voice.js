/**
 * Saorsa Canvas Voice Input
 *
 * Provides speech recognition integration for voice-controlled canvas interactions.
 * Uses the Web Speech API with fallback for unsupported browsers.
 */

/**
 * Voice recognition result.
 * @typedef {Object} VoiceResult
 * @property {string} transcript - The recognized speech text
 * @property {number} confidence - Confidence score (0.0 to 1.0)
 * @property {boolean} isFinal - Whether this is a final result
 * @property {number} timestamp - Timestamp in milliseconds
 */

/**
 * Voice input configuration.
 * @typedef {Object} VoiceConfig
 * @property {string} language - Recognition language (default: 'en-US')
 * @property {boolean} continuous - Whether to continuously recognize (default: true)
 * @property {boolean} interimResults - Whether to return interim results (default: true)
 * @property {number} fusionWindowMs - Time window to fuse with touch events (default: 2000)
 */

/**
 * Voice input handler for speech recognition.
 */
class VoiceInput {
    /**
     * Create a new voice input handler.
     * @param {VoiceConfig} config - Configuration options
     */
    constructor(config = {}) {
        this.config = {
            language: config.language || 'en-US',
            continuous: config.continuous !== false,
            interimResults: config.interimResults !== false,
            fusionWindowMs: config.fusionWindowMs || 2000,
            ...config
        };

        this.recognition = null;
        this.isListening = false;
        this.isSupported = false;

        // Event callbacks
        this.onResult = null;
        this.onError = null;
        this.onStart = null;
        this.onEnd = null;
        this.onInterim = null;

        // Touch fusion state
        this.lastTouch = null;
        this.lastTouchTimestamp = 0;

        this._initRecognition();
    }

    /**
     * Initialize the speech recognition API.
     * @private
     */
    _initRecognition() {
        // Check for browser support
        const SpeechRecognition = window.SpeechRecognition || window.webkitSpeechRecognition;

        if (!SpeechRecognition) {
            console.warn('Speech recognition not supported in this browser');
            this.isSupported = false;
            return;
        }

        this.isSupported = true;
        this.recognition = new SpeechRecognition();

        // Configure recognition
        this.recognition.continuous = this.config.continuous;
        this.recognition.interimResults = this.config.interimResults;
        this.recognition.lang = this.config.language;
        this.recognition.maxAlternatives = 1;

        // Set up event handlers
        this.recognition.onresult = (event) => this._handleResult(event);
        this.recognition.onerror = (event) => this._handleError(event);
        this.recognition.onstart = () => this._handleStart();
        this.recognition.onend = () => this._handleEnd();
        this.recognition.onspeechstart = () => this._handleSpeechStart();
        this.recognition.onspeechend = () => this._handleSpeechEnd();
    }

    /**
     * Handle speech recognition results.
     * @private
     * @param {SpeechRecognitionEvent} event - The recognition event
     */
    _handleResult(event) {
        const result = event.results[event.results.length - 1];
        const alternative = result[0];

        const voiceResult = {
            transcript: alternative.transcript,
            confidence: alternative.confidence,
            isFinal: result.isFinal,
            timestamp: Date.now()
        };

        // Check for touch fusion
        if (result.isFinal && this.lastTouch) {
            const elapsed = voiceResult.timestamp - this.lastTouchTimestamp;
            if (elapsed < this.config.fusionWindowMs) {
                voiceResult.fusedTouch = this.lastTouch;
                this.lastTouch = null;
            }
        }

        if (result.isFinal) {
            if (this.onResult) {
                this.onResult(voiceResult);
            }
        } else {
            if (this.onInterim) {
                this.onInterim(voiceResult);
            }
        }
    }

    /**
     * Handle speech recognition errors.
     * @private
     * @param {SpeechRecognitionErrorEvent} event - The error event
     */
    _handleError(event) {
        const errorInfo = {
            error: event.error,
            message: this._getErrorMessage(event.error),
            timestamp: Date.now()
        };

        console.error('Speech recognition error:', errorInfo);

        if (this.onError) {
            this.onError(errorInfo);
        }

        // Auto-restart on recoverable errors
        if (event.error === 'network' || event.error === 'aborted') {
            if (this.isListening) {
                setTimeout(() => this.start(), 1000);
            }
        }
    }

    /**
     * Get a human-readable error message.
     * @private
     * @param {string} errorCode - The error code
     * @returns {string} Human-readable error message
     */
    _getErrorMessage(errorCode) {
        const messages = {
            'no-speech': 'No speech detected',
            'audio-capture': 'Microphone not available',
            'not-allowed': 'Microphone permission denied',
            'network': 'Network error occurred',
            'aborted': 'Recognition aborted',
            'language-not-supported': 'Language not supported',
            'service-not-allowed': 'Service not allowed'
        };
        return messages[errorCode] || `Unknown error: ${errorCode}`;
    }

    /**
     * Handle recognition start.
     * @private
     */
    _handleStart() {
        this.isListening = true;
        if (this.onStart) {
            this.onStart();
        }
    }

    /**
     * Handle recognition end.
     * @private
     */
    _handleEnd() {
        this.isListening = false;
        if (this.onEnd) {
            this.onEnd();
        }

        // Auto-restart if continuous mode and not intentionally stopped
        if (this.config.continuous && this._shouldRestart) {
            setTimeout(() => this.start(), 100);
        }
    }

    /**
     * Handle speech start detection.
     * @private
     */
    _handleSpeechStart() {
        // Could emit event for UI feedback
    }

    /**
     * Handle speech end detection.
     * @private
     */
    _handleSpeechEnd() {
        // Could emit event for UI feedback
    }

    /**
     * Start speech recognition.
     * @returns {boolean} Whether recognition was started
     */
    start() {
        if (!this.isSupported) {
            console.warn('Speech recognition not supported');
            return false;
        }

        if (this.isListening) {
            return true;
        }

        try {
            this._shouldRestart = true;
            this.recognition.start();
            return true;
        } catch (error) {
            console.error('Failed to start speech recognition:', error);
            return false;
        }
    }

    /**
     * Stop speech recognition.
     */
    stop() {
        if (!this.isSupported || !this.isListening) {
            return;
        }

        this._shouldRestart = false;
        this.recognition.stop();
    }

    /**
     * Abort speech recognition immediately.
     */
    abort() {
        if (!this.isSupported) {
            return;
        }

        this._shouldRestart = false;
        this.recognition.abort();
    }

    /**
     * Register a touch event for potential fusion with voice.
     * @param {Object} touchInfo - Touch event information
     * @param {number} touchInfo.x - X coordinate
     * @param {number} touchInfo.y - Y coordinate
     * @param {string} [touchInfo.elementId] - Target element ID
     */
    registerTouch(touchInfo) {
        this.lastTouch = touchInfo;
        this.lastTouchTimestamp = Date.now();
    }

    /**
     * Clear any pending touch for fusion.
     */
    clearTouch() {
        this.lastTouch = null;
    }

    /**
     * Check if a touch is pending for fusion.
     * @returns {boolean} Whether a touch is pending
     */
    hasPendingTouch() {
        if (!this.lastTouch) {
            return false;
        }
        const elapsed = Date.now() - this.lastTouchTimestamp;
        return elapsed < this.config.fusionWindowMs;
    }

    /**
     * Set the recognition language.
     * @param {string} language - Language code (e.g., 'en-US', 'es-ES')
     */
    setLanguage(language) {
        this.config.language = language;
        if (this.recognition) {
            this.recognition.lang = language;
        }
    }

    /**
     * Get supported languages (best effort).
     * @returns {string[]} Array of common language codes
     */
    static getSupportedLanguages() {
        return [
            'en-US', 'en-GB', 'en-AU',
            'es-ES', 'es-MX',
            'fr-FR', 'fr-CA',
            'de-DE',
            'it-IT',
            'pt-BR', 'pt-PT',
            'ja-JP',
            'ko-KR',
            'zh-CN', 'zh-TW',
            'ru-RU',
            'ar-SA'
        ];
    }
}

/**
 * Voice command parser for common canvas operations.
 */
class VoiceCommands {
    constructor() {
        this.commands = new Map();
        this._registerDefaultCommands();
    }

    /**
     * Register default voice commands.
     * @private
     */
    _registerDefaultCommands() {
        // Color commands
        this.register(/make (?:this|it) (red|blue|green|yellow|orange|purple|black|white)/i,
            (match) => ({ action: 'setColor', color: match[1].toLowerCase() }));

        // Size commands
        this.register(/make (?:this|it) (bigger|smaller|larger)/i,
            (match) => ({ action: 'resize', direction: match[1].toLowerCase() === 'smaller' ? 'down' : 'up' }));

        // Delete commands
        this.register(/(?:delete|remove) (?:this|it|that)/i,
            () => ({ action: 'delete' }));

        // Move commands
        this.register(/move (?:this|it) (?:to the )?(left|right|up|down)/i,
            (match) => ({ action: 'move', direction: match[1].toLowerCase() }));

        // Undo/redo
        this.register(/undo/i, () => ({ action: 'undo' }));
        this.register(/redo/i, () => ({ action: 'redo' }));

        // Select commands
        this.register(/select (?:all|everything)/i, () => ({ action: 'selectAll' }));
        this.register(/(?:deselect|unselect) (?:all|everything)/i, () => ({ action: 'deselectAll' }));

        // Add element commands
        this.register(/add (?:a )?(?:new )?(text|image|chart|video)/i,
            (match) => ({ action: 'add', elementType: match[1].toLowerCase() }));

        // Zoom commands
        this.register(/zoom (in|out)/i,
            (match) => ({ action: 'zoom', direction: match[1].toLowerCase() }));

        // Save command
        this.register(/save/i, () => ({ action: 'save' }));
    }

    /**
     * Register a voice command.
     * @param {RegExp} pattern - Pattern to match
     * @param {function} handler - Handler that returns command object
     */
    register(pattern, handler) {
        this.commands.set(pattern, handler);
    }

    /**
     * Parse a transcript for commands.
     * @param {string} transcript - The speech transcript
     * @returns {Object|null} Parsed command or null if no match
     */
    parse(transcript) {
        for (const [pattern, handler] of this.commands) {
            const match = transcript.match(pattern);
            if (match) {
                return handler(match);
            }
        }
        return null;
    }
}

/**
 * Fused input combining touch and voice.
 * @typedef {Object} FusedIntent
 * @property {string} transcript - Voice transcript
 * @property {Object} touch - Touch location info
 * @property {number} touch.x - X coordinate
 * @property {number} touch.y - Y coordinate
 * @property {string} [touch.elementId] - Target element ID
 * @property {Object} [command] - Parsed command if recognized
 */

/**
 * Voice input manager that integrates with the canvas.
 */
class VoiceManager {
    /**
     * Create a voice manager.
     * @param {Object} options - Configuration options
     */
    constructor(options = {}) {
        this.voice = new VoiceInput(options);
        this.commands = new VoiceCommands();

        // Callbacks
        this.onCommand = null;
        this.onFusedIntent = null;
        this.onTranscript = null;
        this.onStatusChange = null;

        this._setupHandlers();
    }

    /**
     * Set up voice input handlers.
     * @private
     */
    _setupHandlers() {
        this.voice.onResult = (result) => {
            // Try to parse as command
            const command = this.commands.parse(result.transcript);

            if (result.fusedTouch) {
                // Fused intent (touch + voice)
                const intent = {
                    transcript: result.transcript,
                    touch: result.fusedTouch,
                    command,
                    timestamp: result.timestamp
                };

                if (this.onFusedIntent) {
                    this.onFusedIntent(intent);
                }
            } else if (command) {
                // Command without touch
                if (this.onCommand) {
                    this.onCommand(command, result.transcript);
                }
            }

            // Always notify of transcript
            if (this.onTranscript) {
                this.onTranscript(result);
            }
        };

        this.voice.onInterim = (result) => {
            // Could show interim results in UI
        };

        this.voice.onStart = () => {
            if (this.onStatusChange) {
                this.onStatusChange('listening');
            }
        };

        this.voice.onEnd = () => {
            if (this.onStatusChange) {
                this.onStatusChange('stopped');
            }
        };

        this.voice.onError = (error) => {
            if (this.onStatusChange) {
                this.onStatusChange('error', error);
            }
        };
    }

    /**
     * Start listening for voice commands.
     * @returns {boolean} Whether listening started
     */
    start() {
        return this.voice.start();
    }

    /**
     * Stop listening.
     */
    stop() {
        this.voice.stop();
    }

    /**
     * Toggle listening state.
     * @returns {boolean} New listening state
     */
    toggle() {
        if (this.voice.isListening) {
            this.stop();
            return false;
        } else {
            return this.start();
        }
    }

    /**
     * Register a touch for fusion.
     * @param {Object} touchInfo - Touch information
     */
    registerTouch(touchInfo) {
        this.voice.registerTouch(touchInfo);
    }

    /**
     * Check if voice is supported.
     * @returns {boolean} Whether voice is supported
     */
    get isSupported() {
        return this.voice.isSupported;
    }

    /**
     * Check if currently listening.
     * @returns {boolean} Whether currently listening
     */
    get isListening() {
        return this.voice.isListening;
    }

    /**
     * Register a custom command.
     * @param {RegExp} pattern - Pattern to match
     * @param {function} handler - Handler function
     */
    registerCommand(pattern, handler) {
        this.commands.register(pattern, handler);
    }
}

// Export for use
window.VoiceInput = VoiceInput;
window.VoiceCommands = VoiceCommands;
window.VoiceManager = VoiceManager;
