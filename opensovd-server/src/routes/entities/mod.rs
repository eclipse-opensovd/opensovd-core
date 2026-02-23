// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Entity discovery routes.
//!
//! Provides routes for:
//! - `GET /` - Query capabilities of the root entity (`SOVDServer`)
//! - `GET /components` - List all components
//! - `GET /components/{component_id}` - Query capabilities of a component
//! - `GET /components/{component_id}/hosts` - List apps hosted on a component
//! - `GET /components/{component_id}/belongs-to` - Get areas containing a component
//! - `GET /apps` - List all apps
//! - `GET /apps/{app_id}` - Query capabilities of an app
//! - `GET /apps/{app_id}/is-located-on` - Get the component hosting an app
//! - `GET /apps/{app_id}/belongs-to` - Get areas containing an app
//! - `GET /areas` - List all areas
//! - `GET /areas/{area_id}` - Query capabilities of an area
//! - `GET /areas/{area_id}/contains` - List entities contained in an area

mod app;
mod area;
mod component;

use axum::{
    Router,
    extract::{Query, State},
    http::request::Parts,
    response::Json,
    routing::get,
};
use axum_extra::extract::WithRejection;
use opensovd_core::Topology;
use opensovd_models::Response;
use opensovd_models::discovery::{EntityCapabilities, EntityCapabilitiesQuery};
use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};

use super::AppState;
use super::error::{Error, Result};
use crate::schema::JsonSchema;

pub fn routes<V>() -> Router<AppState<V>>
where
    V: Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/", get(root_capabilities))
        .merge(component::routes())
        .merge(app::routes())
        .merge(area::routes())
}

/// GET / - Query for capabilities of the root entity.
///
/// Returns the capabilities of the vehicle or system represented by the SOVD API.
async fn root_capabilities(
    State(topology): State<Topology>,
    parts: Parts,
    WithRejection(Query(query), _): WithRejection<Query<EntityCapabilitiesQuery>, Error>,
) -> Result<Json<Response<EntityCapabilities>>> {
    let base = super::versioned_uri(&parts);
    let topo = topology.read().await;

    // Only include entity collection links if there are entities of that type
    let components = (topo.components().len() > 0).then(|| format!("{base}/components").into());

    let apps = (topo.apps().len() > 0).then(|| format!("{base}/apps").into());

    let areas = (topo.areas().len() > 0).then(|| format!("{base}/areas").into());

    Ok(Json(Response {
        data: EntityCapabilities {
            id: String::new(),   // Empty for SOVDServer
            name: String::new(), // Empty for SOVDServer
            components,
            apps,
            areas,
            ..Default::default()
        },
        schema: query.include_schema.then(EntityCapabilities::schema),
    }))
}

/// Characters that must be percent-encoded in URI path segments.
/// Based on RFC 3986 - encodes everything except unreserved characters.
const PATH_SEGMENT_ENCODE_SET: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'<')
    .add(b'>')
    .add(b'?')
    .add(b'`')
    .add(b'{')
    .add(b'}')
    .add(b'/')
    .add(b'%')
    .add(b'[')
    .add(b']')
    .add(b'@')
    .add(b':');

/// Percent-encode a string for use in a URI path segment.
pub(super) fn encode_path_segment(s: &str) -> String {
    utf8_percent_encode(s, PATH_SEGMENT_ENCODE_SET).to_string()
}
