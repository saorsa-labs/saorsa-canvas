# Changelog

All notable changes to Saorsa Canvas will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.2] - 2026-01-22

### Fixed
- Fixed clippy warnings in canvas-renderer spatial module (format string inlining)
- Fixed clippy warnings in canvas-renderer video module (use sort_unstable for primitives)
- Fixed clippy warnings in canvas-renderer holographic tests (len_zero to is_empty)
- Fixed canvas-app WASM tests to use proper DOM-based test helper
- Fixed canvas-server agui_integration test (while_let_loop)
- Fixed documentation warning in canvas-app render_quilt function

### Changed
- Updated test assertions to avoid float comparison warnings
- Improved test structure in canvas-app with create_test_app helper
- Updated get_holographic_preset tests to match actual String return type

## [0.1.1] - 2026-01-20

### Added
- Voice input WASM bindings for touch+voice fusion
- VoiceEvent struct for voice command processing
- Comprehensive voice integration tests

### Fixed
- Various test reliability improvements

## [0.1.0] - 2026-01-20

### Added
- Initial release of Saorsa Canvas
- Core scene graph with elements and transforms
- WebSocket-based real-time sync
- MCP server integration for AI tools
- WASM bindings for web deployment
- Holographic rendering support for Looking Glass displays
- Offline mode with conflict resolution
- AG-UI interaction streaming
- A2UI component tree rendering
- Video texture pipeline
- GPU-accelerated quilt composition
