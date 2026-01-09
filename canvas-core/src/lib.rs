//! # Saorsa Canvas Core
//!
//! Core canvas logic for universal AI visual output.
//! Compiles to WASM for true cross-platform portability.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────┐
//! │              canvas-core.wasm               │
//! ├─────────────────────────────────────────────┤
//! │  Scene Graph     │  Input Handler           │
//! │  - Elements      │  - Touch events          │
//! │  - Transforms    │  - Gesture recognition   │
//! │  - Hierarchy     │  - Voice command bridge  │
//! ├─────────────────────────────────────────────┤
//! │  State Machine   │  Layout Engine           │
//! │  - Offline mode  │  - Responsive sizing     │
//! │  - Sync queue    │  - Constraint solving    │
//! └─────────────────────────────────────────────┘
//! ```

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(clippy::all)]
#![deny(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]

pub mod element;
pub mod error;
pub mod event;
pub mod scene;
pub mod state;

#[cfg(feature = "wasm")]
pub mod wasm;

pub use element::{Element, ElementId, ElementKind, Transform};
pub use error::{CanvasError, CanvasResult};
pub use event::{InputEvent, TouchEvent, TouchPhase};
pub use scene::Scene;
pub use state::{CanvasState, ConnectionStatus};

/// Canvas core version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
