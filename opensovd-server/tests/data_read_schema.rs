// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

#![allow(clippy::unwrap_used, clippy::expect_used)]

//! The schema of a data read describes the whole response and follows
//! `include-schema`, not the provider.

mod common;

use async_trait::async_trait;
use http_body_util::BodyExt;
use hyper::Request;
use opensovd_core::{Component, Data, DataError, DataFilter, DataProvider, Metadata, Topology};
use opensovd_models::data::DataCategory;
use opensovd_providers::data::{Constant, DataProviderBuilder};
use schemars::JsonSchema;
use serde::Serialize;
use serde_json::{Value, json};

/// A data provider that always answers with the same schema, asked or not.
struct FixedProvider {
    schema: Option<Value>,
}

#[async_trait]
impl DataProvider for FixedProvider {
    async fn data_list(&self, _filter: DataFilter) -> Result<Vec<Metadata>, DataError> {
        Ok(Vec::new())
    }

    async fn data_read(&self, _data_id: &str, _include_schema: bool) -> Result<Data, DataError> {
        Ok(Data {
            data: json!({ "value": 1 }),
            schema: self.schema.clone(),
        })
    }

    async fn data_write(&self, _data_id: &str, _value: Value) -> Result<(), DataError> {
        Err(DataError::ReadOnly)
    }
}

/// Blower fan level.
#[derive(Serialize, JsonSchema)]
struct FanLevel {
    level: u8,
    mode: Mode,
}

#[derive(Serialize, JsonSchema)]
#[expect(dead_code, reason = "only appears in the schema")]
enum Mode {
    Auto,
    Manual,
}

async fn get(topology: Topology, path: &str) -> Value {
    let server = common::TestServer::builder()
        .topology(topology)
        .build()
        .await;
    let request = Request::builder()
        .uri(server.url(path))
        .body(http_body_util::Empty::<bytes::Bytes>::new())
        .unwrap();
    let response = common::client().request(request).await.unwrap();
    assert!(response.status().is_success(), "got {}", response.status());
    let body = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&body).unwrap()
}

async fn read_fixed(schema: Option<Value>, query: &str) -> Value {
    let topology = Topology::new();
    topology.write().await.add_component(
        Component::new("Climate", "Climate").with_data_provider(FixedProvider { schema }),
    );
    get(
        topology,
        &format!("/sovd/v1/components/Climate/data/level{query}"),
    )
    .await
}

async fn read_climate(data_id: &str) -> Value {
    let category = DataCategory::CurrentData;
    let provider = DataProviderBuilder::new()
        .read_data(
            "fan",
            "Fan",
            &category,
            Constant::new(FanLevel {
                level: 3,
                mode: Mode::Auto,
            })
            .unwrap(),
        )
        .build()
        .unwrap();
    let topology = Topology::new();
    topology
        .write()
        .await
        .add_component(Component::new("Climate", "Climate").with_data_provider(provider));
    get(
        topology,
        &format!("/sovd/v1/components/Climate/data/{data_id}?include-schema=true"),
    )
    .await
}

fn validator(body: &Value) -> jsonschema::Validator {
    jsonschema::draft202012::new(body.get("schema").expect("schema")).unwrap()
}

/// Assert that `body` validates against its own schema, and that changing its
/// data with `break_data` makes it fail.
fn assert_schema_holds(body: &Value, break_data: impl FnOnce(&mut Value)) {
    let validator = validator(body);
    assert!(validator.is_valid(body), "{body}");
    let mut broken = body.clone();
    break_data(broken.get_mut("data").expect("data"));
    assert!(!validator.is_valid(&broken), "{broken}");
}

#[tokio::test]
async fn read_without_value_schema_still_describes_the_response() {
    let body = read_fixed(None, "?include-schema=true").await;
    assert_eq!(body.pointer("/schema/properties/data"), Some(&json!(true)));
    assert!(validator(&body).is_valid(&body), "{body}");
}

#[tokio::test]
async fn read_without_include_schema_has_no_schema() {
    let body = read_fixed(Some(json!({ "type": "object" })), "").await;
    assert!(body.get("schema").is_none());
}

#[tokio::test]
async fn struct_value_matches_its_schema() {
    let body = read_climate("fan").await;
    assert_schema_holds(&body, |data| data["value"]["mode"] = json!("Turbo"));
}
