# Saorsa Canvas — AI Tool Reference

Render visual content (charts, images, text, 3D, video) to a shared canvas.

## Tools

| Tool | Purpose |
|------|---------|
| `canvas_render` | Render chart, image, or text to the canvas |
| `canvas_render_a2ui` | Render an A2UI component tree with layout |
| `canvas_interact` | Report touch, voice, or selection interaction |
| `canvas_export` | Export canvas to PNG, JPEG, SVG, or PDF |
| `canvas_clear` | Clear all elements from the canvas |
| `canvas_add_element` | Add element with full transform control |
| `canvas_remove_element` | Remove element by ID |
| `canvas_update_element` | Update element position, size, or rotation |
| `canvas_get_scene` | Get current scene as JSON |

## canvas_render

High-level content rendering. Omit `session_id` to use the default session.

### Bar chart

```json
{
  "session_id": "default",
  "content": {
    "type": "Chart",
    "data": {
      "chart_type": "bar",
      "data": { "labels": ["Q1", "Q2", "Q3", "Q4"], "values": [100, 150, 120, 180] },
      "title": "Quarterly Revenue"
    }
  }
}
```

### Line chart

```json
{
  "content": {
    "type": "Chart",
    "data": {
      "chart_type": "line",
      "data": { "labels": ["Jan", "Feb", "Mar", "Apr"], "values": [10, 25, 18, 40] },
      "title": "Monthly Growth"
    }
  }
}
```

### Pie chart

```json
{
  "content": {
    "type": "Chart",
    "data": {
      "chart_type": "pie",
      "data": { "labels": ["Rent", "Food", "Transport", "Other"], "values": [40, 25, 20, 15] },
      "title": "Budget Breakdown"
    }
  }
}
```

### Area chart

```json
{
  "content": {
    "type": "Chart",
    "data": {
      "chart_type": "area",
      "data": { "labels": ["Mon", "Tue", "Wed", "Thu", "Fri"], "values": [5, 12, 8, 15, 20] },
      "title": "Daily Traffic"
    }
  }
}
```

### Scatter plot

```json
{
  "content": {
    "type": "Chart",
    "data": {
      "chart_type": "scatter",
      "data": { "points": [{"x": 1, "y": 2}, {"x": 3, "y": 5}, {"x": 5, "y": 4}, {"x": 7, "y": 8}] },
      "title": "Correlation"
    }
  }
}
```

### Image

```json
{
  "content": {
    "type": "Image",
    "data": { "src": "https://example.com/photo.jpg" }
  }
}
```

### Text annotation

```json
{
  "content": {
    "type": "Text",
    "data": { "content": "Important note", "font_size": 18.0 }
  }
}
```

### With position

Add `position` to place content at a specific location:

```json
{
  "content": { "type": "Text", "data": { "content": "Top-left label", "font_size": 14.0 } },
  "position": { "x": 10, "y": 10, "width": 200, "height": 30 }
}
```

## canvas_render_a2ui

Render a component tree with automatic layout:

```json
{
  "tree": {
    "root": {
      "type": "Container",
      "direction": "column",
      "children": [
        { "type": "Text", "value": "Dashboard", "style": { "fontSize": 24 } },
        { "type": "Chart", "chartType": "bar", "data": { "labels": ["A", "B"], "values": [10, 20] } }
      ]
    }
  },
  "merge": false
}
```

## canvas_interact

Report user interaction with canvas content.

### Touch

```json
{
  "interaction_type": "touch",
  "data": { "element_id": "chart-bar-2", "action": "tap", "x": 150, "y": 200 }
}
```

### Voice with spatial context

```json
{
  "interaction_type": "voice",
  "data": { "transcript": "Make this one red", "context_element": "chart-bar-2" }
}
```

### Selection

```json
{
  "interaction_type": "selection",
  "data": { "element_id": "chart-bar-2", "selected": true }
}
```

## canvas_export

```json
{
  "format": "png",
  "quality": 90
}
```

Formats: `png`, `jpeg`, `svg`, `pdf`. Quality (0-100) applies to lossy formats.

## canvas_clear

```json
{
  "session_id": "default"
}
```

