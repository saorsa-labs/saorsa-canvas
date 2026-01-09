# Saorsa Canvas Skill

Display visual content (charts, images, 3D models) through a universal canvas that works on any device.

## Trigger

Invoke when the user asks to:
- Display a chart, graph, or visualization
- Show an image or diagram
- Render a 3D model
- Create a visual presentation
- Start a video call or screen share

## Usage

### Display a Chart

```bash
# Start the canvas server
saorsa-canvas &

# Server will output: "Open http://localhost:9473 in your browser"
```

Then use the MCP tools:
- `canvas_render` - Render content to the canvas
- `canvas_interact` - Handle touch/voice input
- `canvas_export` - Export canvas to image/PDF

### Example: Bar Chart

```json
{
  "tool": "canvas_render",
  "params": {
    "session_id": "default",
    "content": {
      "type": "Chart",
      "data": {
        "chart_type": "bar",
        "data": {
          "labels": ["Jan", "Feb", "Mar"],
          "values": [10, 20, 15]
        },
        "title": "Monthly Sales"
      }
    }
  }
}
```

### Example: Image

```json
{
  "tool": "canvas_render",
  "params": {
    "session_id": "default",
    "content": {
      "type": "Image",
      "data": {
        "src": "https://example.com/diagram.png",
        "alt": "System architecture diagram"
      }
    }
  }
}
```

## Touch + Voice Interaction

When the user touches the canvas while speaking:

1. Canvas captures touch coordinates and element ID
2. Voice transcript is captured
3. Both are sent to the AI via `canvas_interact`
4. AI interprets "change THIS part" with spatial context

Example interaction flow:
```
User: [touches chart bar] "Make this one red"
Canvas → AI: {touch: {x: 150, y: 200, element: "bar-2"}, voice: "Make this one red"}
AI → Canvas: {update: {element: "bar-2", style: {fill: "#ff0000"}}}
```

## Offline Mode

When disconnected from AI:
- View, pan, zoom still work
- Touch highlights elements but doesn't trigger AI actions
- Changes are queued for sync when reconnected
- Banner shows "Offline mode - some features limited"

## Building

```bash
cd saorsa-canvas
cargo build --release
```

The binary is at `target/release/saorsa-canvas`.

## Requirements

- Rust 1.75+ with 2024 edition
- Any modern browser (for the PWA)
- Optional: Terminal with Sixel/Kitty support for inline previews
