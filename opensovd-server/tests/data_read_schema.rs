// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

#![allow(clippy::unwrap_used, clippy::expect_used)]

//! The schema of a data read follows `include-schema`, not the provider.

mod common;

use async_trait::async_trait;
use http_body_util::BodyExt;
use hyper::Request;
use opensovd_core::{Component, Data, DataError, DataFilter, DataProvider, Metadata, Topology};
use serde_json::{Value, json};

/// A data provider that always answers with the same schema, asked or not.
struct FixedProvider {
    schema: Option<Value>,
}

#[async_trait]
impl DataProvider for FixedProvider {
    async fn list(&self, _filter: DataFilter) -> Result<Vec<Metadata>, DataError> {
        Ok(Vec::new())
    }

    async fn read(&self, _data_id: &str, _include_schema: bool) -> Result<Data, DataError> {
        Ok(Data {
            data: json!({ "value": 1 }),
            schema: self.schema.clone(),
        })
    }

    async fn write(&self, _data_id: &str, _value: Value) -> Result<(), DataError> {
        Err(DataError::ReadOnly)
    }
}

async fn read(schema: Option<Value>, query: &str) -> Value {
    let topology = Topology::new();
    topology.write().await.add_component(
        Component::new("Climate", "Climate").with_data_provider(FixedProvider { schema }),
    );
    let server = common::TestServer::builder()
        .topology(topology)
        .build()
        .await;

    let request = Request::builder()
        .uri(server.url(&format!("/sovd/v1/components/Climate/data/level{query}")))
        .body(http_body_util::Empty::<bytes::Bytes>::new())
        .unwrap();
    let response = common::client().request(request).await.unwrap();
    assert!(response.status().is_success(), "got {}", response.status());
    let body = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&body).unwrap()
}

#[tokio::test]
async fn read_without_value_schema_still_describes_the_response() {
    let body = read(None, "?include-schema=true").await;
    assert_eq!(body.pointer("/schema/properties/data"), Some(&json!(true)));
    assert_eq!(
        body.pointer("/schema/properties/id"),
        Some(&json!({ "type": "string" }))
    );
}

#[tokio::test]
async fn read_without_include_schema_has_no_schema() {
    let body = read(Some(json!({ "type": "object" })), "").await;
    assert!(body.get("schema").is_none());
}
