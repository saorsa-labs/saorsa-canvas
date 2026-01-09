//! Scene graph for managing canvas elements.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::{CanvasError, CanvasResult, Element, ElementId};

/// A scene containing all canvas elements.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Scene {
    /// All elements in the scene, indexed by ID.
    elements: HashMap<ElementId, Element>,
    /// Root-level element IDs (not children of any group).
    root_elements: Vec<ElementId>,
    /// Currently selected element IDs.
    selected: Vec<ElementId>,
    /// Viewport width in pixels.
    pub viewport_width: f32,
    /// Viewport height in pixels.
    pub viewport_height: f32,
    /// Current zoom level (1.0 = 100%).
    pub zoom: f32,
    /// Pan offset X.
    pub pan_x: f32,
    /// Pan offset Y.
    pub pan_y: f32,
}

impl Scene {
    /// Create a new empty scene with the given viewport size.
    #[must_use]
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            elements: HashMap::new(),
            root_elements: Vec::new(),
            selected: Vec::new(),
            viewport_width: width,
            viewport_height: height,
            zoom: 1.0,
            pan_x: 0.0,
            pan_y: 0.0,
        }
    }

    /// Add an element to the scene.
    pub fn add_element(&mut self, element: Element) -> ElementId {
        let id = element.id;
        if element.parent.is_none() {
            self.root_elements.push(id);
        }
        self.elements.insert(id, element);
        id
    }

    /// Remove an element from the scene.
    ///
    /// # Errors
    ///
    /// Returns an error if the element is not found.
    pub fn remove_element(&mut self, id: &ElementId) -> CanvasResult<Element> {
        self.root_elements.retain(|&eid| eid != *id);
        self.selected.retain(|&eid| eid != *id);
        self.elements
            .remove(id)
            .ok_or_else(|| CanvasError::ElementNotFound(id.to_string()))
    }

    /// Get an element by ID.
    #[must_use]
    pub fn get_element(&self, id: ElementId) -> Option<&Element> {
        self.elements.get(&id)
    }

    /// Get a mutable reference to an element by ID.
    pub fn get_element_mut(&mut self, id: ElementId) -> Option<&mut Element> {
        self.elements.get_mut(&id)
    }

    /// Get all elements in the scene.
    pub fn elements(&self) -> impl Iterator<Item = &Element> {
        self.elements.values()
    }

    /// Get mutable references to all elements in the scene.
    pub fn elements_mut(&mut self) -> impl Iterator<Item = &mut Element> {
        self.elements.values_mut()
    }

    /// Get root-level elements (not children of groups).
    pub fn root_elements(&self) -> impl Iterator<Item = &Element> {
        self.root_elements
            .iter()
            .filter_map(|id| self.elements.get(id))
    }

    /// Set the viewport dimensions.
    pub fn set_viewport(&mut self, width: f32, height: f32) {
        self.viewport_width = width;
        self.viewport_height = height;
    }

    /// Find the element at the given canvas coordinates.
    /// Returns the ID of the topmost (highest z-index) interactive element.
    #[must_use]
    pub fn element_at(&self, x: f32, y: f32) -> Option<ElementId> {
        // Transform screen coordinates to canvas coordinates
        let canvas_x = (x - self.pan_x) / self.zoom;
        let canvas_y = (y - self.pan_y) / self.zoom;

        // Find all elements containing this point
        let mut hits: Vec<_> = self
            .elements
            .values()
            .filter(|e| e.interactive && e.contains_point(canvas_x, canvas_y))
            .collect();

        // Sort by z-index (highest first)
        hits.sort_by(|a, b| b.transform.z_index.cmp(&a.transform.z_index));

        hits.first().map(|e| e.id)
    }

    /// Select an element.
    ///
    /// # Errors
    ///
    /// Returns an error if the element is not found.
    pub fn select(&mut self, id: ElementId) -> CanvasResult<()> {
        if let Some(element) = self.elements.get_mut(&id) {
            element.selected = true;
            if !self.selected.contains(&id) {
                self.selected.push(id);
            }
            Ok(())
        } else {
            Err(CanvasError::ElementNotFound(id.to_string()))
        }
    }

    /// Deselect all elements.
    pub fn deselect_all(&mut self) {
        for id in &self.selected {
            if let Some(element) = self.elements.get_mut(id) {
                element.selected = false;
            }
        }
        self.selected.clear();
    }

    /// Get currently selected elements.
    pub fn selected_elements(&self) -> impl Iterator<Item = &Element> {
        self.selected.iter().filter_map(|id| self.elements.get(id))
    }

    /// Get the number of elements in the scene.
    #[must_use]
    pub fn element_count(&self) -> usize {
        self.elements.len()
    }

    /// Check if the scene is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }

    /// Serialize the scene to JSON.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization fails.
    pub fn to_json(&self) -> CanvasResult<String> {
        serde_json::to_string(self).map_err(CanvasError::Serialization)
    }

    /// Deserialize a scene from JSON.
    ///
    /// # Errors
    ///
    /// Returns an error if deserialization fails.
    pub fn from_json(json: &str) -> CanvasResult<Self> {
        serde_json::from_str(json).map_err(CanvasError::Serialization)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ElementKind, Transform};

    #[test]
    fn test_scene_add_remove() {
        let mut scene = Scene::new(800.0, 600.0);
        assert!(scene.is_empty());

        let element = Element::new(ElementKind::Text {
            content: "Hello".to_string(),
            font_size: 16.0,
            color: "#000000".to_string(),
        });
        let id = scene.add_element(element);

        assert_eq!(scene.element_count(), 1);
        assert!(scene.get_element(id).is_some());

        scene.remove_element(&id).expect("should remove");
        assert!(scene.is_empty());
    }

    #[test]
    fn test_element_at() {
        let mut scene = Scene::new(800.0, 600.0);

        let element = Element::new(ElementKind::Text {
            content: "Test".to_string(),
            font_size: 16.0,
            color: "#000000".to_string(),
        })
        .with_transform(Transform {
            x: 100.0,
            y: 100.0,
            width: 200.0,
            height: 50.0,
            rotation: 0.0,
            z_index: 0,
        });

        scene.add_element(element);

        // Point inside element
        assert!(scene.element_at(150.0, 125.0).is_some());

        // Point outside element
        assert!(scene.element_at(50.0, 50.0).is_none());
    }
}
