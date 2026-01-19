# Saorsa Canvas Configuration Guide

Complete guide to configuring the Saorsa Canvas server via environment variables.

## Quick Reference

| Variable | Default | Description |
|----------|---------|-------------|
| `CANVAS_PORT` | 9473 | Server port |
| `RUST_LOG` | info,canvas_server=debug,tower_http=debug | Log levels |
| `RUST_LOG_FORMAT` | text | Log format (text/json) |
| `WS_RATE_LIMIT_BURST` | 100 | WebSocket burst limit |
| `WS_RATE_LIMIT_SUSTAINED` | 10 | WebSocket sustained rate/sec |
| `COMMUNITAS_MCP_URL` | - | Upstream MCP server URL |
| `COMMUNITAS_MCP_TOKEN` | - | Upstream auth token |

---

## Server Configuration

### CANVAS_PORT

The TCP port the server listens on.

| Property | Value |
|----------|-------|
| Type | Integer |
| Default | 9473 |
| Range | 1-65535 |

```bash
# Use port 8080
export CANVAS_PORT=8080
```

**Note**: Port 9473 spells "SAOR" on a phone keypad.

---

### RUST_LOG

Controls logging verbosity using the [env_logger](https://docs.rs/env_logger) format.

| Property | Value |
|----------|-------|
| Type | String |
| Default | `info,canvas_server=debug,tower_http=debug` |

**Format**: `target=level,target=level,...`

**Levels** (from most to least verbose):
- `trace` - Very detailed debugging
- `debug` - Debug information
- `info` - General information
- `warn` - Warnings
- `error` - Errors only

```bash
# Minimal logging (errors only)
export RUST_LOG=error

# Verbose debugging
export RUST_LOG=debug

# Target-specific levels
export RUST_LOG=warn,canvas_server=debug,tower_http=trace

# Silence tower_http but keep canvas_server verbose
export RUST_LOG=info,canvas_server=debug,tower_http=warn
```

---

### RUST_LOG_FORMAT

Controls log output format.

| Property | Value |
|----------|-------|
| Type | String |
| Default | text |
| Values | `text`, `json` |

**Text format** (default) - Human-readable:
```
2024-01-19T10:30:00.123Z  INFO canvas_server::main: Server starting on http://127.0.0.1:9473
```

**JSON format** - Machine-parseable (recommended for production):
```json
{"timestamp":"2024-01-19T10:30:00.123Z","level":"INFO","target":"canvas_server::main","message":"Server starting on http://127.0.0.1:9473"}
```

```bash
# Enable JSON logging for production
export RUST_LOG_FORMAT=json
```

---

## Security Configuration

### WS_RATE_LIMIT_BURST

Maximum number of WebSocket messages allowed in a burst before rate limiting.

| Property | Value |
|----------|-------|
| Type | Integer |
| Default | 100 |
| Minimum | 1 |

The rate limiter uses a token bucket algorithm:
- Starts with `burst` tokens
- Each message consumes 1 token
- Tokens refill at `sustained` rate per second

```bash
# Allow larger bursts for high-traffic scenarios
export WS_RATE_LIMIT_BURST=200
```

---

### WS_RATE_LIMIT_SUSTAINED

Sustained message rate (messages per second) for WebSocket connections.

| Property | Value |
|----------|-------|
| Type | Integer |
| Default | 10 |
| Minimum | 1 |

```bash
# Higher sustained rate for interactive applications
export WS_RATE_LIMIT_SUSTAINED=20
```

**Rate Limiting Behavior**:

When a client exceeds the rate limit:
1. Server returns an error with `code: "rate_limited"`
2. Error includes `retry_after` hint in milliseconds
3. Subsequent messages are dropped until tokens refill

**Example error**:
```json
{
  "type": "error",
  "code": "rate_limited",
  "message": "Rate limit exceeded. Retry after 100ms"
}
```

---

### CORS Origins

CORS is restricted to localhost origins only for security. The following origins are allowed by default:

| Origin | Purpose |
|--------|---------|
| `http://localhost:{CANVAS_PORT}` | Server's own port |
| `http://127.0.0.1:{CANVAS_PORT}` | Server's own port (IP) |
| `http://localhost:3000` | Create React App |
| `http://127.0.0.1:3000` | Create React App (IP) |
| `http://localhost:5173` | Vite |
| `http://127.0.0.1:5173` | Vite (IP) |
| `http://localhost:8080` | Generic dev server |
| `http://127.0.0.1:8080` | Generic dev server (IP) |

**Note**: CORS origins are not configurable via environment variables. The server is designed to run locally only.

---

## Communitas Integration

Connect to an upstream Communitas MCP server for scene synchronization.

### COMMUNITAS_MCP_URL

URL of the upstream Communitas MCP server.

| Property | Value |
|----------|-------|
| Type | String (URL) |
| Default | - (disabled) |

```bash
# Connect to local Communitas
export COMMUNITAS_MCP_URL=http://localhost:8080/mcp

# Connect to remote Communitas
export COMMUNITAS_MCP_URL=https://communitas.example.com/mcp
```

When set, the server will:
1. Initialize connection on startup
2. Fetch the initial scene from Communitas
3. Push local scene changes upstream
4. Pull remote changes periodically

---

### COMMUNITAS_MCP_TOKEN

Authentication token for the Communitas MCP server.

| Property | Value |
|----------|-------|
| Type | String |
| Default | - (no authentication) |

```bash
# Set authentication token
export COMMUNITAS_MCP_TOKEN=your-secret-token
```

**Security Note**: Store tokens securely. Avoid:
- Hardcoding in scripts
- Committing to version control
- Logging or printing

Use secrets management:
```bash
# From file
export COMMUNITAS_MCP_TOKEN=$(cat /run/secrets/communitas-token)

# From environment (already set securely)
# COMMUNITAS_MCP_TOKEN is injected by orchestrator
```

---

## Example Configurations

### Development (Default)

Minimal configuration for local development:

```bash
# No environment variables needed - defaults work well
cargo run -p canvas-server
```

Effective configuration:
- Port: 9473
- Logging: Debug level, text format
- Rate limiting: 100 burst, 10/sec sustained
- Communitas: Disabled

---

### Development with Verbose Logging

```bash
export RUST_LOG=trace,hyper=warn,mio=warn
cargo run -p canvas-server
```

---

### Production

Recommended production configuration:

```bash
# Structured logging for log aggregation
export RUST_LOG=info,canvas_server=info
export RUST_LOG_FORMAT=json

# Conservative rate limits
export WS_RATE_LIMIT_BURST=50
export WS_RATE_LIMIT_SUSTAINED=5

# Run server
./canvas-server
```

---

### Production with Communitas

Full production setup with upstream synchronization:

```bash
# Logging
export RUST_LOG=info
export RUST_LOG_FORMAT=json

# Communitas connection
export COMMUNITAS_MCP_URL=https://communitas.internal/mcp
export COMMUNITAS_MCP_TOKEN=$(cat /run/secrets/communitas-token)

# Rate limiting
export WS_RATE_LIMIT_BURST=100
export WS_RATE_LIMIT_SUSTAINED=10

# Run server
./canvas-server
```

---

### Docker Environment File

Create `.env` for Docker Compose:

```env
# .env
CANVAS_PORT=9473
RUST_LOG=info,canvas_server=info
RUST_LOG_FORMAT=json
WS_RATE_LIMIT_BURST=100
WS_RATE_LIMIT_SUSTAINED=10
```

Use with docker-compose:
```yaml
services:
  canvas:
    image: saorsa-canvas
    env_file: .env
```

---

### Kubernetes ConfigMap

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: canvas-config
data:
  RUST_LOG: "info,canvas_server=info"
  RUST_LOG_FORMAT: "json"
  WS_RATE_LIMIT_BURST: "100"
  WS_RATE_LIMIT_SUSTAINED: "10"
```

With Secret for sensitive values:
```yaml
apiVersion: v1
kind: Secret
metadata:
  name: canvas-secrets
type: Opaque
stringData:
  COMMUNITAS_MCP_TOKEN: "your-secret-token"
```

---

## Validation

### Session IDs

Session IDs are validated with the following rules:
- **Characters**: Alphanumeric, hyphens (`-`), underscores (`_`)
- **Length**: 1-64 characters
- **Forbidden**: Path traversal (`..`), spaces, special characters

Valid examples:
- `default`
- `my-session`
- `session_123`
- `ABC-xyz_456`

Invalid examples:
- `../etc/passwd` (path traversal)
- `my session` (spaces)
- `<script>` (special characters)
- `` (empty)

---

### Element IDs

Element IDs follow the same validation as session IDs, but are typically UUIDs:
- `550e8400-e29b-41d4-a716-446655440000`
- `my-custom-id`

---

### Peer IDs

Peer IDs (for WebRTC) are auto-generated UUIDs:
- Format: `peer-{uuid}`
- Example: `peer-550e8400-e29b-41d4-a716-446655440000`

---

## Troubleshooting

### Server won't start

**Port already in use**:
```bash
# Check what's using the port
lsof -i :9473

# Use a different port
export CANVAS_PORT=9474
```

**Permission denied on port < 1024**:
```bash
# Use a port >= 1024 (recommended)
export CANVAS_PORT=9473

# Or run with elevated privileges (not recommended)
sudo ./canvas-server
```

### Logs are too verbose

```bash
# Reduce to warnings only
export RUST_LOG=warn
```

### Logs are not JSON

```bash
# Verify format is set correctly
export RUST_LOG_FORMAT=json
echo $RUST_LOG_FORMAT  # Should print "json"
```

### Communitas connection fails

1. Verify URL is correct:
   ```bash
   curl $COMMUNITAS_MCP_URL
   ```

2. Check token is set:
   ```bash
   echo ${COMMUNITAS_MCP_TOKEN:+Token is set}
   ```

3. Check server logs for connection errors:
   ```bash
   export RUST_LOG=debug,canvas_server=trace
   ```

### Rate limiting too aggressive

```bash
# Increase limits for testing
export WS_RATE_LIMIT_BURST=1000
export WS_RATE_LIMIT_SUSTAINED=100
```

**Warning**: Don't use high limits in production - they protect against denial of service.
