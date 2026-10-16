// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Entity discovery: capabilities, listings and relations.
//!
//! Provides handlers for:
//! - GET / - Query for capabilities of the server
//! - GET /areas - Discover areas
//! - GET /components - Discover components
//! - GET /apps - Discover apps
//! - GET /areas/{area_id} - Query for capabilities of an area
//! - GET /areas/{area_id}/contains - Query for the entities in an area
//! - GET /components/{component_id} - Query for capabilities of a component
//! - GET /components/{component_id}/hosts - Query for the apps on a component
//! - GET /apps/{app_id} - Query for capabilities of an app

pub(super) mod app;
pub(super) mod area;
pub(super) mod component;

use std::collections::HashMap;

use axum::{extract::State, http::request::Parts};
use opensovd_core::{EntityPath, Topology};
use opensovd_models::discovery::{EntityCapabilities, EntityCapabilitiesQuery};
use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};
use serde_json::{Map, Value};

use super::error::Result;
use super::extract::{Json, Query};
use crate::schema::JsonSchema;

/// GET / - Query for capabilities of the server.
///
/// Returns the capabilities of the vehicle or system represented by the SOVD API.
pub(super) async fn server_capabilities(
    State(topology): State<Topology>,
    parts: Parts,
    Query(query): Query<EntityCapabilitiesQuery>,
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

/// The path of an entity with percent-encoded ids, such as
/// `components/Front%20Left`. Empty for the server.
pub(super) fn encode_path(path: &EntityPath) -> String {
    let mut encoded = String::new();
    for (collection, id) in path.segments() {
        if !encoded.is_empty() {
            encoded.push('/');
        }
        encoded.push_str(collection);
        encoded.push('/');
        encoded.extend(utf8_percent_encode(id, PATH_SEGMENT_ENCODE_SET));
    }
    encoded
}

#[cfg(test)]
mod tests {
    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use opensovd_core::EntityPath;
    use opensovd_mocks::create_mock_topology;
    use tower::ServiceExt;

    use super::{encode_path, encode_path_segment};

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
    fn segments_are_percent_encoded_where_they_need_it() {
        let cases = [
            ("Front Left", "Front%20Left"),
            (
                "Cabin/subcomponents/Display",
                "Cabin%2Fsubcomponents%2FDisplay",
            ),
            ("100%", "100%25"),
            ("a?b#c", "a%3Fb%23c"),
            ("user@host:1", "user%40host%3A1"),
            ("\"..\"", "%22..%22"),
            ("[a]{b}<c>|^`\\", "%5Ba%5D%7Bb%7D%3Cc%3E%7C%5E%60%5C"),
            ("\t\n", "%09%0A"),
            ("Ölstand", "%C3%96lstand"),
            ("sw.version-1_~!$&'()*+,;=", "sw.version-1_~!$&'()*+,;="),
        ];
        for (segment, encoded) in cases {
            assert_eq!(encode_path_segment(segment), encoded, "{segment:?}");
        }
    }

    #[test]
    fn every_ascii_character_encodes_as_a_segment_character() {
        for byte in 0u8..=0x7f {
            let segment = encode_path_segment(&format!("x{}x", char::from(byte)));
            assert!(
                segment
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "-._~!$&'()*+,;=%".contains(c)),
                "{byte:#04x} as {segment}"
            );
        }
    }

    #[test]
    fn paths_encode_as_collections_and_ids() {
        let comfort = EntityPath::area("Comfort").unwrap();
        let cabin = EntityPath::component("Cabin").unwrap();
        let cases: [(EntityPath, &str); 6] = [
            (EntityPath::server(), ""),
            (
                comfort.child("Media").unwrap().into(),
                "areas/Comfort/subareas/Media",
            ),
            (cabin.clone().into(), "components/Cabin"),
            (
                cabin.child("Front Left").unwrap().into(),
                "components/Cabin/subcomponents/Front%20Left",
            ),
            (EntityPath::app("a/b").unwrap().into(), "apps/a%2Fb"),
            (
                EntityPath::function("SelfTest").unwrap().into(),
                "functions/SelfTest",
            ),
        ];
        for (path, encoded) in cases {
            assert_eq!(encode_path(&path), encoded, "{path:?}");
        }
    }
}