## canvas_add_element

Low-level element creation with full transform control:

```json
{
  "kind": {
    "type": "Chart",
    "data": { "chart_type": "bar", "data": { "labels": ["A", "B"], "values": [10, 20] } }
  },
  "transform": { "x": 50, "y": 50, "width": 400, "height": 300, "rotation": 0, "z_index": 1 },
  "interactive": true
}
```

Element types: `Text`, `Chart`, `Image`, `Model3D`, `Video`, `OverlayLayer`, `Group`.

## canvas_remove_element

```json
{
  "element_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

## canvas_update_element

Partial transform update (only specified fields change):

```json
{
  "element_id": "550e8400-e29b-41d4-a716-446655440000",
  "transform": { "x": 100, "y": 200 }
}
```

## canvas_get_scene

```json
{
  "session_id": "default"
}
```

Returns the full scene graph as JSON including all elements, transforms, and properties.

## Chart data formats

| chart_type | data shape |
|------------|------------|
| `bar` | `{ "labels": [...], "values": [...] }` |
| `line` | `{ "labels": [...], "values": [...] }` |
| `pie` | `{ "labels": [...], "values": [...] }` |
| `area` | `{ "labels": [...], "values": [...] }` |
| `scatter` | `{ "points": [{"x": N, "y": N}, ...] }` |

## Content types

| type | required fields |
|------|----------------|
| `Chart` | `chart_type`, `data` |
| `Image` | `src` |
| `Text` | `content` |
| `Model3D` | `src` |
| `Video` | `stream_id` |

## Installation

Saorsa Canvas server binaries are available from GitHub Releases and crates.io.

### Download from GitHub Releases

Pre-built binaries for all major platforms:

```bash
# Detect platform and download latest release
REPO="saorsa-labs/saorsa-canvas"
VERSION=$(curl -s "https://api.github.com/repos/$REPO/releases/latest" | grep '"tag_name"' | sed 's/.*"tag_name": "\(.*\)".*/\1/')

# macOS Apple Silicon
curl -L "https://github.com/$REPO/releases/download/$VERSION/saorsa-canvas-$VERSION-aarch64-apple-darwin.tar.gz" -o saorsa-canvas.tar.gz

# macOS Intel
curl -L "https://github.com/$REPO/releases/download/$VERSION/saorsa-canvas-$VERSION-x86_64-apple-darwin.tar.gz" -o saorsa-canvas.tar.gz

# Linux x64
curl -L "https://github.com/$REPO/releases/download/$VERSION/saorsa-canvas-$VERSION-x86_64-unknown-linux-gnu.tar.gz" -o saorsa-canvas.tar.gz

# Linux ARM64
curl -L "https://github.com/$REPO/releases/download/$VERSION/saorsa-canvas-$VERSION-aarch64-unknown-linux-gnu.tar.gz" -o saorsa-canvas.tar.gz

# Windows x64
curl -L "https://github.com/$REPO/releases/download/$VERSION/saorsa-canvas-$VERSION-x86_64-pc-windows-msvc.zip" -o saorsa-canvas.zip
```

Extract and run:

```bash
tar -xzf saorsa-canvas.tar.gz  # Unix
./saorsa-canvas                 # Start server on port 9473
```

### Install via crates.io

```bash
cargo install canvas-server
saorsa-canvas
```

### Available platforms

| Platform | Architecture | Archive |
|----------|-------------|---------|
| macOS | Apple Silicon (M1/M2/M3/M4) | `.tar.gz` |
| macOS | Intel x64 | `.tar.gz` |
| Linux | x64 (AMD/Intel) | `.tar.gz` |
| Linux | ARM64 (Raspberry Pi, AWS Graviton) | `.tar.gz` |
| Windows | x64 | `.zip` |

## Patterns

- **Clear and rebuild**: Call `canvas_clear`, then `canvas_render` with new content.
- **Update in place**: Use `canvas_update_element` with the element ID from a previous render.
- **Layer annotations**: Render a chart first, then add `Text` elements on top.
- **Touch + Voice fusion**: "Change THIS to blue" + touch on bar-3 resolves to updating bar-3's color.
