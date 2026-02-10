# canvas-server

HTTP/WebSocket server for [Saorsa Canvas](https://github.com/saorsa-labs/saorsa-canvas) — serves the canvas PWA, handles real-time sync, MCP tool requests, and scene export.

## Features

- Axum-based HTTP server with WebSocket sync
- Real-time collaborative editing via CRDT-like sync protocol
- MCP tool endpoint (`POST /mcp`)
- Scene export endpoint (`POST /api/export`) — PNG, JPEG, SVG, PDF
- Session persistence to disk
- Health and metrics endpoints

## Installation

```toml
[dependencies]
canvas-server = "0.1.4"
```

Or run the standalone binary:

```bash
cargo install canvas-server
saorsa-canvas --port 9473
```

## Endpoints

| Method | Path | Description |
|--------|------|-------------|
| GET | `/health` | Health check |
| GET | `/ws/sync` | WebSocket sync |
| POST | `/mcp` | MCP tool calls |
| POST | `/api/export` | Scene export |

## License

MIT OR Apache-2.0
