// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Entity topology management.

use std::collections::HashMap;
use std::ops::Deref;
use std::sync::Arc;

use indexmap::IndexMap;
use indexmap::IndexSet;
use indexmap::map::Values;
use tokio::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard, broadcast};

use crate::entity::{App, Area, Component, EntityRef};

/// Events emitted when the topology changes.
#[derive(Debug, Clone)]
pub enum TopologyEvent {
    /// An entity was added to the topology.
    Added(EntityRef),
    /// An entity was removed from the topology.
    Removed(EntityRef),
}

/// A `Result` alias where the `Err` variant is [`TopologyError`].
pub type Result<T> = std::result::Result<T, TopologyError>;

#[derive(Debug, thiserror::Error)]
pub enum TopologyError {
    #[error("{0} not found")]
    NotFound(EntityRef),
}

pub struct TopologyState {
    components: IndexMap<String, Component>,
    apps: IndexMap<String, App>,
    areas: IndexMap<String, Area>,
    /// Component ID -> set of app IDs hosted on it
    apps_by_component: HashMap<String, IndexSet<String>>,
    /// Area ID -> set of component IDs in that area
    components_by_area: HashMap<String, IndexSet<String>>,
    /// Area ID -> set of app IDs in that area
    apps_by_area: HashMap<String, IndexSet<String>>,
}

impl TopologyState {
    fn new() -> Self {
        Self {
            components: IndexMap::new(),
            apps: IndexMap::new(),
            areas: IndexMap::new(),
            apps_by_component: HashMap::new(),
            components_by_area: HashMap::new(),
            apps_by_area: HashMap::new(),
        }
    }

    fn insert_component(&mut self, id: String, component: Component) {
        if let Some(area) = component.area_id() {
            self.components_by_area
                .entry(area.to_owned())
                .or_default()
                .insert(id.clone());
        }
        self.components.insert(id, component);
    }

    fn remove_component(&mut self, id: &str) -> Option<Component> {
        let removed = self.components.shift_remove(id)?;
        if let Some(area) = removed.area_id()
            && let Some(set) = self.components_by_area.get_mut(area)
        {
            set.shift_remove(id);
            if set.is_empty() {
                self.components_by_area.remove(area);
            }
        }
        Some(removed)
    }

    fn insert_app(&mut self, id: String, app: App) {
        if let Some(comp) = app.component_id() {
            self.apps_by_component
                .entry(comp.to_owned())
                .or_default()
                .insert(id.clone());
        }
        if let Some(area) = app.area_id() {
            self.apps_by_area
                .entry(area.to_owned())
                .or_default()
                .insert(id.clone());
        }
        self.apps.insert(id, app);
    }

    fn remove_app(&mut self, id: &str) -> Option<App> {
        let removed = self.apps.shift_remove(id)?;
        if let Some(comp) = removed.component_id()
            && let Some(set) = self.apps_by_component.get_mut(comp)
        {
            set.shift_remove(id);
            if set.is_empty() {
                self.apps_by_component.remove(comp);
            }
        }
        if let Some(area) = removed.area_id()
            && let Some(set) = self.apps_by_area.get_mut(area)
        {
            set.shift_remove(id);
            if set.is_empty() {
                self.apps_by_area.remove(area);
            }
        }
        Some(removed)
    }

    fn insert_area(&mut self, id: String, area: Area) {
        self.areas.insert(id, area);
    }

    fn remove_area(&mut self, id: &str) -> Option<Area> {
        let removed = self.areas.shift_remove(id)?;
        self.components_by_area.remove(id);
        self.apps_by_area.remove(id);
        Some(removed)
    }

    /// Gets a component by ID.
    ///
    /// # Errors
    ///
    /// Returns [`TopologyError::NotFound`] if the component does not exist.
    pub fn get_component(&self, id: &str) -> Result<&Component> {
        self.components
            .get(id)
            .ok_or_else(|| TopologyError::NotFound(EntityRef::component(id)))
    }

