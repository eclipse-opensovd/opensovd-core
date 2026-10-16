// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Entity discovery routes.
//!
//! Provides routes for:
//! - `GET /` - Query capabilities of the root entity (`SOVDServer`)
//! - `GET /components` - List all components
//! - `GET /components/{component_id}` - Query capabilities of a component
//! - `GET /components/{component_id}/hosts` - List apps hosted on a component
//! - `GET /apps` - List all apps
//! - `GET /apps/{app_id}` - Query capabilities of an app
//! - `GET /areas` - List all areas
//! - `GET /areas/{area_id}` - Query capabilities of an area
//! - `GET /areas/{area_id}/contains` - List entities contained in an area

mod app;
mod area;
mod component;

use std::collections::HashMap;

use axum::{Router, extract::State, http::request::Parts, response::Json, routing::get};
use axum_extra::extract::{Query, WithRejection};
use opensovd_core::Topology;
use opensovd_models::discovery::{EntityCapabilities, EntityCapabilitiesQuery};
use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};
use serde_json::{Map, Value};

use super::AppState;
use super::error::{Error, Result};
use crate::schema::JsonSchema;

pub fn routes<V>() -> Router<AppState<V>>
where
    V: Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/", get(server_capabilities))
        .merge(component::routes())
        .merge(app::routes())
        .merge(area::routes())
}

/// GET / - Query for capabilities of the root entity.
///
/// Returns the capabilities of the vehicle or system represented by the SOVD API.
async fn server_capabilities(
    State(topology): State<Topology>,
    parts: Parts,
    WithRejection(Query(query), _): WithRejection<Query<EntityCapabilitiesQuery>, Error>,
) -> Result<Json<EntityCapabilities>> {
    let base = super::versioned_uri(&parts);
    let topo = topology.read().await;

    // Only include entity collection links if there are entities of that type
    let components = (topo.components().len() > 0).then(|| format!("{base}/components").into());

    let apps = (topo.apps().len() > 0).then(|| format!("{base}/apps").into());

    let areas = (topo.areas().len() > 0).then(|| format!("{base}/areas").into());

    Ok(Json(EntityCapabilities {
        id: String::new(),   // Empty for SOVDServer
        name: String::new(), // Empty for SOVDServer
        components,
        apps,
        areas,
        schema: query.include_schema.then(EntityCapabilities::schema),
        ..Default::default()
    }))
}

/// The variant of an entity with metadata, each value as a list of one string.
fn variant(metadata: &HashMap<String, String>) -> Option<Map<String, Value>> {
    (!metadata.is_empty()).then(|| {
        metadata
            .iter()
            .map(|(key, value)| (key.clone(), Value::from(vec![value.clone()])))
            .collect()
    })
}

/// Characters percent-encoded in a URI path segment: all that RFC 3986 does
/// not allow there, plus `:` and `@`.
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
    .add(b'\\')
    .add(b'^')
    .add(b'|')
    .add(b'@')
    .add(b':');

/// Percent-encode a string for use in a URI path segment.
pub(super) fn encode_path_segment(s: &str) -> String {
    utf8_percent_encode(s, PATH_SEGMENT_ENCODE_SET).to_string()
}

#[cfg(test)]
mod tests {
    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use opensovd_mocks::create_mock_topology;
    use tower::ServiceExt;

    use super::encode_path_segment;

    #[tokio::test]
    async fn listings_reject_a_malformed_query_as_json() {
        let app = crate::routes::test_router(create_mock_topology().await);

        for path in [
            "/sovd/v1/areas",
            "/sovd/v1/areas/powertrain/contains",
            "/sovd/v1/apps",
            "/sovd/v1/components",
            "/sovd/v1/components/ecu/hosts",
        ] {
            let request = Request::builder()
                .uri(format!("{path}?include-schema=maybe"))
                .body(Body::empty())
                .unwrap();
            let response = app.clone().oneshot(request).await.unwrap();
            assert_eq!(response.status(), 400, "{path}");

            let body = response.into_body().collect().await.unwrap().to_bytes();
            let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(json["error_code"], "incomplete-request", "{path}");
        }
    }

    #[test]
    fn encode_path_segment_keeps_only_path_characters() {
        assert_eq!(encode_path_segment("a^b|c\\d"), "a%5Eb%7Cc%5Cd");
        assert_eq!(encode_path_segment("a/b c:d"), "a%2Fb%20c%3Ad");
        assert_eq!(
            encode_path_segment("sw.version-1_~!$&'()*+,;="),
            "sw.version-1_~!$&'()*+,;="
        );
    }
}
