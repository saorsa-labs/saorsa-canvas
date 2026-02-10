//! Shared scene storage for multi-component access.
//!
//! Provides a thread-safe [`SceneStore`] that can be shared across MCP handlers,
//! WebSocket connections, and HTTP routes for consistent scene state management.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::{Element, ElementId, Scene, SceneDocument};

/// Default session identifier.
pub const DEFAULT_SESSION: &str = "default";

/// Default viewport width in pixels.
const DEFAULT_WIDTH: f32 = 800.0;

/// Default viewport height in pixels.
const DEFAULT_HEIGHT: f32 = 600.0;

/// Errors that can occur during store operations.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// The internal lock was poisoned by a panicking thread.
    #[error("Lock poisoned")]
    LockPoisoned,
    /// The requested session does not exist.
    #[error("Session not found: {0}")]
    SessionNotFound(String),
    /// The requested element does not exist in the session.
    #[error("Element not found: {0}")]
    ElementNotFound(String),
    /// An error occurred while manipulating the scene.
    #[error("Scene error: {0}")]
    SceneError(String),
    /// An I/O error occurred during persistence.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    /// A serialization or deserialization error occurred.
    #[error("Serialization error: {0}")]
    Serialization(String),
}

/// Thread-safe scene storage shared across MCP, WebSocket, and HTTP.
///
/// # Example
///
/// ```
/// use canvas_core::store::SceneStore;
/// use canvas_core::{Element, ElementKind};
///
/// let store = SceneStore::new();
///
/// // Add an element to the default session
/// let element = Element::new(ElementKind::Text {
///     content: "Hello".to_string(),
///     font_size: 16.0,
///     color: "#000000".to_string(),
/// });
///
/// let id = store.add_element("default", element).unwrap();
/// ```
#[derive(Debug, Clone, Default)]
pub struct SceneStore {
    scenes: Arc<RwLock<HashMap<String, Scene>>>,
    /// Optional data directory for filesystem persistence.
    data_dir: Option<PathBuf>,
}

impl SceneStore {
    /// Create a new store with a default session (no persistence).
    ///
    /// The default session is created with an 800x600 viewport.
    #[must_use]
    pub fn new() -> Self {
        let mut scenes = HashMap::new();
        scenes.insert(
            DEFAULT_SESSION.to_string(),
            Scene::new(DEFAULT_WIDTH, DEFAULT_HEIGHT),
        );
        Self {
            scenes: Arc::new(RwLock::new(scenes)),
            data_dir: None,
        }
    }

    /// Create a store with filesystem persistence.
    ///
    /// Sessions are saved as JSON files in `data_dir`. The directory is created
    /// if it doesn't exist.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::Io`] if the directory cannot be created.
    pub fn with_data_dir(data_dir: impl Into<PathBuf>) -> Result<Self, StoreError> {
        let data_dir = data_dir.into();
        std::fs::create_dir_all(&data_dir)?;
        let mut scenes = HashMap::new();
        scenes.insert(
            DEFAULT_SESSION.to_string(),
            Scene::new(DEFAULT_WIDTH, DEFAULT_HEIGHT),
        );
        Ok(Self {
            scenes: Arc::new(RwLock::new(scenes)),
            data_dir: Some(data_dir),
        })
    }

    /// Get or create a scene for the given session ID.
    ///
    /// If the session does not exist, a new scene with default viewport is created.
    #[must_use]
    pub fn get_or_create(&self, session_id: &str) -> Scene {
        let mut scenes = self
            .scenes
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        scenes
            .entry(session_id.to_string())
            .or_insert_with(|| Scene::new(DEFAULT_WIDTH, DEFAULT_HEIGHT))
            .clone()
    }

    /// Get a scene by session ID if it exists.
    #[must_use]
    pub fn get(&self, session_id: &str) -> Option<Scene> {
        let scenes = self
            .scenes
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        scenes.get(session_id).cloned()
    }

    /// Replace the entire scene for a session.
    ///
    /// Creates the session if it does not exist.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::LockPoisoned`] if the lock is poisoned (currently
    /// recovered from, so this variant is reserved for future stricter modes).
    pub fn replace(&self, session_id: &str, scene: Scene) -> Result<(), StoreError> {
        {
            let mut scenes = self
                .scenes
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            scenes.insert(session_id.to_string(), scene);
        }
        self.persist_session(session_id);
        Ok(())
    }

