# Phase 4.3: Documentation

> **Goal**: Production-ready documentation for API, configuration, and deployment.

## Prerequisites

- [x] Phase 4.1 (Observability) complete
- [x] Phase 4.2 (Security) complete
- [x] All HTTP routes implemented
- [x] All MCP tools implemented
- [x] Environment variables defined

## Overview

Create comprehensive documentation for:
1. **API Reference** - HTTP endpoints, MCP tools, WebSocket protocol
2. **Configuration Guide** - Environment variables with defaults
3. **Deployment Guide** - Running, Docker, Kubernetes, security

---

## Task 1: Create API Reference Documentation

<task type="auto" priority="p1">
  <n>Create comprehensive API reference</n>
  <files>
    docs/API.md
  </files>
  <action>
    Create docs/API.md with complete API documentation:

    1. HTTP Endpoints section:
       - GET /health/live - Kubernetes liveness probe
       - GET /health/ready - Kubernetes readiness probe
       - GET /health - Backward compatible health check
       - GET /metrics - Prometheus metrics endpoint
       - GET /api/scene - Get default session scene
       - GET /api/scene/{session_id} - Get session scene
       - POST /api/scene - Update scene (add/remove/clear)
       - POST /mcp - MCP JSON-RPC endpoint
       - GET /ws - Legacy WebSocket endpoint
       - GET /ws/sync - Scene sync WebSocket
       - GET /ag-ui/stream - AG-UI SSE stream
       - POST /ag-ui/render - AG-UI render endpoint

    2. MCP Tools section (with JSON examples):
       - canvas_render - Render chart/image/video to canvas
       - canvas_interact - Report touch/voice input
       - canvas_export - Export scene to image format
       - canvas_clear - Clear all elements
       - canvas_add_element - Add single element
       - canvas_remove_element - Remove element by ID
       - canvas_update_element - Update element properties
       - canvas_get_scene - Get current scene state

    3. WebSocket Protocol section:
       - Client messages: subscribe, ping, add_element, update_element, remove_element, sync_queue
       - Server messages: welcome, pong, scene_update, element_added, element_removed, sync_result, ack, error
       - Rate limiting behavior (100 burst, 10/s sustained)
       - Error codes and retry-after hints

    4. Response formats with TypeScript interfaces

    Use clear markdown tables and code blocks.
    Include curl examples for HTTP endpoints.
  </action>
  <verify>
    - File exists at docs/API.md
    - All endpoints documented
    - All MCP tools documented
    - Examples are valid JSON
  </verify>
  <done>
    - Complete API reference in docs/API.md
    - All 8 MCP tools documented with examples
    - All HTTP routes documented with curl examples
    - WebSocket protocol fully documented
  </done>
</task>

---

## Task 2: Create Configuration Guide

<task type="auto" priority="p1">
  <n>Document all environment variables</n>
  <files>
    docs/CONFIGURATION.md
  </files>
  <action>
    Create docs/CONFIGURATION.md documenting all environment variables:

    1. Server Configuration:
       - CANVAS_PORT (default: 9473)
       - RUST_LOG (default: info,canvas_server=debug,tower_http=debug)
       - RUST_LOG_FORMAT (values: json for structured logs)

    2. Security Configuration:
       - WS_RATE_LIMIT_BURST (default: 100)
       - WS_RATE_LIMIT_SUSTAINED (default: 10)
       - CORS origins (localhost only, configurable dev ports)

    3. Communitas Integration:
       - COMMUNITAS_MCP_URL (upstream MCP server URL)
       - COMMUNITAS_MCP_TOKEN (authentication token)

    4. Example configurations:
       - Development setup
       - Production setup with JSON logging
       - Communitas-connected setup

    Include a summary table at the top with all variables.
  </action>
  <verify>
    - File exists at docs/CONFIGURATION.md
    - All env vars documented with types and defaults
    - Example configurations are complete
  </verify>
  <done>
    - Complete configuration guide in docs/CONFIGURATION.md
    - All 7 environment variables documented
    - Development and production examples included
  </done>
</task>

---

## Task 3: Create Deployment Guide

<task type="auto" priority="p1">
  <n>Create deployment guide</n>
  <files>
    docs/DEPLOYMENT.md
  </files>
  <action>
    Create docs/DEPLOYMENT.md with deployment instructions:

    1. Quick Start:
       - cargo build --release
       - Running the server
       - Browser access at localhost:9473

    2. Docker Deployment:
       - Dockerfile example (multi-stage Rust build)
       - docker-compose.yml example
       - Volume mounts for web/ directory

    3. Kubernetes Deployment:
       - Deployment manifest with resource limits
       - Service manifest
       - Health check configuration (liveness/readiness probes)
       - ConfigMap for environment variables
       - Optional: Ingress example (localhost only caveat)

    4. Security Considerations:
       - Localhost-only binding (127.0.0.1)
       - CORS restrictions
       - Rate limiting
       - Input validation
       - No sensitive data in logs

    5. Monitoring:
       - Prometheus metrics scraping
       - Key metrics to watch (requests, connections, rate limits)
       - Log aggregation with JSON format

    6. Troubleshooting:
       - Common issues and solutions
       - Debug logging configuration
       - Health check failures
  </action>
  <verify>
    - File exists at docs/DEPLOYMENT.md
    - Docker example is valid
    - Kubernetes manifests are valid YAML
    - Security section is comprehensive
  </verify>
  <done>
    - Complete deployment guide in docs/DEPLOYMENT.md
    - Docker and Kubernetes examples included
    - Security considerations documented
    - Monitoring setup documented
  </done>
</task>

---

## Exit Criteria

- [x] docs/API.md exists with complete API reference
- [x] docs/CONFIGURATION.md exists with all env vars
- [x] docs/DEPLOYMENT.md exists with Docker/K8s examples
- [x] README.md updated with links to new docs
- [x] All documentation follows consistent style

## Completion Notes (2026-01-19)

All documentation tasks completed:

1. **docs/API.md** - Comprehensive API reference including:
   - 12 HTTP endpoints with curl examples
   - 8 MCP tools with JSON parameter examples
   - Complete WebSocket protocol documentation
   - TypeScript interfaces for all types
   - Error codes reference

2. **docs/CONFIGURATION.md** - Complete configuration guide:
   - 7 environment variables documented
   - Development and production examples
   - Docker and Kubernetes config examples
   - Troubleshooting section

3. **docs/DEPLOYMENT.md** - Production deployment guide:
   - Quick start instructions
   - Multi-stage Dockerfile
   - docker-compose.yml example
   - Complete Kubernetes manifests
   - Security considerations
   - Monitoring setup
   - Troubleshooting guide

## Notes

- Documentation should be production-focused
- Include security considerations throughout
- Keep examples practical and copy-pasteable
- Reference existing docs (VISION.md, SPECS.md) where appropriate