    /// Lists all components.
    #[must_use]
    pub fn components(&self) -> Values<'_, String, Component> {
        self.components.values()
    }

    /// Gets an app by ID.
    ///
    /// # Errors
    ///
    /// Returns [`TopologyError::NotFound`] if the app does not exist.
    pub fn get_app(&self, id: &str) -> Result<&App> {
        self.apps
            .get(id)
            .ok_or_else(|| TopologyError::NotFound(EntityRef::app(id)))
    }

    /// Lists all apps.
    #[must_use]
    pub fn apps(&self) -> Values<'_, String, App> {
        self.apps.values()
    }

    /// Queries all apps hosted on a specific component.
    pub fn apps_of_component(&self, component_id: &str) -> impl Iterator<Item = &App> {
        self.apps_by_component
            .get(component_id)
            .into_iter()
            .flatten()
            .filter_map(|id| self.apps.get(id))
    }

    /// Gets an area by ID.
    ///
    /// # Errors
    ///
    /// Returns [`TopologyError::NotFound`] if the area does not exist.
    pub fn get_area(&self, id: &str) -> Result<&Area> {
        self.areas
            .get(id)
            .ok_or_else(|| TopologyError::NotFound(EntityRef::area(id)))
    }

    /// Lists all areas.
    #[must_use]
    pub fn areas(&self) -> Values<'_, String, Area> {
        self.areas.values()
    }

    /// Queries all components contained in a specific area.
    pub fn components_of_area(&self, area_id: &str) -> impl Iterator<Item = &Component> {
        self.components_by_area
            .get(area_id)
            .into_iter()
            .flatten()
            .filter_map(|id| self.components.get(id))
    }

    /// Queries all apps contained in a specific area.
    pub fn apps_of_area(&self, area_id: &str) -> impl Iterator<Item = &App> {
        self.apps_by_area
            .get(area_id)
            .into_iter()
            .flatten()
            .filter_map(|id| self.apps.get(id))
    }

    /// Gets the component that hosts a given app (the "is-located-on" relationship).
    ///
    /// # Errors
    ///
    /// Returns [`TopologyError::NotFound`] if the app does not exist.
    pub fn component_of_app(&self, app_id: &str) -> Result<Option<&Component>> {
        let app = self
            .apps
            .get(app_id)
            .ok_or_else(|| TopologyError::NotFound(EntityRef::app(app_id)))?;
        let Some(component_id) = app.component_id() else {
            return Ok(None);
        };
        Ok(self.components.get(component_id))
    }

    /// Gets the area that contains a given component (the "belongs-to" relationship).
    ///
    /// # Errors
    ///
    /// Returns [`TopologyError::NotFound`] if the component does not exist.
    pub fn area_of_component(&self, component_id: &str) -> Result<Option<&Area>> {
        let component = self
            .components
            .get(component_id)
            .ok_or_else(|| TopologyError::NotFound(EntityRef::component(component_id)))?;
        let Some(area_id) = component.area_id() else {
            return Ok(None);
        };
        Ok(self.areas.get(area_id))
    }

    /// Gets the area that contains a given app (the "belongs-to" relationship).
    ///
    /// # Errors
    ///
    /// Returns [`TopologyError::NotFound`] if the app does not exist.
    pub fn area_of_app(&self, app_id: &str) -> Result<Option<&Area>> {
        let app = self
            .apps
            .get(app_id)
            .ok_or_else(|| TopologyError::NotFound(EntityRef::app(app_id)))?;
        let Some(area_id) = app.area_id() else {
            return Ok(None);
        };
        Ok(self.areas.get(area_id))
    }
}

struct TopologyInner {
    state: RwLock<TopologyState>,
    events: broadcast::Sender<TopologyEvent>,
}

/// A read guard over the topology state.
///
/// Holds a single read lock for the duration of its lifetime, ensuring
/// consistent reads across multiple queries. All read-only topology
/// methods are available on this guard.
pub struct TopologyReadGuard<'a>(RwLockReadGuard<'a, TopologyState>);

impl Deref for TopologyReadGuard<'_> {
    type Target = TopologyState;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// A write guard over the topology state.
///
/// Holds a single write lock for the duration of its lifetime. Mutations
/// are applied immediately while topology events are batched and emitted
/// when the guard is dropped.
pub struct TopologyWriteGuard<'a> {
    state: RwLockWriteGuard<'a, TopologyState>,
    events: &'a broadcast::Sender<TopologyEvent>,
    pending: Vec<TopologyEvent>,
}

impl Deref for TopologyWriteGuard<'_> {
    type Target = TopologyState;

    fn deref(&self) -> &Self::Target {
        &self.state
    }
}

