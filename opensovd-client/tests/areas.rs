// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

mod common;

use common::mock_client;
use mock_http_connector::Connector;
use serde_json::json;

#[tokio::test]
async fn list_areas() {
    let mut builder = Connector::builder();
    builder
        .expect()
        .with_uri("http://localhost/sovd/v1/areas")
        .returning(json!({"items": []}).to_string())
        .unwrap();
    let client = mock_client(builder.build());
    let result = client.list_areas().send().await.unwrap();
    assert!(result.data.items.is_empty());
}

#[tokio::test]
async fn area_contains() {
    let mut builder = Connector::builder();
    builder
        .expect()
        .with_uri("http://localhost/sovd/v1/areas/powertrain/contains")
        .returning(json!({"items": []}).to_string())
        .unwrap();
    let client = mock_client(builder.build());
    let result = client.area("powertrain").contains().await.unwrap();
    assert!(result.items.is_empty());
}

#[tokio::test]
async fn list_areas_with_schema() {
    let mut builder = Connector::builder();
    builder
        .expect()
        .with_uri("http://localhost/sovd/v1/areas?include-schema=true")
        .returning(json!({"items": [], "schema": {"type": "object"}}).to_string())
        .unwrap();
    let client = mock_client(builder.build());
    let result = client.list_areas().schema(true).send().await.unwrap();
    assert!(result.data.items.is_empty());
    assert_eq!(result.schema.unwrap(), json!({"type": "object"}));
}

#[tokio::test]
async fn area_capabilities_resolve_relative_links() {
    let mut builder = Connector::builder();
    builder
        .expect()
        .with_uri("http://localhost/sovd/v1/areas/powertrain")
        .returning(
            json!({
                "id": "powertrain",
                "name": "Powertrain",
                "contains": "/sovd/v1/domains/powertrain/contains"
            })
            .to_string(),
        )
        .unwrap();
    builder
        .expect()
        .with_uri("http://localhost/sovd/v1/domains/powertrain/contains")
        .returning(json!({"items": []}).to_string())
        .unwrap();
    let client = mock_client(builder.build());

    let area = client.area("powertrain").capabilities().await.unwrap();
    let result = area.contains().await.unwrap();
    assert!(result.items.is_empty());
}
