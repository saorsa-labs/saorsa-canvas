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

pub mod a2ui;
pub mod element;
pub mod error;
pub mod event;
pub mod fusion;
pub mod offline;
pub mod scene;
pub mod schema;
pub mod state;
pub mod store;

#[cfg(feature = "wasm")]
pub mod wasm;

pub use a2ui::{A2UINode, A2UIStyle, A2UITree, ConversionResult, Layout};
pub use element::{
    CropRect, Element, ElementId, ElementKind, ImageFormat, MediaConfig, MediaStats, QualityPreset,
    Resolution, Transform,
};
pub use error::{CanvasError, CanvasResult};
pub use event::{InputEvent, TouchEvent, TouchPhase, TouchPoint, VoiceEvent};
pub use fusion::{FusedIntent, FusionConfig, FusionResult, InputFusion, VoiceOnlyIntent};
pub use offline::{ConflictResolution, ConflictStrategy, OfflineQueue, Operation, SyncResult};
pub use scene::Scene;
pub use schema::{ElementDocument, SceneDocument, ViewportDocument};
pub use state::{CanvasState, ConnectionStatus};
pub use store::{SceneStore, StoreError};

/// Canvas core version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
