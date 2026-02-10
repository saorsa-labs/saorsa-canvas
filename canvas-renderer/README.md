# canvas-renderer

Rendering engine for [Saorsa Canvas](https://github.com/saorsa-labs/saorsa-canvas) — GPU rendering via wgpu with SVG/PNG/JPEG/PDF export.

## Features

- GPU rendering via wgpu (WebGPU/WebGL2)
- Chart rendering (bar, line, pie, scatter) via plotters
- Image element support
- Export to PNG, JPEG, SVG, and PDF (via `export` feature)
- WASM-compatible rendering path

## Installation

```toml
[dependencies]
canvas-renderer = "0.1.4"
```

Enable export support:

```toml
[dependencies]
canvas-renderer = { version = "0.1.4", features = ["export"] }
```

## Usage

```rust
use canvas_core::Scene;
use canvas_renderer::export::{SceneExporter, ExportFormat};

let scene = Scene::new(800.0, 600.0);
let exporter = SceneExporter::with_defaults();
let png_bytes = exporter.export(&scene, ExportFormat::Png)?;
```

## Feature Flags

| Feature | Default | Description |
|---------|---------|-------------|
| `gpu` | yes | wgpu-based GPU rendering |
| `charts` | yes | Chart rendering via plotters |
| `images` | yes | Image element support |
| `export` | no | PNG/JPEG/SVG/PDF export via resvg + tiny-skia |
| `wasm` | no | WASM/browser target support |

## License

MIT OR Apache-2.0