impl TopologyWriteGuard<'_> {
    /// Adds a component. Overwrites an existing entry with the same ID.
    pub fn add_component(&mut self, component: Component) {
        let entity_ref = EntityRef::component(component.id());
        self.state
            .insert_component(component.id().to_owned(), component);
        self.pending.push(TopologyEvent::Added(entity_ref));
    }

    /// Adds an app. Overwrites an existing entry with the same ID.
    pub fn add_app(&mut self, app: App) {
        let entity_ref = EntityRef::app(app.id());
        self.state.insert_app(app.id().to_owned(), app);
        self.pending.push(TopologyEvent::Added(entity_ref));
    }

    /// Adds an area. Overwrites an existing entry with the same ID.
    pub fn add_area(&mut self, area: Area) {
        let entity_ref = EntityRef::area(area.id());
        self.state.insert_area(area.id().to_owned(), area);
        self.pending.push(TopologyEvent::Added(entity_ref));
    }

    /// Removes a component by ID. Missing IDs are silently skipped.
    pub fn remove_component(&mut self, id: &str) {
        if self.state.remove_component(id).is_some() {
            self.pending
                .push(TopologyEvent::Removed(EntityRef::component(id)));
        }
    }

    /// Removes an app by ID. Missing IDs are silently skipped.
    pub fn remove_app(&mut self, id: &str) {
        if self.state.remove_app(id).is_some() {
            self.pending
                .push(TopologyEvent::Removed(EntityRef::app(id)));
        }
    }

    /// Removes an area by ID. Missing IDs are silently skipped.
    pub fn remove_area(&mut self, id: &str) {
        if self.state.remove_area(id).is_some() {
            self.pending
                .push(TopologyEvent::Removed(EntityRef::area(id)));
        }
    }
}

impl Drop for TopologyWriteGuard<'_> {
    fn drop(&mut self) {
        for event in self.pending.drain(..) {
            let _ = self.events.send(event);
        }
    }
}

/// Default capacity for the topology event broadcast channel.
const DEFAULT_EVENT_CHANNEL_CAPACITY: usize = 64;

/// Thread-safe topology holding a set of components.
#[derive(Clone)]
pub struct Topology(Arc<TopologyInner>);

impl Topology {
    /// Creates an empty topology with the default event channel capacity.
    #[must_use]
    pub fn new() -> Self {
        Self::with_event_capacity(DEFAULT_EVENT_CHANNEL_CAPACITY)
    }

    /// Creates an empty topology with the given event channel capacity.
    ///
    /// The capacity controls how many [`TopologyEvent`] values the broadcast
    /// channel can buffer before slow receivers start lagging.
    #[must_use]
    pub fn with_event_capacity(capacity: usize) -> Self {
        let (events, _) = broadcast::channel(capacity);
        Self(Arc::new(TopologyInner {
            state: RwLock::new(TopologyState::new()),
            events,
        }))
    }

    /// Subscribes to topology change events.
    ///
    /// Returns a receiver that yields [`TopologyEvent`] values whenever
    /// entities are added or removed.
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<TopologyEvent> {
        self.0.events.subscribe()
    }

    /// Acquires a read guard over the topology state.
    ///
    /// The returned [`TopologyReadGuard`] holds a single read lock, ensuring
    /// consistent reads across multiple queries for the duration of its
    /// lifetime.
    pub async fn read(&self) -> TopologyReadGuard<'_> {
        TopologyReadGuard(self.0.state.read().await)
    }

    /// Acquires a write guard over the topology state.
    ///
    /// The returned [`TopologyWriteGuard`] holds a single write lock.
    /// Mutations are applied immediately while topology events are batched
    /// and flushed when [`TopologyWriteGuard::flush_events`] is called or
    /// the guard is dropped.
    pub async fn write(&self) -> TopologyWriteGuard<'_> {
        let guard = self.0.state.write().await;
        TopologyWriteGuard {
            state: guard,
            events: &self.0.events,
            pending: Vec::new(),
        }
    }
}

impl Default for Topology {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::*;
    use crate::entity::{Component, EntityKind};

    #[tokio::test]
    async fn test_topology_empty() {
        let topology = Topology::default();
        assert_eq!(topology.read().await.components().len(), 0);
    }

    #[tokio::test]
    async fn test_topology_get_component() {
        let topology = Topology::new();
        topology
            .write()
            .await
            .add_component(Component::new("ecu", "ECU"));

        let topo = topology.read().await;
        let entity = topo.get_component("ecu");
        assert!(entity.is_ok());
        assert_eq!(entity.unwrap().id(), "ecu");
    }

