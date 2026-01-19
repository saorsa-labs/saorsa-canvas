//! Shared scene storage for multi-component access.
//!
//! Provides a thread-safe [`SceneStore`] that can be shared across MCP handlers,
//! WebSocket connections, and HTTP routes for consistent scene state management.

use std::collections::HashMap;
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
}

impl SceneStore {
    /// Create a new store with a default session.
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
        }
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
        let mut scenes = self
            .scenes
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        scenes.insert(session_id.to_string(), scene);
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
        let mut scenes = self
            .scenes
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let scene = scenes
            .get_mut(session_id)
            .ok_or_else(|| StoreError::SessionNotFound(session_id.to_string()))?;
        f(scene);
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
        let mut scenes = self
            .scenes
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let scene = scenes
            .entry(session_id.to_string())
            .or_insert_with(|| Scene::new(DEFAULT_WIDTH, DEFAULT_HEIGHT));
        let id = scene.add_element(element);
        Ok(id)
    }

    /// Remove an element from a session's scene.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::SessionNotFound`] if the session does not exist.
    /// Returns [`StoreError::ElementNotFound`] if the element does not exist.
    pub fn remove_element(&self, session_id: &str, id: ElementId) -> Result<(), StoreError> {
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

    /// Clear all elements from a session's scene.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::SessionNotFound`] if the session does not exist.
    pub fn clear(&self, session_id: &str) -> Result<(), StoreError> {
        let mut scenes = self
            .scenes
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let scene = scenes
            .get_mut(session_id)
            .ok_or_else(|| StoreError::SessionNotFound(session_id.to_string()))?;
        scene.clear();
        Ok(())
    }
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
}