    /// Update a scene using a closure.
    ///
    /// The closure receives a mutable reference to the scene and can modify it.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::SessionNotFound`] if the session does not exist.
    pub fn update<F>(&self, session_id: &str, f: F) -> Result<(), StoreError>
    where
        F: FnOnce(&mut Scene),
    {
        {
            let mut scenes = self
                .scenes
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let scene = scenes
                .get_mut(session_id)
                .ok_or_else(|| StoreError::SessionNotFound(session_id.to_string()))?;
            f(scene);
        }
        self.persist_session(session_id);
        Ok(())
    }

    /// Add an element to a session's scene.
    ///
    /// Creates the session if it does not exist.
    ///
    /// # Errors
    ///
    /// Currently infallible but returns `Result` for API consistency.
    pub fn add_element(&self, session_id: &str, element: Element) -> Result<ElementId, StoreError> {
        let id = {
            let mut scenes = self
                .scenes
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let scene = scenes
                .entry(session_id.to_string())
                .or_insert_with(|| Scene::new(DEFAULT_WIDTH, DEFAULT_HEIGHT));
            scene.add_element(element)
        };
        self.persist_session(session_id);
        Ok(id)
    }

    /// Remove an element from a session's scene.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::SessionNotFound`] if the session does not exist.
    /// Returns [`StoreError::ElementNotFound`] if the element does not exist.
    pub fn remove_element(&self, session_id: &str, id: ElementId) -> Result<(), StoreError> {
        {
            let mut scenes = self
                .scenes
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let scene = scenes
                .get_mut(session_id)
                .ok_or_else(|| StoreError::SessionNotFound(session_id.to_string()))?;
            scene
                .remove_element(&id)
                .map_err(|e| StoreError::ElementNotFound(e.to_string()))?;
        }
        self.persist_session(session_id);
        Ok(())
    }

    /// Update an element using a closure.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::SessionNotFound`] if the session does not exist.
    /// Returns [`StoreError::ElementNotFound`] if the element does not exist.
    pub fn update_element<F>(&self, session_id: &str, id: ElementId, f: F) -> Result<(), StoreError>
    where
        F: FnOnce(&mut Element),
    {
        {
            let mut scenes = self
                .scenes
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let scene = scenes
                .get_mut(session_id)
                .ok_or_else(|| StoreError::SessionNotFound(session_id.to_string()))?;
            let element = scene
                .get_element_mut(id)
                .ok_or_else(|| StoreError::ElementNotFound(id.to_string()))?;
            f(element);
        }
        self.persist_session(session_id);
        Ok(())
    }

    /// Get the canonical document representation of a scene.
    ///
    /// If the session does not exist, returns a document for an empty scene.
    #[must_use]
    pub fn scene_document(&self, session_id: &str) -> SceneDocument {
        let scenes = self
            .scenes
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let timestamp = current_timestamp_ms();
        if let Some(scene) = scenes.get(session_id) {
            SceneDocument::from_scene(session_id, scene, timestamp)
        } else {
            let empty_scene = Scene::new(DEFAULT_WIDTH, DEFAULT_HEIGHT);
            SceneDocument::from_scene(session_id, &empty_scene, timestamp)
        }
    }

    /// Get a list of all session IDs.
    #[must_use]
    pub fn session_ids(&self) -> Vec<String> {
        let scenes = self
            .scenes
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        scenes.keys().cloned().collect()
    }

    // -----------------------------------------------------------------------
    // Persistence
    // -----------------------------------------------------------------------

    /// Save a session's scene to disk as JSON.
    ///
    /// No-op if the store was created without a data directory.
    fn persist_session(&self, session_id: &str) {
        let Some(ref data_dir) = self.data_dir else {
            return;
        };
        let doc = self.scene_document(session_id);
        let json = match serde_json::to_string_pretty(&doc) {
            Ok(j) => j,
            Err(e) => {
                tracing::warn!("Failed to serialize session {session_id}: {e}");
                return;
            }
        };
        let path = data_dir.join(format!("{}.json", sanitize_filename(session_id)));
        if let Err(e) = std::fs::write(&path, json) {
            tracing::warn!(
                "Failed to persist session {session_id} to {}: {e}",
                path.display()
            );
        }
    }

