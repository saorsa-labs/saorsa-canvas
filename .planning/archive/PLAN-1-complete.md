# Phase 1: State Consolidation

> Unify scene storage so MCP, WebSocket, HTTP, and AG-UI all use one source of truth.

## Overview

**Problem**: `CanvasMcpServer` maintains its own `sessions: HashMap<String, SessionState>` while `SyncState` maintains a separate `scenes: HashMap<String, Scene>`. Changes through MCP don't update SyncState (only a callback fires), and changes through WebSocket/HTTP don't update MCP's internal state.

**Solution**: Create a shared `SceneStore` type that both components reference. All scene mutations flow through `SceneStore`, ensuring consistency.

## Prerequisites

- [ ] All tests pass: `cargo test --workspace`
- [ ] No clippy warnings: `cargo clippy --workspace -- -D warnings`
- [ ] Review current state duplication in `canvas-mcp/src/server.rs` and `canvas-server/src/sync.rs`

---

## Tasks

<task type="auto" priority="p0">
  <n>Create SceneStore abstraction in canvas-core</n>
  <files>
    canvas-core/src/lib.rs,
    canvas-core/src/store.rs
  </files>
  <action>
    Create a new `store.rs` module in canvas-core that provides a thread-safe scene storage abstraction:

    1. Create `canvas-core/src/store.rs`:
       ```rust
       //! Shared scene storage for multi-component access.

       use std::collections::HashMap;
       use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};
       use crate::{Scene, SceneDocument, ElementId, Element, ElementDocument};

       /// Thread-safe scene storage shared across MCP, WebSocket, and HTTP.
       #[derive(Clone)]
       pub struct SceneStore {
           scenes: Arc<RwLock<HashMap<String, Scene>>>,
       }

       impl SceneStore {
           /// Create a new scene store with a default session.
           pub fn new() -> Self { ... }

           /// Get or create a scene for a session.
           pub fn get_or_create(&self, session_id: &str) -> Scene { ... }

           /// Get a scene by session ID.
           pub fn get(&self, session_id: &str) -> Option<Scene> { ... }

           /// Replace a scene entirely.
           pub fn replace(&self, session_id: &str, scene: Scene) -> Result<(), StoreError> { ... }

           /// Update a scene with a closure.
           pub fn update<F>(&self, session_id: &str, f: F) -> Result<(), StoreError>
           where F: FnOnce(&mut Scene) { ... }

           /// Add an element to a session's scene.
           pub fn add_element(&self, session_id: &str, element: Element) -> Result<ElementId, StoreError> { ... }

           /// Remove an element from a session's scene.
           pub fn remove_element(&self, session_id: &str, id: ElementId) -> Result<(), StoreError> { ... }

           /// Get mutable access to an element.
           pub fn update_element<F>(&self, session_id: &str, id: ElementId, f: F) -> Result<(), StoreError>
           where F: FnOnce(&mut Element) { ... }

           /// Get a scene as a canonical document.
           pub fn scene_document(&self, session_id: &str) -> SceneDocument { ... }

           /// List all session IDs.
           pub fn session_ids(&self) -> Vec<String> { ... }

           /// Clear a session's scene.
           pub fn clear(&self, session_id: &str) -> Result<(), StoreError> { ... }
       }

       #[derive(Debug, thiserror::Error)]
       pub enum StoreError {
           #[error("Lock poisoned")]
           LockPoisoned,
           #[error("Session not found: {0}")]
           SessionNotFound(String),
           #[error("Element not found: {0}")]
           ElementNotFound(String),
           #[error("Scene error: {0}")]
           SceneError(String),
       }
       ```

    2. Export from `canvas-core/src/lib.rs`:
       ```rust
       mod store;
       pub use store::{SceneStore, StoreError};
       ```

    3. Add `thiserror` to canvas-core dependencies if not present.

    4. Use `unwrap_or_else(|e| e.into_inner())` for lock recovery (not unwrap).
  </action>
  <verify>
    cargo fmt -p canvas-core -- --check
    cargo clippy -p canvas-core -- -D warnings
    cargo test -p canvas-core
  </verify>
  <done>
    - SceneStore type exists with all methods
    - StoreError enum covers all failure cases
    - Unit tests pass for basic operations
    - No clippy warnings or unwrap calls
  </done>
</task>

