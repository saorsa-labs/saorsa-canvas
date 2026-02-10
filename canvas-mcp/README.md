# canvas-mcp

[MCP](https://modelcontextprotocol.io/) (Model Context Protocol) tools for [Saorsa Canvas](https://github.com/saorsa-labs/saorsa-canvas). Provides tool definitions and handlers for AI agents to create and manipulate canvas content.

## Features

- `canvas_render` — push charts, images, text to the canvas
- `canvas_interact` — report user interactions (touch, voice)
- `canvas_export` — export sessions to PNG/JPEG/SVG/PDF
- `canvas_clear` — clear all elements
- `canvas_add_element` / `canvas_remove_element` / `canvas_update_element` — low-level scene manipulation
- `canvas_get_scene` — retrieve current scene as JSON

## Installation

```toml
[dependencies]
canvas-mcp = "0.1.4"
```

## Usage

```rust
use canvas_mcp::CanvasMcpServer;

let mcp = CanvasMcpServer::new();
let tools = mcp.list_tools();
let result = mcp.call_tool("canvas_render", input).await?;
```

## License

MIT OR Apache-2.0