    /// Load a single session from disk into memory.
    ///
    /// # Errors
    ///
    /// Returns an error if the file doesn't exist or can't be parsed.
    pub fn load_session_from_disk(&self, session_id: &str) -> Result<(), StoreError> {
        let data_dir = self
            .data_dir
            .as_ref()
            .ok_or_else(|| StoreError::SceneError("No data directory configured".into()))?;
        let path = data_dir.join(format!("{}.json", sanitize_filename(session_id)));
        let contents = std::fs::read_to_string(&path)?;
        let doc: SceneDocument = serde_json::from_str(&contents)
            .map_err(|e| StoreError::Serialization(e.to_string()))?;

        // Rebuild Scene from SceneDocument.
        let mut scene = Scene::new(doc.viewport.width, doc.viewport.height);
        scene.zoom = doc.viewport.zoom;
        scene.pan_x = doc.viewport.pan_x;
        scene.pan_y = doc.viewport.pan_y;
        for elem_doc in &doc.elements {
            let element = crate::Element::new(elem_doc.kind.clone())
                .with_transform(elem_doc.transform)
                .with_interactive(elem_doc.interactive);
            scene.add_element(element);
        }

        let mut scenes = self
            .scenes
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        scenes.insert(session_id.to_string(), scene);
        Ok(())
    }

    /// Discover and load all persisted sessions from the data directory.
    ///
    /// Returns a list of session IDs that were found on disk.
    ///
    /// # Errors
    ///
    /// Returns an error if the data directory can't be read.
    pub fn load_all_sessions(&self) -> Result<Vec<String>, StoreError> {
        let data_dir = self
            .data_dir
            .as_ref()
            .ok_or_else(|| StoreError::SceneError("No data directory configured".into()))?;
        let mut session_ids = Vec::new();
        for entry in std::fs::read_dir(data_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "json") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    session_ids.push(stem.to_string());
                }
            }
        }
        Ok(session_ids)
    }

    /// Remove a session's persisted file from disk.
    ///
    /// No-op if the store has no data directory or the file doesn't exist.
    pub fn delete_session_file(&self, session_id: &str) {
        let Some(ref data_dir) = self.data_dir else {
            return;
        };
        let path = data_dir.join(format!("{}.json", sanitize_filename(session_id)));
        if path.exists() {
            if let Err(e) = std::fs::remove_file(&path) {
                tracing::warn!("Failed to delete session file {}: {e}", path.display());
            }
        }
    }

    /// Clear all elements from a session's scene.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::SessionNotFound`] if the session does not exist.
    pub fn clear(&self, session_id: &str) -> Result<(), StoreError> {
        {
            let mut scenes = self
                .scenes
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let scene = scenes
                .get_mut(session_id)
                .ok_or_else(|| StoreError::SessionNotFound(session_id.to_string()))?;
            scene.clear();
        }
        self.persist_session(session_id);
        Ok(())
    }
}

