// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! The entity a route addresses.

use std::sync::Arc;

use axum::extract::{FromRequestParts, State};
use axum::http::{Uri, request::Parts};
use opensovd_core::{
    App, Area, BulkDataProvider, Component, DataProvider, EntityKind, EntityPath, InvalidPath,
    LogProvider, Topology, TopologyReadGuard,
};
use serde::Deserialize;

use super::discovery::encode_path;
use super::error::{Error, Result};

/// The entity the route is nested at, and the request path below the API
/// version for error messages. A param that is not UTF-8 answers 400 and an
/// invalid id `resource-not-found`.
pub(crate) struct Entity {
    path: EntityPath,
    uri: Uri,
    /// Where the path below the API version starts in `uri`.
    start: usize,
}

/// Whether the entity exists. A nested entity is not served yet, so only its
/// parent is checked, and the topology holds no functions to check.
fn exists(path: &EntityPath, topo: &TopologyReadGuard<'_>) -> bool {
    if let Some(parent) = path.parent() {
        return exists(&parent, topo);
    }
    match path.kind() {
        EntityKind::Server | EntityKind::Function => true,
        EntityKind::Area => topo.get_area(path.id()).is_ok(),
        EntityKind::Component => topo.get_component(path.id()).is_ok(),
        EntityKind::App => topo.get_app(path.id()).is_ok(),
    }
}

/// The route params that name the entity. The server's routes name none.
#[derive(Deserialize)]
#[expect(clippy::struct_field_names)]
struct Params {
    subarea_id: Option<String>,
    subcomponent_id: Option<String>,
    area_id: Option<String>,
    component_id: Option<String>,
    app_id: Option<String>,
    function_id: Option<String>,
}

/// The entity the route params name, and the server without any of them.
fn entity_path(params: Params) -> std::result::Result<EntityPath, InvalidPath> {
    let Params {
        subarea_id,
        subcomponent_id,
        area_id,
        component_id,
        app_id,
        function_id,
    } = params;
    let path = if let Some(area) = area_id {
        let area = EntityPath::area(area)?;
        match subarea_id {
            Some(subarea) => area.child(subarea)?.into(),
            None => area.into(),
        }
    } else if let Some(component) = component_id {
        let component = EntityPath::component(component)?;
        match subcomponent_id {
            Some(subcomponent) => component.child(subcomponent)?.into(),
            None => component.into(),
        }
    } else if let Some(app) = app_id {
        EntityPath::app(app)?.into()
    } else if let Some(function) = function_id {
        EntityPath::function(function)?.into()
    } else {
        EntityPath::server()
    };
    Ok(path)
}

impl<S: Send + Sync> FromRequestParts<S> for Entity {
    type Rejection = Error;

    #[expect(
        clippy::disallowed_types,
        reason = "reads the params and rejects itself"
    )]
    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self> {
        let uri = super::original_uri(parts).clone();
        let start = uri
            .path()
            .len()
            .saturating_sub(super::api_path(parts).len());
        let params =
            Option::<axum::extract::Path<Params>>::from_request_parts(parts, state).await?;
        let path = params.map_or(Ok(EntityPath::server()), |axum::extract::Path(params)| {
            entity_path(params)
        });
        let Ok(path) = path else {
            return Err(Error::ResourceNotFound(super::api_path(parts).to_owned()));
        };
        Ok(Self { path, uri, start })
    }
}

impl Entity {
    /// The path of the entity the route addresses.
    pub(crate) const fn path(&self) -> &EntityPath {
        &self.path
    }

    /// The id of a top-level entity of `kind`, the only ones served yet.
    fn top_level(&self, kind: EntityKind) -> Option<&str> {
        (self.path.kind() == kind && self.path.parent().is_none()).then(|| self.path.id())
    }

    /// The area the route names: `entity-not-found` if it does not exist.
    /// The routes of a subarea are not served yet.
    pub(crate) fn area<'t>(&self, topo: &'t TopologyReadGuard<'_>) -> Result<&'t Area> {
        match self.top_level(EntityKind::Area) {
            Some(id) => topo.get_area(id).map_err(|_| self.entity_not_found()),
            None => Err(self.unserved(topo)),
        }
    }

    /// The component the route names, like [`Entity::area`].
    pub(crate) fn component<'t>(&self, topo: &'t TopologyReadGuard<'_>) -> Result<&'t Component> {
        match self.top_level(EntityKind::Component) {
            Some(id) => topo.get_component(id).map_err(|_| self.entity_not_found()),
            None => Err(self.unserved(topo)),
        }
    }

    /// The app the route names, like [`Entity::area`].
    pub(crate) fn app<'t>(&self, topo: &'t TopologyReadGuard<'_>) -> Result<&'t App> {
        match self.top_level(EntityKind::App) {
            Some(id) => topo.get_app(id).map_err(|_| self.entity_not_found()),
            None => Err(self.unserved(topo)),
        }
    }

