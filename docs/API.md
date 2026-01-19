# Saorsa Canvas API Reference

Complete API documentation for the Saorsa Canvas server.

## Table of Contents

- [HTTP Endpoints](#http-endpoints)
  - [Health Checks](#health-checks)
  - [Metrics](#metrics)
  - [Scene API](#scene-api)
  - [MCP Endpoint](#mcp-endpoint)
  - [AG-UI Endpoints](#ag-ui-endpoints)
- [MCP Tools](#mcp-tools)
- [WebSocket Protocol](#websocket-protocol)
- [TypeScript Interfaces](#typescript-interfaces)

---

## HTTP Endpoints

Base URL: `http://localhost:9473`

### Health Checks

#### GET /health/live

Kubernetes liveness probe. Returns 200 if the server process is running.

```bash
curl http://localhost:9473/health/live
```

**Response**: `200 OK` (empty body)

#### GET /health/ready

Kubernetes readiness probe. Checks all dependencies and returns component status.

```bash
curl http://localhost:9473/health/ready
```

**Response** (200 OK):
```json
{
  "status": "healthy",
  "version": "0.1.0",
  "checks": {
    "scene_store": true,
    "websocket": true
  }
}
```

**Response** (503 Service Unavailable):
```json
{
  "status": "unhealthy",
  "version": "0.1.0",
  "checks": {
    "scene_store": false,
    "websocket": true
  }
}
```

#### GET /health

Backward-compatible health check (alias for `/health/ready`).

---

### Metrics

#### GET /metrics

Prometheus metrics endpoint. Returns metrics in Prometheus text format.

```bash
curl http://localhost:9473/metrics
```

**Response** (200 OK):
```
# HELP canvas_http_requests_total Total HTTP requests
# TYPE canvas_http_requests_total counter
canvas_http_requests_total{method="GET",path="/api/scene",status="200"} 42

# HELP canvas_ws_connections_active Active WebSocket connections
# TYPE canvas_ws_connections_active gauge
canvas_ws_connections_active 3

# HELP canvas_rate_limited_total Rate limited requests
# TYPE canvas_rate_limited_total counter
canvas_rate_limited_total{source="websocket"} 5
```

**Available Metrics**:

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `canvas_http_requests_total` | counter | method, path, status | Total HTTP requests |
| `canvas_http_request_duration_seconds` | histogram | method, path | Request latency |
| `canvas_ws_connections_active` | gauge | - | Active WebSocket connections |
| `canvas_ws_messages_total` | counter | direction, type | WebSocket messages |
| `canvas_scene_elements_total` | gauge | - | Elements in scene |
| `canvas_mcp_tool_calls_total` | counter | tool, success | MCP tool invocations |
| `canvas_signaling_messages_total` | counter | type | WebRTC signaling messages |
| `canvas_validation_failures_total` | counter | type | Input validation failures |
| `canvas_rate_limited_total` | counter | source | Rate limited requests |

---

### Scene API

#### GET /api/scene

Get the scene for the default session.

```bash
curl http://localhost:9473/api/scene
```

**Response** (200 OK):
```json
{
  "success": true,
  "scene": {
    "session_id": "default",
    "viewport": {
      "width": 800,
      "height": 600,
      "zoom": 1.0,
      "pan_x": 0,
      "pan_y": 0
    },
    "elements": [
      {
        "id": "550e8400-e29b-41d4-a716-446655440000",
        "kind": {
          "type": "Chart",
          "chart_type": "bar",
          "data": { "labels": ["A", "B"], "values": [10, 20] }
        },
        "transform": {
          "x": 100,
          "y": 100,
          "width": 400,
          "height": 300,
          "rotation": 0,
          "z_index": 0
        }
      }
    ],
    "timestamp": 1705689600000
  }
}
```

#### GET /api/scene/{session_id}

Get the scene for a specific session.

```bash
curl http://localhost:9473/api/scene/my-session
```

**Path Parameters**:
- `session_id` - Session identifier (alphanumeric, hyphens, underscores; max 64 chars)

**Response** (200 OK): Same as `/api/scene`

**Response** (400 Bad Request):
```json
{
  "success": false,
  "error": "Invalid session_id: contains invalid characters"
}
```

#### POST /api/scene

Update the scene (add, remove, or clear elements).

```bash
curl -X POST http://localhost:9473/api/scene \
  -H "Content-Type: application/json" \
  -d '{
    "session_id": "default",
    "add": [{
      "id": "my-element",
      "kind": { "type": "Text", "content": "Hello", "font_size": 24, "color": "#000000" },
      "transform": { "x": 50, "y": 50, "width": 200, "height": 50 }
    }],
    "remove": ["old-element-id"],
    "clear": false
  }'
```

**Request Body**:
```json
{
  "session_id": "default",
  "add": [],
  "remove": [],
  "clear": false
}
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `session_id` | string | "default" | Target session |
| `add` | array | [] | Elements to add |
| `remove` | array | [] | Element IDs to remove |
| `clear` | boolean | false | Clear all elements first |

**Response**: Returns the updated scene (same format as GET).

---

### MCP Endpoint

#### POST /mcp

JSON-RPC 2.0 endpoint for MCP tool calls.

```bash
curl -X POST http://localhost:9473/mcp \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "id": 1,
    "method": "tools/call",
    "params": {
      "name": "canvas_get_scene",
      "arguments": { "session_id": "default" }
    }
  }'
```

**Supported Methods**:
- `tools/list` - List available tools
- `tools/call` - Call a tool
- `resources/list` - List available resources
- `resources/read` - Read a resource

**Response** (Success):
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": { ... }
}
```

**Response** (Error):
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "error": {
    "code": -32602,
    "message": "Invalid params"
  }
}
```

---

### AG-UI Endpoints

#### GET /ag-ui/stream

Server-Sent Events (SSE) stream for AG-UI component updates.

```bash
curl -N http://localhost:9473/ag-ui/stream
```

**Response**: SSE stream with events:
```
event: component
data: {"type":"text","content":"Hello"}

event: end
data: {}
```

#### POST /ag-ui/render

Render AG-UI components to the canvas.

```bash
curl -X POST http://localhost:9473/ag-ui/render \
  -H "Content-Type: application/json" \
  -d '{"components": [{"type": "text", "content": "Hello"}]}'
```

---

### WebSocket Endpoints

#### GET /ws

Legacy WebSocket endpoint for scene synchronization (alias for `/ws/sync`).

#### GET /ws/sync

WebSocket endpoint for real-time scene synchronization.

```javascript
const ws = new WebSocket('ws://localhost:9473/ws/sync');
```

See [WebSocket Protocol](#websocket-protocol) for message formats.

---

## MCP Tools

### canvas_render

Render content (chart, image, text, 3D model) to the canvas.

**Parameters**:
```json
{
  "session_id": "default",
  "content": {
    "type": "Chart",
    "data": {
      "chart_type": "bar",
      "data": { "labels": ["Jan", "Feb"], "values": [10, 20] }
    }
  },
  "position": { "x": 100, "y": 100 }
}
```

**Content Types**:

| Type | Required Fields | Optional Fields |
|------|-----------------|-----------------|
| Chart | chart_type, data | - |
| Image | src | - |
| Text | content | font_size |
| Model3D | src | rotation |

**Chart Types**: `bar`, `line`, `pie`, `area`, `scatter`

---

### canvas_interact

Report user interaction on the canvas.

**Parameters**:
```json
{
  "session_id": "default",
  "interaction": {
    "type": "touch",
    "x": 150,
    "y": 200,
    "element_id": "chart-1"
  },
  "voice": "Make this one red"
}
```

---

### canvas_export

Export the canvas to an image format.

**Parameters**:
```json
{
  "session_id": "default",
  "format": "png",
  "width": 1920,
  "height": 1080
}
```

**Formats**: `png`, `jpeg`, `svg`, `pdf`

---

### canvas_clear

Clear all elements from the canvas.

**Parameters**:
```json
{
  "session_id": "default"
}
```

---

### canvas_add_element

Add an element with full control over type, transform, and properties.

**Parameters**:
```json
{
  "session_id": "default",
  "element_type": "chart",
  "transform": {
    "x": 100,
    "y": 100,
    "width": 400,
    "height": 300,
    "rotation": 0,
    "z_index": 1
  },
  "properties": {
    "chart_type": "bar",
    "data": { "labels": ["A", "B"], "values": [10, 20] }
  }
}
```

**Element Types**: `chart`, `image`, `text`, `model3d`, `video`

---

### canvas_remove_element

Remove an element by ID.

**Parameters**:
```json
{
  "session_id": "default",
  "element_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

---

### canvas_update_element

Update an existing element's transform or properties.

**Parameters**:
```json
{
  "session_id": "default",
  "element_id": "550e8400-e29b-41d4-a716-446655440000",
  "transform": {
    "x": 200,
    "y": 150
  },
  "properties": {
    "color": "#ff0000"
  }
}
```

---

### canvas_get_scene

Get the current scene state as JSON.

**Parameters**:
```json
{
  "session_id": "default"
}
```

**Response**:
```json
{
  "content": [{
    "type": "text",
    "text": "{\"session_id\":\"default\",\"viewport\":{...},\"elements\":[...]}"
  }]
}
```

---

## WebSocket Protocol

### Connection

```javascript
const ws = new WebSocket('ws://localhost:9473/ws/sync');

ws.onopen = () => {
  ws.send(JSON.stringify({ type: 'subscribe', session_id: 'default' }));
};
```

### Rate Limiting

- **Burst**: 100 messages (configurable via `WS_RATE_LIMIT_BURST`)
- **Sustained**: 10 messages/second (configurable via `WS_RATE_LIMIT_SUSTAINED`)

When rate limited, you'll receive:
```json
{
  "type": "error",
  "code": "rate_limited",
  "message": "Rate limit exceeded. Retry after 100ms"
}
```

### Client Messages

#### subscribe
```json
{ "type": "subscribe", "session_id": "default" }
```

#### ping
```json
{ "type": "ping" }
```

#### add_element
```json
{
  "type": "add_element",
  "element": {
    "id": "my-id",
    "kind": { "type": "Text", "content": "Hello", "font_size": 16, "color": "#000" },
    "transform": { "x": 0, "y": 0, "width": 100, "height": 50 }
  },
  "message_id": "msg-123"
}
```

#### update_element
```json
{
  "type": "update_element",
  "id": "element-id",
  "changes": { "transform": { "x": 200 } },
  "message_id": "msg-124"
}
```

#### remove_element
```json
{
  "type": "remove_element",
  "id": "element-id",
  "message_id": "msg-125"
}
```

#### sync_queue
```json
{
  "type": "sync_queue",
  "operations": [
    { "type": "add", "element": {...} },
    { "type": "remove", "element_id": "..." }
  ]
}
```

#### get_scene
```json
{ "type": "get_scene" }
```

### Server Messages

#### welcome
```json
{
  "type": "welcome",
  "version": "0.1.0",
  "session_id": "default",
  "peer_id": "peer-abc123"
}
```

#### pong
```json
{ "type": "pong" }
```

#### scene_update
```json
{
  "type": "scene_update",
  "scene": {
    "session_id": "default",
    "viewport": {...},
    "elements": [...],
    "timestamp": 1705689600000
  }
}
```

#### element_added
```json
{
  "type": "element_added",
  "element": {
    "id": "new-id",
    "kind": {...},
    "transform": {...}
  }
}
```

#### element_removed
```json
{
  "type": "element_removed",
  "id": "removed-element-id"
}
```

#### ack
```json
{
  "type": "ack",
  "message_id": "msg-123"
}
```

#### sync_result
```json
{
  "type": "sync_result",
  "synced": 5,
  "failed": 0
}
```

#### error
```json
{
  "type": "error",
  "code": "invalid_session",
  "message": "Session not found",
  "message_id": "msg-123"
}
```

### WebRTC Signaling

The WebSocket also handles WebRTC signaling for peer-to-peer video.

#### Client -> Server

```json
{ "type": "start_call", "target_peer_id": "peer-xyz", "session_id": "default" }
{ "type": "offer", "target_peer_id": "peer-xyz", "sdp": "v=0..." }
{ "type": "answer", "target_peer_id": "peer-xyz", "sdp": "v=0..." }
{ "type": "ice_candidate", "target_peer_id": "peer-xyz", "candidate": "..." }
{ "type": "end_call", "target_peer_id": "peer-xyz" }
```

#### Server -> Client

```json
{ "type": "peer_assigned", "peer_id": "peer-abc123" }
{ "type": "incoming_call", "from_peer_id": "peer-xyz", "session_id": "default" }
{ "type": "relay_offer", "from_peer_id": "peer-xyz", "sdp": "v=0..." }
{ "type": "relay_answer", "from_peer_id": "peer-xyz", "sdp": "v=0..." }
{ "type": "relay_ice_candidate", "from_peer_id": "peer-xyz", "candidate": "..." }
{ "type": "call_ended", "from_peer_id": "peer-xyz", "reason": "hangup" }
```

---

## TypeScript Interfaces

```typescript
// Scene Document
interface SceneDocument {
  session_id: string;
  viewport: Viewport;
  elements: ElementDocument[];
  timestamp: number;
}

interface Viewport {
  width: number;
  height: number;
  zoom: number;
  pan_x: number;
  pan_y: number;
}

// Element Document
interface ElementDocument {
  id: string;
  kind: ElementKind;
  transform: Transform;
}

interface Transform {
  x: number;
  y: number;
  width: number;
  height: number;
  rotation: number;
  z_index: number;
}

type ElementKind =
  | { type: 'Chart'; chart_type: string; data: object }
  | { type: 'Image'; src: string; format: string }
  | { type: 'Text'; content: string; font_size: number; color: string }
  | { type: 'Model3D'; src: string; rotation: [number, number, number]; scale: number }
  | { type: 'Video'; stream_id: string; media_config?: MediaConfig };

// Health Status
interface HealthStatus {
  status: 'healthy' | 'unhealthy';
  version: string;
  checks: {
    scene_store: boolean;
    websocket: boolean;
  };
}

// Scene Response
interface SceneResponse {
  success: boolean;
  scene?: SceneDocument;
  error?: string;
}

// JSON-RPC
interface JsonRpcRequest {
  jsonrpc: '2.0';
  id: number | string;
  method: string;
  params?: object;
}

interface JsonRpcResponse {
  jsonrpc: '2.0';
  id: number | string;
  result?: object;
  error?: {
    code: number;
    message: string;
    data?: object;
  };
}

// WebSocket Messages
type ClientMessage =
  | { type: 'subscribe'; session_id: string }
  | { type: 'ping' }
  | { type: 'add_element'; element: ElementDocument; message_id?: string }
  | { type: 'update_element'; id: string; changes: object; message_id?: string }
  | { type: 'remove_element'; id: string; message_id?: string }
  | { type: 'sync_queue'; operations: QueuedOperation[] }
  | { type: 'get_scene' };

type ServerMessage =
  | { type: 'welcome'; version: string; session_id: string; peer_id: string }
  | { type: 'pong' }
  | { type: 'scene_update'; scene: SceneDocument }
  | { type: 'element_added'; element: ElementDocument }
  | { type: 'element_removed'; id: string }
  | { type: 'ack'; message_id: string }
  | { type: 'sync_result'; synced: number; failed: number }
  | { type: 'error'; code: string; message: string; message_id?: string };
```

---

## Error Codes

### HTTP Status Codes

| Code | Description |
|------|-------------|
| 200 | Success |
| 400 | Bad Request (validation error) |
| 404 | Not Found |
| 429 | Too Many Requests (rate limited) |
| 500 | Internal Server Error |
| 503 | Service Unavailable (health check failed) |

### WebSocket Error Codes

| Code | Description |
|------|-------------|
| `invalid_session` | Session ID not found or invalid |
| `invalid_element` | Element ID not found or invalid |
| `rate_limited` | Too many messages, retry after delay |
| `validation_error` | Message failed validation |
| `internal_error` | Server-side error |

### JSON-RPC Error Codes

| Code | Description |
|------|-------------|
| -32700 | Parse error |
| -32600 | Invalid request |
| -32601 | Method not found |
| -32602 | Invalid params |
| -32603 | Internal error |
