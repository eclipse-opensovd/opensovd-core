// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

#![allow(clippy::unwrap_used, clippy::expect_used)]

//! End-to-end coverage for the SOVD data filter on the data list endpoint.
//!
//! The filter (`groups`, `categories`, `tags`) is sent as repeated query
//! parameters per ISO 17978-3 (form style, explode=true). These tests pin
//! that the server parses that wire format into the `DataFilter` the provider
//! receives, and that a malformed query still rejects cleanly.

mod common;

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use http_body_util::BodyExt;
use hyper::Request;
use opensovd_core::{
    Component, Data, DataError, DataFilter, DataProvider, DataScope, Metadata, Topology,
};

/// A data provider that records the last filter it was asked to list with.
#[derive(Clone)]
struct RecordingProvider {
    seen: Arc<Mutex<Option<DataFilter>>>,
}

#[async_trait]
impl DataProvider for RecordingProvider {
    async fn list(&self, filter: DataFilter) -> Result<Vec<Metadata>, DataError> {
        *self.seen.lock().unwrap() = Some(filter);
        Ok(Vec::new())
    }

    async fn read(&self, data_id: &str, _include_schema: bool) -> Result<Data, DataError> {
        Err(DataError::NotFound(data_id.into()))
    }

    async fn write(&self, _data_id: &str, _value: serde_json::Value) -> Result<(), DataError> {
        Err(DataError::ReadOnly)
    }
}

async fn server_with_recorder() -> (common::TestServer, Arc<Mutex<Option<DataFilter>>>) {
    let seen = Arc::new(Mutex::new(None));
    let provider = RecordingProvider {
        seen: Arc::clone(&seen),
    };
    let topology = Topology::new();
    topology
        .write()
        .await
        .add_component(Component::new("ECU", "Engine Control Unit").with_data_provider(provider));
    let server = common::TestServer::builder()
        .topology(topology)
        .build()
        .await;
    (server, seen)
}

#[tokio::test]
async fn data_list_parses_repeated_filter_keys() {
    let (server, seen) = server_with_recorder().await;
    let client = common::client();

    let request = Request::builder()
        .uri(server.url(
            "/sovd/v1/components/ECU/data\
             ?groups=sensors&groups=actuators\
             &categories=currentData&categories=x-custom\
             &tags=OBD&tags=ePTI",
        ))
        .body(http_body_util::Empty::<bytes::Bytes>::new())
        .unwrap();

    let response = client.request(request).await.unwrap();
    assert!(response.status().is_success(), "got {}", response.status());

    let filter = seen.lock().unwrap().clone().expect("provider was listed");
    // groups present, so the filter resolves to a Groups scope; categories dropped.
    assert_eq!(
        filter.scope,
        Some(DataScope::Groups(vec![
            "sensors".into(),
            "actuators".into()
        ]))
    );
    assert_eq!(filter.tags, vec!["OBD", "ePTI"]);
}

#[tokio::test]
async fn data_list_parses_categories_into_scope() {
    let (server, seen) = server_with_recorder().await;
    let client = common::client();

    let request = Request::builder()
        .uri(server.url(
            "/sovd/v1/components/ECU/data\
             ?categories=currentData&categories=x-custom&tags=OBD",
        ))
        .body(http_body_util::Empty::<bytes::Bytes>::new())
        .unwrap();

    let response = client.request(request).await.unwrap();
    assert!(response.status().is_success(), "got {}", response.status());

    let filter = seen.lock().unwrap().clone().expect("provider was listed");
    assert_eq!(
        filter.scope,
        Some(DataScope::Categories(vec![
            "currentData".into(),
            "x-custom".into()
        ]))
    );
    assert_eq!(filter.tags, vec!["OBD"]);
}

#[tokio::test]
async fn data_list_without_filter_is_empty() {
    let (server, seen) = server_with_recorder().await;
    let client = common::client();

    let request = Request::builder()
        .uri(server.url("/sovd/v1/components/ECU/data"))
        .body(http_body_util::Empty::<bytes::Bytes>::new())
        .unwrap();

    let response = client.request(request).await.unwrap();
    assert!(response.status().is_success());

    let filter = seen.lock().unwrap().clone().expect("provider was listed");
    assert!(filter.scope.is_none());
    assert!(filter.tags.is_empty());
}

#[tokio::test]
async fn data_list_groups_take_precedence_over_categories() {
    // "voltage" is currentData in group "power". groups=power selects it;
    // categories=identData would exclude it, but groups wins.
    let topology = opensovd_mocks::create_mock_topology().await;
    let server = common::TestServer::builder()
        .topology(topology)
        .build()
        .await;
    let client = common::client();

    let request = Request::builder()
        .uri(server.url("/sovd/v1/components/ecu/data?groups=power&categories=identData"))
        .body(http_body_util::Empty::<bytes::Bytes>::new())
        .unwrap();

    let response = client.request(request).await.unwrap();
    assert!(response.status().is_success(), "got {}", response.status());

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let ids: Vec<&str> = json["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, vec!["voltage"]);
}

#[tokio::test]
async fn data_list_rejects_malformed_query() {
    let (server, _seen) = server_with_recorder().await;
    let client = common::client();

    // include-schema expects a bool; a non-bool value must 400.
    let request = Request::builder()
        .uri(server.url("/sovd/v1/components/ECU/data?include-schema=maybe"))
        .body(http_body_util::Empty::<bytes::Bytes>::new())
        .unwrap();

    let response = client.request(request).await.unwrap();
    assert_eq!(response.status(), 400);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error_code"], "incomplete-request");
}
