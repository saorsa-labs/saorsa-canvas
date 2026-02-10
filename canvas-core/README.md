# canvas-core

Core canvas logic for [Saorsa Canvas](https://github.com/saorsa-labs/saorsa-canvas) — scene graph, state management, input handling, and persistence.

## Features

- Scene graph with typed elements (text, charts, images, 3D models)
- Transform system with position, size, rotation, and z-ordering
- State management with undo/redo history
- Scene persistence via `SceneStore`
- Compiles to WASM for browser deployment

## Installation

```toml
[dependencies]
canvas-core = "0.1.4"
```

For WASM targets:

```toml
[dependencies]
canvas-core = { version = "0.1.4", features = ["wasm"] }
```

## Usage

```rust
use canvas_core::{Scene, element::{Element, ElementKind, Transform}};

let mut scene = Scene::new(800.0, 600.0);

scene.add_element(
    Element::new(ElementKind::Text {
        content: "Hello Canvas".to_string(),
        font_size: 24.0,
        color: "#000000".to_string(),
    })
    .with_transform(Transform {
        x: 10.0, y: 20.0,
        width: 200.0, height: 40.0,
        rotation: 0.0, z_index: 0,
    })
);
```

## License

MIT OR Apache-2.0
