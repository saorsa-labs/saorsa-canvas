//! Canonical serialized representation for scenes shared across MCP, WebSocket, and web client.

use serde::{Deserialize, Serialize};

use crate::{Element, ElementId, ElementKind, Scene, Transform};

/// Document-friendly element description.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElementDocument {
    /// Element identifier.
    pub id: String,
    /// Element content.
    pub kind: ElementKind,
    /// Transform metadata.
    #[serde(default = "ElementDocument::default_transform")]
    pub transform: Transform,
    /// Interactivity flag.
    #[serde(default = "ElementDocument::default_interactive")]
    pub interactive: bool,
    /// Selection flag.
    #[serde(default)]
    pub selected: bool,
}

impl From<&Element> for ElementDocument {
    fn from(element: &Element) -> Self {
        Self {
            id: element.id.to_string(),
            kind: element.kind.clone(),
            transform: element.transform,
            interactive: element.interactive,
            selected: element.selected,
        }
    }
}

impl ElementDocument {
    fn default_transform() -> Transform {
        Transform::default()
    }

    const fn default_interactive() -> bool {
        true
    }

    /// Convert document to runtime element.
    ///
    /// # Errors
    ///
    /// Returns error string if the element id is not a valid UUID.
    pub fn into_element(self) -> Result<Element, String> {
        let mut element = Element::new(self.kind).with_transform(self.transform);
        element.interactive = self.interactive;
        element.selected = self.selected;
        let id = ElementId::parse(&self.id).map_err(|e| e.to_string())?;
        element.id = id;
        Ok(element)
    }
}

/// Viewport information.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ViewportDocument {
    /// Width in pixels.
    pub width: f32,
    /// Height in pixels.
    pub height: f32,
    /// Zoom level.
    #[serde(default = "ViewportDocument::default_zoom")]
    pub zoom: f32,
    /// Horizontal pan offset.
    #[serde(default)]
    pub pan_x: f32,
    /// Vertical pan offset.
    #[serde(default)]
    pub pan_y: f32,
}

impl ViewportDocument {
    const fn default_zoom() -> f32 {
        1.0
    }
}

impl From<&Scene> for ViewportDocument {
    fn from(scene: &Scene) -> Self {
        Self {
            width: scene.viewport_width,
            height: scene.viewport_height,
            zoom: scene.zoom,
            pan_x: scene.pan_x,
            pan_y: scene.pan_y,
        }
    }
}

/// Canonical scene document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneDocument {
    /// Scene identifier/session.
    pub session_id: String,
    /// Viewport metadata.
    pub viewport: ViewportDocument,
    /// Elements in z-order order.
    pub elements: Vec<ElementDocument>,
    /// Timestamp in milliseconds.
    pub timestamp: u64,
}

impl SceneDocument {
    /// Build a document from a runtime scene.
    pub fn from_scene(session_id: impl Into<String>, scene: &Scene, timestamp: u64) -> Self {
        let mut elements: Vec<_> = scene.elements().map(ElementDocument::from).collect();
        elements.sort_by_key(|doc| doc.transform.z_index);
        Self {
            session_id: session_id.into(),
            viewport: ViewportDocument::from(scene),
            elements,
            timestamp,
        }
    }

    /// Apply this document to a scene (overwriting current data).
    ///
    /// # Errors
    ///
    /// Returns error string if any element cannot be materialized.
    pub fn into_scene(self) -> Result<Scene, String> {
        let mut scene = Scene::new(self.viewport.width, self.viewport.height);
        scene.zoom = self.viewport.zoom;
        scene.pan_x = self.viewport.pan_x;
        scene.pan_y = self.viewport.pan_y;

        for element_doc in self.elements {
            let element = element_doc.into_element()?;
            scene.add_element(element);
        }

        Ok(scene)
    }
}