<task type="auto" priority="p1">
  <n>Refactor SyncState to use SceneStore</n>
  <files>
    canvas-server/src/sync.rs,
    canvas-server/src/main.rs,
    canvas-server/src/routes.rs
  </files>
  <action>
    Modify SyncState to wrap SceneStore instead of having its own scenes map:

    1. Update `canvas-server/src/sync.rs`:
       ```rust
       use canvas_core::SceneStore;

       pub struct SyncState {
           store: SceneStore,
           event_tx: broadcast::Sender<SyncEvent>,
           offline_queue: Arc<RwLock<OfflineQueue>>,
       }

       impl SyncState {
           pub fn new() -> Self {
               let (event_tx, _) = broadcast::channel(100);
               Self {
                   store: SceneStore::new(),
                   event_tx,
                   offline_queue: Arc::new(RwLock::new(OfflineQueue::new())),
               }
           }

           /// Get the underlying SceneStore for sharing with MCP.
           pub fn store(&self) -> SceneStore {
               self.store.clone()
           }

           // Delegate methods to store, adding broadcast logic:
           pub fn get_or_create_scene(&self, session_id: &str) -> Scene {
               self.store.get_or_create(session_id)
           }

           pub fn replace_scene(&self, session_id: &str, scene: Scene, origin: SyncOrigin) -> Result<(), SyncError> {
               self.store.replace(session_id, scene.clone())?;
               self.broadcast_scene_update(session_id, origin);
               Ok(())
           }

           // ... update remaining methods to use self.store
       }
       ```

    2. Convert `SyncError` to wrap `StoreError`:
       ```rust
       impl From<canvas_core::StoreError> for SyncError {
           fn from(e: canvas_core::StoreError) -> Self {
               match e {
                   StoreError::LockPoisoned => SyncError::LockPoisoned,
                   StoreError::SessionNotFound(s) => SyncError::SessionNotFound(s),
                   StoreError::ElementNotFound(s) => SyncError::ElementNotFound(s),
                   StoreError::SceneError(s) => SyncError::InvalidMessage(s),
               }
           }
       }
       ```

    3. Update `canvas-server/src/main.rs` to expose SceneStore to MCP:
       ```rust
       let sync_state = SyncState::new();
       let scene_store = sync_state.store();
       // Pass scene_store to MCP server constructor
       ```

    4. Ensure all existing tests still pass.
  </action>
  <verify>
    cargo fmt -p canvas-server -- --check
    cargo clippy -p canvas-server -- -D warnings
    cargo test -p canvas-server
  </verify>
  <done>
    - SyncState wraps SceneStore
    - SyncState.store() returns cloneable SceneStore
    - All existing sync tests pass
    - Broadcast still works on mutations
    - No duplicate scene storage in SyncState
  </done>
</task>

<task type="auto" priority="p1">
  <n>Refactor CanvasMcpServer to use shared SceneStore</n>
  <files>
    canvas-mcp/src/server.rs,
    canvas-mcp/src/lib.rs,
    canvas-server/src/main.rs
  </files>
  <action>
    Remove internal scene storage from CanvasMcpServer and use injected SceneStore:

    1. Update `canvas-mcp/src/server.rs`:
       - Remove `sessions: Arc<RwLock<HashMap<String, SessionState>>>`
       - Add `store: SceneStore` field
       - Keep session metadata (CanvasSession) separate if needed for resources/list

       ```rust
       use canvas_core::SceneStore;

       pub struct CanvasMcpServer {
           store: SceneStore,
           session_metadata: Arc<RwLock<HashMap<String, CanvasSession>>>,
           on_change: Option<OnChangeCallback>,
       }

       impl CanvasMcpServer {
           pub fn new(store: SceneStore) -> Self {
               Self {
                   store,
                   session_metadata: Arc::new(RwLock::new(HashMap::new())),
                   on_change: None,
               }
           }

           // Update all tool implementations to use self.store instead of self.sessions
       }
       ```

    2. Update tool methods like `call_canvas_render`:
       ```rust
       async fn call_canvas_render(&self, arguments: serde_json::Value) -> ToolResponse {
           // ... parse params ...
           let element = create_element_from_content(&params.content);
           let element_id = self.store.add_element(&session_id, element)?;

           if let Some(ref callback) = self.on_change {
               let scene = self.store.get(&session_id).unwrap_or_default();
               callback(&session_id, &scene);
           }

           ToolResponse::success(...)
       }
       ```

    3. Update `canvas-server/src/main.rs`:
       ```rust
       let sync_state = SyncState::new();
       let scene_store = sync_state.store();

       let mut mcp = CanvasMcpServer::new(scene_store);
       mcp.set_on_change(move |session_id, scene| {
           // Broadcast to WebSocket clients
       });
       ```

    4. Remove `import_scene_document` or update to use store.

    5. Update all 11 existing MCP tests to pass with new constructor.
  </action>
  <verify>
    cargo fmt -p canvas-mcp -- --check
    cargo clippy -p canvas-mcp -- -D warnings
    cargo test -p canvas-mcp
    cargo test --workspace
  </verify>
  <done>
    - CanvasMcpServer constructor takes SceneStore
    - No internal scenes HashMap in MCP server
    - All 8 MCP tools use shared store
    - All 11 MCP tests pass
    - MCP changes immediately visible to WebSocket clients
    - WebSocket changes immediately visible via MCP get_scene
  </done>
</task>

---

## Exit Criteria

- [ ] Single `SceneStore` instance shared between MCP and SyncState
- [ ] MCP tool calls update the same scenes that WebSocket clients see
- [ ] WebSocket mutations are visible via `canvas_get_scene` MCP tool
- [ ] All existing tests pass: `cargo test --workspace`
- [ ] No clippy warnings: `cargo clippy --workspace -- -D warnings`
- [ ] Integration test: MCP add -> WebSocket receives broadcast
- [ ] Integration test: WebSocket add -> MCP get_scene returns it

## Notes

- Keep `on_change` callback for now as a bridge to broadcast channel
- Session metadata (name, created_at, modified_at, element_count) can live in MCP server separately
- Consider removing session metadata entirely in Phase 2 if not needed
- The `OfflineQueue` in SyncState remains unchanged for now (Phase 3 will address offline sync)
