// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Entity types.

use std::fmt;

pub use app::App;
pub use area::Area;
pub use component::Component;

mod app;
mod area;
mod component;

/// The kind of entity in the SOVD hierarchy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntityKind {
    Component,
    App,
    Area,
}

impl fmt::Display for EntityKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Component => f.write_str("component"),
            Self::App => f.write_str("app"),
            Self::Area => f.write_str("area"),
        }
    }
}

/// A lightweight reference to an entity by kind and ID.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EntityRef {
    kind: EntityKind,
    id: String,
}

impl EntityRef {
    /// Creates an `EntityRef` for a top-level component.
    #[must_use]
    pub fn component(id: impl Into<String>) -> Self {
        Self {
            kind: EntityKind::Component,
            id: id.into(),
        }
    }

    /// Creates an `EntityRef` for an app.
    #[must_use]
    pub fn app(id: impl Into<String>) -> Self {
        Self {
            kind: EntityKind::App,
            id: id.into(),
        }
    }

    /// Creates an `EntityRef` for a top-level area.
    #[must_use]
    pub fn area(id: impl Into<String>) -> Self {
        Self {
            kind: EntityKind::Area,
            id: id.into(),
        }
    }

    /// Returns the entity kind.
    #[must_use]
    pub const fn kind(&self) -> EntityKind {
        self.kind
    }

    /// Returns the entity ID string.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }
}

impl fmt::Display for EntityRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} '{}'", self.kind, self.id)
    }
}

/// A collection that can hold different entity types.
#[derive(Debug, Default)]
pub struct EntityCollection {
    pub components: Vec<Component>,
    pub apps: Vec<App>,
    pub areas: Vec<Area>,
}

impl EntityCollection {
    /// Creates a collection with all entity types.
    #[must_use]
    pub const fn new(components: Vec<Component>, apps: Vec<App>, areas: Vec<Area>) -> Self {
        Self {
            components,
            apps,
            areas,
        }
    }

    /// Adds a component to the collection.
    pub fn add_component(&mut self, component: Component) {
        self.components.push(component);
    }

    /// Adds an app to the collection.
    pub fn add_app(&mut self, app: App) {
        self.apps.push(app);
    }

    /// Adds an area to the collection.
    pub fn add_area(&mut self, area: Area) {
        self.areas.push(area);
    }

    /// Collects entity references from all entities in the collection.
    #[must_use]
    pub fn entity_refs(&self) -> Vec<EntityRef> {
        let mut refs = Vec::new();
        refs.extend(self.components.iter().map(|e| e.entity_ref().clone()));
        refs.extend(self.apps.iter().map(|e| e.entity_ref().clone()));
        refs.extend(self.areas.iter().map(|e| e.entity_ref().clone()));
        refs
    }
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::*;

    #[test]
    fn test_component_entity() {
        let component = Component::new("test-id", "Test Component");

        assert_eq!(component.id(), "test-id");
        assert_eq!(component.name(), "Test Component");
        assert!(component.tags().is_empty());
        assert!(component.metadata().is_empty());
    }

    #[test]
    fn entity_ref_constructors() {
        let comp = EntityRef::component("ECU1");
        assert_eq!(comp.kind(), EntityKind::Component);
        assert_eq!(comp.id(), "ECU1");

        let app = EntityRef::app("diag-app");
        assert_eq!(app.kind(), EntityKind::App);
        assert_eq!(app.id(), "diag-app");

        let area = EntityRef::area("zone-front");
        assert_eq!(area.kind(), EntityKind::Area);
        assert_eq!(area.id(), "zone-front");
    }

    #[test]
    fn entity_ref_equality() {
        assert_eq!(EntityRef::component("a"), EntityRef::component("a"));
        assert_ne!(EntityRef::component("a"), EntityRef::component("b"));
        assert_ne!(EntityRef::component("a"), EntityRef::app("a"));
    }

    #[test]
    fn entity_kind_display() {
        assert_eq!(EntityKind::Component.to_string(), "component");
        assert_eq!(EntityKind::App.to_string(), "app");
        assert_eq!(EntityKind::Area.to_string(), "area");
    }

    #[test]
    fn entity_ref_display() {
        assert_eq!(EntityRef::component("ECU1").to_string(), "component 'ECU1'");
        assert_eq!(EntityRef::app("diag").to_string(), "app 'diag'");
        assert_eq!(EntityRef::area("zone").to_string(), "area 'zone'");
    }

    #[test]
    fn component_entity_ref() {
        let component = Component::new("ecu1", "ECU 1");
        let eid = component.entity_ref();
        assert_eq!(eid.kind(), EntityKind::Component);
        assert_eq!(eid.id(), "ecu1");
    }

    #[test]
    fn app_entity_ref() {
        let app = App::new("diag", "Diagnostics", "ecu1");
        let eid = app.entity_ref();
        assert_eq!(eid.kind(), EntityKind::App);
        assert_eq!(eid.id(), "diag");
    }

    #[test]
    fn area_entity_ref() {
        let area = Area::new("zone-front", "Front Zone");
        let eid = area.entity_ref();
        assert_eq!(eid.kind(), EntityKind::Area);
        assert_eq!(eid.id(), "zone-front");
    }
}