    /// The data provider of a component or app, see [`Entity::provider`].
    pub(crate) fn data_provider<'t>(
        &self,
        topo: &'t TopologyReadGuard<'_>,
    ) -> Result<&'t dyn DataProvider> {
        self.provider(topo, Component::data_provider, App::data_provider)
    }

    /// The bulk-data provider of a component or app, see
    /// [`Entity::provider`].
    pub(crate) fn bulk_data_provider(
        &self,
        topo: &TopologyReadGuard<'_>,
    ) -> Result<Arc<dyn BulkDataProvider>> {
        self.provider(topo, Component::bulkdata_provider, App::bulkdata_provider)
    }

    /// The log provider of a component or app, see [`Entity::provider`].
    pub(crate) fn log_provider(
        &self,
        topo: &TopologyReadGuard<'_>,
    ) -> Result<Arc<dyn LogProvider>> {
        self.provider(topo, Component::log_provider, App::log_provider)
    }

    /// The provider `component` or `app` returns for the entity:
    /// `entity-not-found` if it does not exist, `resource-not-found` if it
    /// has none. The server and nested entities serve no providers yet.
    fn provider<'t, P>(
        &self,
        topo: &'t TopologyReadGuard<'_>,
        component: impl FnOnce(&'t Component) -> Option<P>,
        app: impl FnOnce(&'t App) -> Option<P>,
    ) -> Result<P> {
        let provider = match self.path.kind() {
            EntityKind::Component => component(self.component(topo)?),
            EntityKind::App => app(self.app(topo)?),
            _ => return Err(self.unserved(topo)),
        };
        provider.ok_or_else(|| self.not_found())
    }

    /// The answer when the topology does not hold the entity.
    fn entity_not_found(&self) -> Error {
        Error::EntityNotFound(encode_path(&self.path))
    }

    /// The request path below the API version.
    fn resource(&self) -> &str {
        self.uri.path().get(self.start..).unwrap_or_default()
    }

    fn not_found(&self) -> Error {
        Error::ResourceNotFound(self.resource().to_owned())
    }

    /// The answer of a route that is not served yet: `entity-not-found` if
    /// the entity does not exist, `resource-not-found` otherwise.
    fn unserved(&self, topo: &TopologyReadGuard<'_>) -> Error {
        if exists(&self.path, topo) {
            self.not_found()
        } else {
            self.entity_not_found()
        }
    }
}

/// Answers a route that is not served yet: `entity-not-found` if the entity
/// does not exist, `resource-not-found` otherwise.
pub(crate) async fn stub(State(topology): State<Topology>, entity: Entity) -> Error {
    entity.unserved(&topology.read().await)
}

#[cfg(test)]
mod tests {
    use opensovd_core::{EntityPath, InvalidPath};
    use serde_json::{Map, Value};

    use super::{Params, entity_path};

    fn params(pairs: &[(&str, &str)]) -> Params {
        let map: Map<String, Value> = pairs
            .iter()
            .map(|(name, id)| ((*name).to_owned(), (*id).into()))
            .collect();
        serde_json::from_value(map.into()).unwrap()
    }

    #[test]
    fn without_an_entity_param_the_server() {
        let server = Ok(EntityPath::server());
        assert_eq!(entity_path(params(&[])), server);
        assert_eq!(entity_path(params(&[("data_id", "vin")])), server);
    }

    #[test]
    fn each_kind_names_its_top_level_entity() {
        let cases: [(&str, EntityPath); 4] = [
            ("area_id", EntityPath::area("Comfort").unwrap().into()),
            (
                "component_id",
                EntityPath::component("Cabin").unwrap().into(),
            ),
            ("app_id", EntityPath::app("Radio").unwrap().into()),
            (
                "function_id",
                EntityPath::function("SelfTest").unwrap().into(),
            ),
        ];
        for (name, path) in cases {
            let found = entity_path(params(&[(name, path.id()), ("data_id", "vin")]));
            assert_eq!(found, Ok(path), "{name}");
        }
    }

    #[test]
    fn a_sub_entity_nests_below_its_parent() {
        let media = EntityPath::area("Comfort").unwrap().child("Media").unwrap();
        assert_eq!(
            entity_path(params(&[("area_id", "Comfort"), ("subarea_id", "Media")])),
            Ok(media.into())
        );
        let display = EntityPath::component("Cabin")
            .unwrap()
            .child("Display")
            .unwrap();
        assert_eq!(
            entity_path(params(&[
                ("component_id", "Cabin"),
                ("subcomponent_id", "Display")
            ])),
            Ok(display.into())
        );
    }

    #[test]
    fn invalid_ids_are_rejected() {
        for id in ["", ".", ".."] {
            let invalid = Err(InvalidPath::Id(id.to_owned()));
            assert_eq!(entity_path(params(&[("app_id", id)])), invalid);
            assert_eq!(
                entity_path(params(&[
                    ("component_id", id),
                    ("subcomponent_id", "Display")
                ])),
                invalid
            );
            assert_eq!(
                entity_path(params(&[
                    ("component_id", "Cabin"),
                    ("subcomponent_id", id)
                ])),
                invalid
            );
        }
    }
}