/// Sanitize a session ID for use as a filename.
///
/// Replaces any character that is not alphanumeric, `-`, or `_` with `_`.
fn sanitize_filename(session_id: &str) -> String {
    session_id
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Get the current Unix timestamp in milliseconds.
fn current_timestamp_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| {
        // Timestamp will not exceed u64 max for millennia
        #[allow(clippy::cast_possible_truncation)]
        {
            d.as_millis() as u64
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ElementKind;

    #[test]
    fn test_new_creates_default_session() {
        let store = SceneStore::new();
        let ids = store.session_ids();
        assert!(ids.contains(&DEFAULT_SESSION.to_string()));
    }

    #[test]
    fn test_get_or_create_existing() {
        let store = SceneStore::new();
        let scene = store.get_or_create(DEFAULT_SESSION);
        assert!((scene.viewport_width - DEFAULT_WIDTH).abs() < f32::EPSILON);
        assert!((scene.viewport_height - DEFAULT_HEIGHT).abs() < f32::EPSILON);
    }

    #[test]
    fn test_get_or_create_new_session() {
        let store = SceneStore::new();
        let scene = store.get_or_create("new-session");
        assert!(scene.is_empty());
        assert!(store.session_ids().contains(&"new-session".to_string()));
    }

    #[test]
    fn test_get_nonexistent_returns_none() {
        let store = SceneStore::new();
        assert!(store.get("nonexistent").is_none());
    }

    #[test]
    fn test_add_and_get_element() {
        let store = SceneStore::new();
        let element = Element::new(ElementKind::Text {
            content: "Hello".to_string(),
            font_size: 16.0,
            color: "#000000".to_string(),
        });

        let id = store
            .add_element(DEFAULT_SESSION, element)
            .expect("should add element");

        let scene = store.get(DEFAULT_SESSION).expect("session should exist");
        assert!(scene.get_element(id).is_some());
    }

    #[test]
    fn test_remove_element() {
        let store = SceneStore::new();
        let element = Element::new(ElementKind::Text {
            content: "Remove me".to_string(),
            font_size: 14.0,
            color: "#FF0000".to_string(),
        });

        let id = store
            .add_element(DEFAULT_SESSION, element)
            .expect("should add");

        store
            .remove_element(DEFAULT_SESSION, id)
            .expect("should remove");

        let scene = store.get(DEFAULT_SESSION).expect("session exists");
        assert!(scene.get_element(id).is_none());
    }

    #[test]
    fn test_remove_nonexistent_element_fails() {
        let store = SceneStore::new();
        let fake_id = ElementId::new();
        let result = store.remove_element(DEFAULT_SESSION, fake_id);
        assert!(matches!(result, Err(StoreError::ElementNotFound(_))));
    }

    #[test]
    fn test_update_element() {
        let store = SceneStore::new();
        let element = Element::new(ElementKind::Text {
            content: "Original".to_string(),
            font_size: 12.0,
            color: "#000000".to_string(),
        });

        let id = store
            .add_element(DEFAULT_SESSION, element)
            .expect("should add");

        store
            .update_element(DEFAULT_SESSION, id, |el| {
                el.transform.x = 100.0;
                el.transform.y = 200.0;
            })
            .expect("should update");

        let scene = store.get(DEFAULT_SESSION).expect("session exists");
        let updated = scene.get_element(id).expect("element exists");
        assert!((updated.transform.x - 100.0).abs() < f32::EPSILON);
        assert!((updated.transform.y - 200.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_replace_scene() {
        let store = SceneStore::new();
        let mut new_scene = Scene::new(1920.0, 1080.0);
        new_scene.add_element(Element::new(ElementKind::Text {
            content: "New scene".to_string(),
            font_size: 20.0,
            color: "#00FF00".to_string(),
        }));

        store.replace(DEFAULT_SESSION, new_scene).expect("replace");

        let scene = store.get(DEFAULT_SESSION).expect("session exists");
        assert!((scene.viewport_width - 1920.0).abs() < f32::EPSILON);
        assert_eq!(scene.element_count(), 1);
    }

    #[test]
    fn test_clear_session() {
        let store = SceneStore::new();
        store
            .add_element(
                DEFAULT_SESSION,
                Element::new(ElementKind::Text {
                    content: "Test".to_string(),
                    font_size: 16.0,
                    color: "#000".to_string(),
                }),
            )
            .expect("add");

        store.clear(DEFAULT_SESSION).expect("clear");

        let scene = store.get(DEFAULT_SESSION).expect("session exists");
        assert!(scene.is_empty());
    }

    #[test]
    fn test_clear_nonexistent_session_fails() {
        let store = SceneStore::new();
        let result = store.clear("nonexistent");
        assert!(matches!(result, Err(StoreError::SessionNotFound(_))));
    }

    #[test]
    fn test_scene_document() {
        let store = SceneStore::new();
        store
            .add_element(
                DEFAULT_SESSION,
                Element::new(ElementKind::Text {
                    content: "Doc test".to_string(),
                    font_size: 14.0,
                    color: "#123456".to_string(),
                }),
            )
            .expect("add");

        let doc = store.scene_document(DEFAULT_SESSION);
        assert_eq!(doc.session_id, DEFAULT_SESSION);
        assert_eq!(doc.elements.len(), 1);
    }

    #[test]
    fn test_scene_document_nonexistent_returns_empty() {
        let store = SceneStore::new();
        let doc = store.scene_document("nonexistent");
        assert_eq!(doc.session_id, "nonexistent");
        assert!(doc.elements.is_empty());
    }

    #[test]
    fn test_update_session() {
        let store = SceneStore::new();
        store
            .update(DEFAULT_SESSION, |scene| {
                scene.zoom = 2.0;
                scene.pan_x = 50.0;
            })
            .expect("update");

        let scene = store.get(DEFAULT_SESSION).expect("exists");
        assert!((scene.zoom - 2.0).abs() < f32::EPSILON);
        assert!((scene.pan_x - 50.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_update_nonexistent_session_fails() {
        let store = SceneStore::new();
        let result = store.update("nonexistent", |_| {});
        assert!(matches!(result, Err(StoreError::SessionNotFound(_))));
    }

    // -----------------------------------------------------------------------
    // Persistence tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_persistence_save_and_load() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = SceneStore::with_data_dir(dir.path()).expect("store");

        // Add an element and verify it persists
        let element = Element::new(ElementKind::Text {
            content: "Persisted".to_string(),
            font_size: 20.0,
            color: "#ABCDEF".to_string(),
        });
        store.add_element(DEFAULT_SESSION, element).expect("add");

        // Load into a fresh store and verify
        let store2 = SceneStore::with_data_dir(dir.path()).expect("store2");
        store2
            .load_session_from_disk(DEFAULT_SESSION)
            .expect("load");

        let scene = store2.get(DEFAULT_SESSION).expect("session exists");
        assert_eq!(scene.element_count(), 1);
    }

    #[test]
    fn test_persistence_load_nonexistent_session() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = SceneStore::with_data_dir(dir.path()).expect("store");
        let result = store.load_session_from_disk("does-not-exist");
        assert!(result.is_err());
    }

    #[test]
    fn test_persistence_auto_save_on_mutation() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = SceneStore::with_data_dir(dir.path()).expect("store");

        // add_element triggers auto-save
        let element = Element::new(ElementKind::Text {
            content: "Auto-saved".to_string(),
            font_size: 14.0,
            color: "#000000".to_string(),
        });
        let id = store.add_element(DEFAULT_SESSION, element).expect("add");

        // Verify file exists
        let path = dir.path().join(format!("{DEFAULT_SESSION}.json"));
        assert!(path.exists(), "JSON file should be written on add_element");

        // update_element triggers auto-save
        store
            .update_element(DEFAULT_SESSION, id, |el| {
                el.transform.x = 42.0;
            })
            .expect("update");

        // Load fresh and verify
        let store2 = SceneStore::with_data_dir(dir.path()).expect("store2");
        store2
            .load_session_from_disk(DEFAULT_SESSION)
            .expect("load");
        let scene = store2.get(DEFAULT_SESSION).expect("exists");
        let elements: Vec<_> = scene.elements().collect();
        assert_eq!(elements.len(), 1);
        assert!((elements[0].transform.x - 42.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_load_all_sessions() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = SceneStore::with_data_dir(dir.path()).expect("store");

        // Create multiple sessions with elements so they get persisted
        for name in &["session-a", "session-b", "session-c"] {
            store
                .add_element(
                    name,
                    Element::new(ElementKind::Text {
                        content: format!("In {name}"),
                        font_size: 12.0,
                        color: "#000".to_string(),
                    }),
                )
                .expect("add");
        }

        let found = store.load_all_sessions().expect("list");
        assert!(found.contains(&"session-a".to_string()));
        assert!(found.contains(&"session-b".to_string()));
        assert!(found.contains(&"session-c".to_string()));
    }

    #[test]
    fn test_persistence_clear_saves() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = SceneStore::with_data_dir(dir.path()).expect("store");

        store
            .add_element(
                DEFAULT_SESSION,
                Element::new(ElementKind::Text {
                    content: "Clearable".to_string(),
                    font_size: 12.0,
                    color: "#000".to_string(),
                }),
            )
            .expect("add");

        store.clear(DEFAULT_SESSION).expect("clear");

        // Load fresh and verify cleared
        let store2 = SceneStore::with_data_dir(dir.path()).expect("store2");
        store2
            .load_session_from_disk(DEFAULT_SESSION)
            .expect("load");
        let scene = store2.get(DEFAULT_SESSION).expect("exists");
        assert!(scene.is_empty());
    }

    #[test]
    fn test_persistence_delete_session_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = SceneStore::with_data_dir(dir.path()).expect("store");

        store
            .add_element(
                DEFAULT_SESSION,
                Element::new(ElementKind::Text {
                    content: "Delete me".to_string(),
                    font_size: 12.0,
                    color: "#000".to_string(),
                }),
            )
            .expect("add");

        let path = dir.path().join(format!("{DEFAULT_SESSION}.json"));
        assert!(path.exists());

        store.delete_session_file(DEFAULT_SESSION);
        assert!(!path.exists());
    }

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("simple"), "simple");
        assert_eq!(sanitize_filename("with-dash"), "with-dash");
        assert_eq!(sanitize_filename("with_under"), "with_under");
        assert_eq!(sanitize_filename("has/slash"), "has_slash");
        assert_eq!(sanitize_filename("has space"), "has_space");
        assert_eq!(sanitize_filename("a.b.c"), "a_b_c");
    }
}
