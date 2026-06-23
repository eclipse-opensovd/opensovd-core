// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

#![allow(clippy::unwrap_used, clippy::expect_used)]

//! End-to-end coverage for the SOVD online capability description
//! `GET /{entity-path}/data/docs` returns a self-contained OpenAPI 3.1
//! specification describing how to interact with the data collection endpoint.

mod common;

use async_trait::async_trait;
use http_body_util::BodyExt;
use hyper::Request;
use opensovd_core::{
    App, Component, Data, DataError, DataFilter, DataProvider, Metadata, Topology,
};

/// Lists a fixed set of resources so tests can pin their capabilities.
#[derive(Clone)]
struct StubProvider {
    items: Vec<Metadata>,
}

#[async_trait]
impl DataProvider for StubProvider {
    async fn list(&self, _filter: DataFilter) -> Result<Vec<Metadata>, DataError> {
        Ok(self.items.clone())
    }

    async fn read(&self, data_id: &str, _include_schema: bool) -> Result<Data, DataError> {
        Err(DataError::NotFound(data_id.into()))
    }

    async fn write(&self, data_id: &str, _value: serde_json::Value) -> Result<(), DataError> {
        Err(DataError::NotFound(data_id.into()))
    }
}

fn metadata(
    id: &str,
    is_readable: bool,
    is_writable: bool,
    schema: Option<serde_json::Value>,
) -> Metadata {
    Metadata {
        id: id.into(),
        name: id.into(),
        category: "currentData".into(),
        translation_id: None,
        groups: Vec::new(),
        tags: Vec::new(),
        schema,
        is_readable,
        is_writable,
    }
}

/// A sample value schema, like a provider would attach to a resource.
fn value_schema() -> serde_json::Value {
    serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "properties": { "value": { "type": "number" } },
        "required": ["value"],
    })
}

/// Server with an "ECU" component exposing:
/// - `rpm`  : read-write, with a per-resource value schema
/// - `temp` : read-only, without a schema (exercises the opaque fallback)
async fn server() -> common::TestServer {
    let provider = StubProvider {
        items: vec![
            metadata("rpm", true, true, Some(value_schema())),
            metadata("temp", true, false, None),
        ],
    };
    let topology = Topology::new();
    topology
        .write()
        .await
        .add_component(Component::new("ECU", "Engine Control Unit").with_data_provider(provider));
    topology
        .write()
        .await
        .add_app(
            App::new("diag", "Diagnostics", "ECU").with_data_provider(StubProvider {
                items: vec![
                    metadata("rpm", true, true, Some(value_schema())),
                    metadata("temp", true, false, None),
                ],
            }),
        );
    common::TestServer::builder()
        .topology(topology)
        .build()
        .await
}

async fn get_json(
    server: &common::TestServer,
    path: &str,
) -> (hyper::StatusCode, serde_json::Value) {
    let client = common::client();
    let request = Request::builder()
        .uri(server.url(path))
        .body(http_body_util::Empty::<bytes::Bytes>::new())
        .unwrap();
    let response = client.request(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, body)
}

#[tokio::test]
async fn data_collection_docs_document_get_and_include_metadata_example() {
    let server = server().await;

    let (status, doc) = get_json(&server, "/sovd/v1/components/ECU/data/docs").await;

    assert!(status.is_success(), "got {status}");
    assert_eq!(doc["openapi"], "3.1.0");

    let path = &doc["paths"]["/components/ECU/data"];
    assert!(path["get"].is_object(), "GET must be documented");
    assert!(path["put"].is_null(), "PUT must not be documented");
    assert_eq!(path["get"]["parameters"][0]["name"], "groups");
    assert_eq!(
        path["get"]["responses"]["200"]["content"]["application/json"]["example"]["items"][0]["id"],
        "rpm"
    );
    assert_eq!(
        path["get"]["responses"]["200"]["content"]["application/json"]["example"]["items"][1]["id"],
        "temp"
    );
}

#[tokio::test]
async fn app_data_collection_docs_are_available() {
    let server = server().await;

    let (status, doc) = get_json(&server, "/sovd/v1/apps/diag/data/docs").await;

    assert!(status.is_success(), "got {status}");
    let path = &doc["paths"]["/apps/diag/data"];
    assert!(path["get"].is_object(), "GET must be documented");
    assert_eq!(
        path["get"]["responses"]["200"]["content"]["application/json"]["example"]["items"][0]["id"],
        "rpm"
    );
}

#[tokio::test]
async fn resource_level_component_docs_route_is_not_available() {
    let server = server().await;

    let (status, _) = get_json(&server, "/sovd/v1/components/ECU/data/temp/docs").await;

    assert_eq!(status, hyper::StatusCode::NOT_FOUND, "got {status}");
}

#[tokio::test]
async fn resource_level_app_docs_route_is_not_available() {
    let server = server().await;

    let (status, _) = get_json(&server, "/sovd/v1/apps/diag/data/temp/docs").await;

    assert_eq!(status, hyper::StatusCode::NOT_FOUND, "got {status}");
}

#[tokio::test]
async fn docs_for_unknown_component_is_not_found() {
    let server = server().await;

    let (status, _) = get_json(&server, "/sovd/v1/components/Nope/data/docs").await;

    assert_eq!(status, hyper::StatusCode::NOT_FOUND, "got {status}");
}