    #[tokio::test]
    async fn test_topology_get_nonexistent() {
        let topology = Topology::default();
        assert!(topology.read().await.get_component("nonexistent").is_err());
    }

    #[tokio::test]
    async fn test_topology_list() {
        let topology = Topology::new();
        {
            let mut t = topology.write().await;
            t.add_component(Component::new("ecu1", "ECU 1"));
            t.add_component(Component::new("ecu2", "ECU 2"));
        }

        assert_eq!(topology.read().await.components().len(), 2);
    }

    #[tokio::test]
    async fn test_topology_clone() {
        let topology = Topology::new();
        topology
            .write()
            .await
            .add_component(Component::new("ecu", "ECU"));

        assert_eq!(
            topology.read().await.get_component("ecu").unwrap().id(),
            "ecu"
        );
    }

    #[tokio::test]
    async fn test_topology_subscribe_add_remove() {
        let topology = Topology::default();
        let mut rx = topology.subscribe();

        topology
            .write()
            .await
            .add_component(Component::new("ecu1", "ECU 1"));

        let event = rx.try_recv().unwrap();
        match &event {
            TopologyEvent::Added(entity_ref) => {
                assert_eq!(entity_ref.kind(), EntityKind::Component);
                assert_eq!(entity_ref.id(), "ecu1");
            }
            TopologyEvent::Removed(_) => panic!("expected Added event"),
        }

        topology.write().await.remove_component("ecu1");

        let event = rx.try_recv().unwrap();
        match &event {
            TopologyEvent::Removed(entity_ref) => {
                assert_eq!(entity_ref.kind(), EntityKind::Component);
                assert_eq!(entity_ref.id(), "ecu1");
            }
            TopologyEvent::Added(_) => panic!("expected Removed event"),
        }
    }

    #[tokio::test]
    async fn test_add_areas() {
        use crate::entity::Area;

        let topology = Topology::default();
        {
            let mut t = topology.write().await;
            t.add_area(Area::new("powertrain", "Powertrain Domain"));
            t.add_area(Area::new("network", "Network Domain"));
        }

        let topo = topology.read().await;
        assert_eq!(topo.areas().len(), 2);
        assert_eq!(topo.get_area("powertrain").unwrap().id(), "powertrain");
        assert_eq!(topo.get_area("network").unwrap().id(), "network");
    }

    #[tokio::test]
    async fn test_remove_areas() {
        use crate::entity::Area;

        let topology = Topology::new();
        {
            let mut t = topology.write().await;
            t.add_area(Area::new("powertrain", "Powertrain Domain"));
            t.add_area(Area::new("network", "Network Domain"));
        }

        topology.write().await.remove_area("powertrain");

        let topo = topology.read().await;
        assert_eq!(topo.areas().len(), 1);
        assert!(topo.get_area("powertrain").is_err());
    }

    #[tokio::test]
    async fn test_add_overwrites_existing() {
        use crate::entity::Area;

        let topology = Topology::new();
        topology
            .write()
            .await
            .add_area(Area::new("powertrain", "Powertrain Domain"));

        // Adding again with same ID overwrites
        topology
            .write()
            .await
            .add_area(Area::new("powertrain", "Powertrain v2"));

        let topo = topology.read().await;
        assert_eq!(topo.areas().len(), 1);
        assert_eq!(topo.get_area("powertrain").unwrap().name(), "Powertrain v2");
    }

    #[tokio::test]
    async fn test_remove_skips_missing() {
        use crate::entity::Area;

        let topology = Topology::new();
        {
            let mut t = topology.write().await;
            t.add_area(Area::new("powertrain", "Powertrain Domain"));
            t.add_area(Area::new("network", "Network Domain"));
        }

        // Remove one existing and one missing (should skip missing)
        {
            let mut t = topology.write().await;
            t.remove_area("powertrain");
            t.remove_area("nonexistent");
            t.add_area(Area::new("chassis", "Chassis"));
        }

        // powertrain was removed, nonexistent was skipped, chassis was added
        let topo = topology.read().await;
        assert_eq!(topo.areas().len(), 2);
        let ids: Vec<&str> = topo.areas().map(Area::id).collect();
        assert!(ids.contains(&"network"));
        assert!(ids.contains(&"chassis"));
    }

