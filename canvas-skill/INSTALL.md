# Saorsa Canvas — Installation & Setup

Install and configure the Saorsa Canvas server for local or remote use.

## Quick start

```bash
# Option 1: Install from crates.io
cargo install canvas-server
saorsa-canvas                    # Starts on port 9473

# Option 2: Download pre-built binary from GitHub
REPO="saorsa-labs/saorsa-canvas"
VERSION=$(curl -s "https://api.github.com/repos/$REPO/releases/latest" | grep '"tag_name"' | sed 's/.*"tag_name": "\(.*\)".*/\1/')
```

## Download from GitHub Releases

Pre-built binaries for all major platforms:

```bash
# macOS Apple Silicon (M1/M2/M3/M4)
curl -LO "https://github.com/$REPO/releases/download/$VERSION/saorsa-canvas-$VERSION-aarch64-apple-darwin.tar.gz"

# macOS Intel
curl -LO "https://github.com/$REPO/releases/download/$VERSION/saorsa-canvas-$VERSION-x86_64-apple-darwin.tar.gz"

# Linux x64
curl -LO "https://github.com/$REPO/releases/download/$VERSION/saorsa-canvas-$VERSION-x86_64-unknown-linux-gnu.tar.gz"

# Linux ARM64 (Raspberry Pi, AWS Graviton)
curl -LO "https://github.com/$REPO/releases/download/$VERSION/saorsa-canvas-$VERSION-aarch64-unknown-linux-gnu.tar.gz"

# Windows x64
curl -LO "https://github.com/$REPO/releases/download/$VERSION/saorsa-canvas-$VERSION-x86_64-pc-windows-msvc.zip"
```

Extract and run:

```bash
tar -xzf saorsa-canvas-*.tar.gz  # Unix
./saorsa-canvas                   # Start server on port 9473
```

## Available platforms

| Platform | Architecture | Archive |
|----------|-------------|---------|
| macOS | Apple Silicon (M1/M2/M3/M4) | `.tar.gz` |
| macOS | Intel x64 | `.tar.gz` |
| Linux | x64 (AMD/Intel) | `.tar.gz` |
| Linux | ARM64 (Raspberry Pi, AWS Graviton) | `.tar.gz` |
| Windows | x64 | `.zip` |

## Connecting remotely

The canvas server exposes a WebSocket endpoint for remote connections. Any MCP-capable
agent can connect to a running canvas-server instance:

```
ws://<host>:9473/ws
```

Scene deltas are synchronised as JSON WebSocket text frames. Full-scene snapshots are
sent on reconnection so clients always have consistent state.

## MCP server configuration

To use Saorsa Canvas as an MCP tool server (e.g. from Claude Desktop or another agent):

```json
{
  "mcpServers": {
    "saorsa-canvas": {
      "command": "saorsa-canvas",
      "args": ["--mcp"]
    }
  }
}
```

For a remote instance, configure the WebSocket transport instead:

```json
{
  "mcpServers": {
    "saorsa-canvas": {
      "transport": "websocket",
      "url": "ws://your-server:9473/ws"
    }
  }
}
```

## Embedded vs remote

| Mode | Setup | Use case |
|------|-------|----------|
| **Embedded** | Canvas built into the application (e.g. Fae) | Desktop apps with local GUI |
| **Local server** | `saorsa-canvas` running on localhost | Development, MCP tool access |
| **Remote server** | `saorsa-canvas` on a network host | Shared sessions, headless agents |

All three modes use the same scene graph format, tool definitions, and rendering pipeline.
The only difference is the transport layer.
