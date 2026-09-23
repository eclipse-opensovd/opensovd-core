// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

#![allow(clippy::unwrap_used, clippy::expect_used)]

//! End-to-end coverage for rejected data writes: the `DataError` body must
//! point at the erroneous element of the request.

mod common;

use async_trait::async_trait;
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::{Request, StatusCode};
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use opensovd_core::{Component, Data, DataError, DataFilter, DataProvider, Metadata, Topology};

/// A stand-in for a data resource holding `{"position": <u8>}`.
struct WindowProvider;

#[async_trait]
impl DataProvider for WindowProvider {
    async fn list(&self, _filter: DataFilter) -> Result<Vec<Metadata>, DataError> {
        Ok(Vec::new())
    }

    async fn read(&self, data_id: &str, _include_schema: bool) -> Result<Data, DataError> {
        Err(DataError::NotFound(data_id.into()))
    }

    async fn write(&self, _data_id: &str, value: serde_json::Value) -> Result<(), DataError> {
        let position = value.get("position").and_then(serde_json::Value::as_u64);
        if position.is_some_and(|p| p <= 255) {
            Ok(())
        } else {
            Err(DataError::InvalidValue {
                path: "/position".into(),
                message: "expected u8".into(),
            })
        }
    }
}

async fn server() -> common::TestServer {
    let topology = Topology::new();
    topology.write().await.add_component(
        Component::new("ECU", "Engine Control Unit").with_data_provider(WindowProvider),
    );
    common::TestServer::builder()
        .topology(topology)
        .build()
        .await
}

async fn put(server: &common::TestServer, body: &'static str) -> (StatusCode, serde_json::Value) {
    let client = Client::builder(TokioExecutor::new()).build_http::<Full<Bytes>>();
    let request = Request::builder()
        .method("PUT")
        .uri(server.url("/sovd/v1/components/ECU/data/window"))
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from_static(body.as_bytes())))
        .unwrap();
    let response = client.request(request).await.unwrap();
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json = if body.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&body).unwrap()
    };
    (status, json)
}

#[tokio::test]
async fn write_of_valid_value_succeeds() {
    let server = server().await;
    let (status, _) = put(&server, r#"{"data": {"position": 50}}"#).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn write_of_wrong_typed_field_points_into_value() {
    let server = server().await;
    let (status, json) = put(&server, r#"{"data": {"position": "open"}}"#).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["path"], "/data/position");
    assert_eq!(json["error"]["error_code"], "incomplete-request");
}

#[tokio::test]
async fn write_of_missing_field_points_at_it() {
    let server = server().await;
    let (status, json) = put(&server, r#"{"data": {}}"#).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["path"], "/data/position");
}

#[tokio::test]
async fn write_with_wrong_typed_signature_points_at_it() {
    let server = server().await;
    let (status, json) = put(&server, r#"{"data": {"position": 50}, "signature": 5}"#).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["path"], "/signature");
}
