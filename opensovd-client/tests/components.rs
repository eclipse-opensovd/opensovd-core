// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

mod common;

use common::mock_client;
use mock_http_connector::Connector;
use serde_json::json;

#[tokio::test]
async fn list_components() {
    let mut builder = Connector::builder();
    builder
        .expect()
        .with_uri("http://localhost/sovd/v1/components")
        .returning(json!({"items": []}).to_string())
        .unwrap();
    let client = mock_client(builder.build());
    let result = client.list_components().send().await.unwrap();
    assert!(result.data.items.is_empty());
}

#[tokio::test]
async fn component_hosts() {
    let mut builder = Connector::builder();
    builder
        .expect()
        .with_uri("http://localhost/sovd/v1/components/ecu1/hosts")
        .returning(json!({"items": []}).to_string())
        .unwrap();
    let client = mock_client(builder.build());
    let result = client.component("ecu1").hosts().await.unwrap();
    assert!(result.items.is_empty());
}

#[tokio::test]
async fn component_belongs_to() {
    let mut builder = Connector::builder();
    builder
        .expect()
        .with_uri("http://localhost/sovd/v1/components/ecu1")
        .returning(
            json!({
                "id": "ecu1",
                "name": "ECU 1",
                "belongs-to": "http://localhost/sovd/v1/areas/body"
            })
            .to_string(),
        )
        .unwrap();
    builder
        .expect()
        .with_uri("http://localhost/sovd/v1/areas/body")
        .returning(json!({"id": "body", "name": "Body"}).to_string())
        .unwrap();
    let client = mock_client(builder.build());
    let area = client
        .component("ecu1")
        .belongs_to()
        .await
        .unwrap()
        .unwrap();
    assert_eq!(area.id, "body");
}

#[tokio::test]
async fn component_without_area() {
    let mut builder = Connector::builder();
    builder
        .expect()
        .with_uri("http://localhost/sovd/v1/components/ecu1")
        .returning(json!({"id": "ecu1", "name": "ECU 1"}).to_string())
        .unwrap();
    let client = mock_client(builder.build());
    assert!(
        client
            .component("ecu1")
            .belongs_to()
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn list_components_with_schema() {
    let mut builder = Connector::builder();
    builder
        .expect()
        .with_uri("http://localhost/sovd/v1/components?include-schema=true")
        .returning(json!({"items": [], "schema": {"type": "object"}}).to_string())
        .unwrap();
    let client = mock_client(builder.build());
    let result = client.list_components().schema(true).send().await.unwrap();
    assert!(result.data.items.is_empty());
    assert_eq!(result.schema.unwrap(), json!({"type": "object"}));
}
