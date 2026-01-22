# A2UI Support in Saorsa Canvas

Saorsa Canvas implements support for [Google's A2UI (Agent-to-User Interface)](https://cloud.google.com/vertex-ai/generative-ai/docs/extensions/a2ui) specification, enabling AI agents to generate visual UI components that are rendered on the canvas.

## Overview

A2UI provides a declarative, JSON-based format for AI agents to specify UI components. Saorsa Canvas converts these component trees into canvas elements with automatic layout calculation.

## Supported Components

### Text

Displays text content with optional styling.

```json
{
  "component": "text",
  "content": "Hello, World!",
  "style": {
    "font_size": 16.0,
    "color": "#000000"
  }
}
```

**Properties:**
- `content` (required): The text to display
- `style` (optional): Styling options

### Container

Groups child components with layout options.

```json
{
  "component": "container",
  "layout": "vertical",
  "children": [
    { "component": "text", "content": "Item 1" },
    { "component": "text", "content": "Item 2" }
  ],
  "style": {
    "padding": 10.0
  }
}
```

**Properties:**
- `layout` (required): Layout direction - `"vertical"`, `"horizontal"`, or `"grid"`
- `children` (required): Array of child components
- `style` (optional): Styling options

### Button

Interactive button that triggers actions.

```json
{
  "component": "button",
  "label": "Submit",
  "action": "form_submit",
  "style": {
    "background_color": "#007AFF"
  }
}
```

**Properties:**
- `label` (required): Button text
- `action` (required): Action identifier sent on click
- `style` (optional): Styling options

### Image

Displays an image from a URL or base64 data URI.

```json
{
  "component": "image",
  "src": "https://example.com/image.png",
  "alt": "Description"
}
```

**Properties:**
- `src` (required): Image source URL or base64 data URI
- `alt` (optional): Alternative text description

### VideoFeed (Extension)

Saorsa Canvas extension for live video streams.

```json
{
  "component": "video_feed",
  "stream_id": "camera-1",
  "width": 640,
  "height": 480
}
```

**Properties:**
- `stream_id` (required): WebRTC stream identifier
- `width` (optional): Video width in pixels (default: 640)
- `height` (optional): Video height in pixels (default: 480)

## Layout System

### Vertical Layout

Children are stacked top-to-bottom.

```json
{
  "component": "container",
  "layout": "vertical",
  "children": [...]
}
```

### Horizontal Layout

Children are arranged left-to-right with automatic wrapping.

```json
{
  "component": "container",
  "layout": "horizontal",
  "children": [...]
}
```

### Grid Layout

Children are arranged in a grid (wraps when reaching container width).

```json
{
  "component": "container",
  "layout": "grid",
  "children": [...]
}
```

## Styling

All components support an optional `style` object with these properties:

| Property | Type | Description |
|----------|------|-------------|
| `font_size` | `f32` | Font size in points |
| `color` | `string` | Text/foreground color (hex format `#RRGGBB`) |
| `background` | `string` | Background color (hex format) |
| `padding` | `f32` | Inner padding in pixels |
| `margin` | `f32` | Outer margin in pixels |
| `width` | `f32` | Explicit width in pixels |
| `height` | `f32` | Explicit height in pixels |

## Data Model

A2UI trees can include a `data_model` object for associating data with the UI:

```json
{
  "root": {
    "component": "text",
    "content": "Welcome, {user.name}!"
  },
  "data_model": {
    "user": {
      "name": "Alice",
      "email": "alice@example.com"
    }
  }
}
```

## API Integration

### MCP Tool: `canvas_render_a2ui`

Render an A2UI tree via the MCP protocol:

```json
{
  "method": "tools/call",
  "params": {
    "name": "canvas_render_a2ui",
    "arguments": {
      "tree": {
        "root": {
          "component": "text",
          "content": "Hello from AI!"
        }
      },
      "session_id": "default",
      "merge": false,
      "offset_x": 0,
      "offset_y": 0
    }
  }
}
```

**Parameters:**
- `tree` (required): A2UI component tree
- `session_id` (optional): Target session (default: `"default"`)
- `merge` (optional): If `true`, adds to existing elements; if `false`, clears first
- `offset_x`, `offset_y` (optional): Position offset for rendered elements

### AG-UI SSE Stream

Subscribe to real-time updates via Server-Sent Events:

```
GET /agui/stream?session_id=default
```

Events:
- `scene_update`: Scene was modified
- `heartbeat`: Keep-alive signal
- `interaction`: User interaction occurred

### REST Endpoint

Render A2UI via REST:

```
POST /agui/render
Content-Type: application/json

{
  "tree": { ... },
  "session_id": "default",
  "clear": true
}
```

## Interaction Events

User interactions are broadcast to AG-UI SSE clients:

### Touch Events

```json
{
  "type": "interaction",
  "session_id": "default",
  "interaction": {
    "type": "touch",
    "element_id": "btn-1",
    "phase": "start",
    "x": 100.0,
    "y": 200.0,
    "pointer_id": 0
  },
  "timestamp": 1705936142000
}
```

### Button Click Events

```json
{
  "type": "interaction",
  "session_id": "default",
  "interaction": {
    "type": "button_click",
    "element_id": "submit-btn",
    "action": "form_submit"
  },
  "timestamp": 1705936142000
}
```

### Form Input Events

```json
{
  "type": "interaction",
  "session_id": "default",
  "interaction": {
    "type": "form_input",
    "element_id": "name-input",
    "field": "username",
    "value": "alice"
  },
  "timestamp": 1705936142000
}
```

### Selection Events

```json
{
  "type": "interaction",
  "session_id": "default",
  "interaction": {
    "type": "selection",
    "element_id": "checkbox-1",
    "selected": true
  },
  "timestamp": 1705936142000
}
```

### Gesture Events

```json
{
  "type": "interaction",
  "session_id": "default",
  "interaction": {
    "type": "gesture",
    "gesture_type": "pinch",
    "scale": 1.5,
    "rotation": 45.0,
    "center_x": 200.0,
    "center_y": 300.0
  },
  "timestamp": 1705936142000
}
```

## Complete Example

Here's a complete example of a card UI with multiple components:

```json
{
  "root": {
    "component": "container",
    "layout": "vertical",
    "style": {
      "padding": 16.0,
      "background_color": "#FFFFFF",
      "border_radius": 8.0
    },
    "children": [
      {
        "component": "text",
        "content": "Welcome to Saorsa Canvas",
        "style": {
          "font_size": 24.0,
          "color": "#1A1A1A"
        }
      },
      {
        "component": "text",
        "content": "AI-native visual interface layer",
        "style": {
          "font_size": 14.0,
          "color": "#666666"
        }
      },
      {
        "component": "container",
        "layout": "horizontal",
        "children": [
          {
            "component": "button",
            "label": "Get Started",
            "action": "start",
            "style": {
              "background_color": "#007AFF"
            }
          },
          {
            "component": "button",
            "label": "Learn More",
            "action": "learn"
          }
        ]
      }
    ]
  },
  "data_model": {
    "version": "1.0.0",
    "feature_flags": {
      "video_compositing": true
    }
  }
}
```

## Conversion to Canvas Elements

A2UI components are converted to canvas elements as follows:

| A2UI Component | Canvas Element |
|----------------|----------------|
| `text` | `Text` element |
| `container` | Layout pass only (no element) |
| `button` | `Text` element with interaction |
| `image` | `Image` element |
| `chart` | `Chart` element |
| `video_feed` | `Video` element |

Layout is calculated during conversion with automatic positioning based on the container's layout mode.

## Error Handling

- Invalid JSON returns a parse error
- Missing required fields return validation errors
- Unknown component types return "unsupported component" errors
- Layout calculation errors include warnings in the response