    #[tokio::test]
    async fn test_app_component_existing_app() {
        let topology = Topology::new();
        {
            let mut t = topology.write().await;
            t.add_component(Component::new("ecu1", "ECU 1"));
            t.add_app(App::new("app1", "App 1", "ecu1"));
        }

        let topo = topology.read().await;
        let result = topo.component_of_app("app1").unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().id(), "ecu1");
    }

    #[tokio::test]
    async fn test_app_component_nonexistent_app() {
        let topology = Topology::default();
        let topo = topology.read().await;
        let result = topo.component_of_app("nonexistent");
        assert!(matches!(result, Err(TopologyError::NotFound(_))));
    }

    #[tokio::test]
    async fn test_component_area() {
        use crate::entity::Area;

        let topology = Topology::new();
        {
            let mut t = topology.write().await;
            t.add_area(Area::new("powertrain", "Powertrain"));
            t.add_component(Component::new("ecu1", "ECU 1").with_area_id("powertrain"));
        }

        let topo = topology.read().await;
        let result = topo.area_of_component("ecu1").unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().id(), "powertrain");
    }

    #[tokio::test]
    async fn test_component_area_no_area() {
        let topology = Topology::new();
        topology
            .write()
            .await
            .add_component(Component::new("ecu1", "ECU 1"));

        let topo = topology.read().await;
        let result = topo.area_of_component("ecu1").unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_app_area() {
        use crate::entity::Area;

        let topology = Topology::new();
        {
            let mut t = topology.write().await;
            t.add_area(Area::new("powertrain", "Powertrain"));
            t.add_app(App::new("app1", "App 1", "ecu1").with_area_id("powertrain"));
        }

        let topo = topology.read().await;
        let result = topo.area_of_app("app1").unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().id(), "powertrain");
    }

    #[tokio::test]
    async fn test_component_area_nonexistent() {
        let topology = Topology::default();
        let topo = topology.read().await;
        let result = topo.area_of_component("nonexistent");
        assert!(matches!(result, Err(TopologyError::NotFound(_))));
    }

    #[tokio::test]
    async fn test_remove_area_cleans_component_index() {
        use crate::entity::Area;

        let topology = Topology::new();
        {
            let mut t = topology.write().await;
            t.add_area(Area::new("powertrain", "Powertrain"));
            t.add_component(Component::new("ecu1", "ECU 1").with_area_id("powertrain"));
        }

        assert_eq!(
            topology
                .read()
                .await
                .components_of_area("powertrain")
                .count(),
            1
        );

        topology.write().await.remove_area("powertrain");

        assert_eq!(
            topology
                .read()
                .await
                .components_of_area("powertrain")
                .count(),
            0
        );
    }

    #[tokio::test]
    async fn test_remove_area_cleans_app_index() {
        use crate::entity::Area;

        let topology = Topology::new();
        {
            let mut t = topology.write().await;
            t.add_area(Area::new("network", "Network"));
            t.add_app(App::new("diag", "Diagnostics", "gw").with_area_id("network"));
        }

        assert_eq!(topology.read().await.apps_of_area("network").count(), 1);

        topology.write().await.remove_area("network");

        assert_eq!(topology.read().await.apps_of_area("network").count(), 0);
    }

    #[tokio::test]
    async fn test_write_guard_read_through_deref() {
        let topology = Topology::new();
        let mut t = topology.write().await;
        t.add_component(Component::new("ecu1", "ECU 1"));
        assert_eq!(t.get_component("ecu1").unwrap().id(), "ecu1");
        assert_eq!(t.components().len(), 1);
    }

    #[tokio::test]
    async fn test_remove_area_then_reinsert_no_stale_data() {
        use crate::entity::Area;

        let topology = Topology::new();
        {
            let mut t = topology.write().await;
            t.add_area(Area::new("powertrain", "Powertrain"));
            t.add_component(Component::new("ecu1", "ECU 1").with_area_id("powertrain"));
        }

        // Remove the area
        topology.write().await.remove_area("powertrain");

        // Re-insert area with same ID but no components assigned to it
        topology
            .write()
            .await
            .add_area(Area::new("powertrain", "Powertrain v2"));

        // Should not inherit stale component index
        assert_eq!(
            topology
                .read()
                .await
                .components_of_area("powertrain")
                .count(),
            0
        );
    }
}
