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
        .with_uri("http://localhost/sovd/v1/components/ecu1/belongs-to")
        .returning(json!({"items": []}).to_string())
        .unwrap();
    let client = mock_client(builder.build());
    let result = client.component("ecu1").belongs_to().await.unwrap();
    assert!(result.items.is_empty());
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

#[tokio::test]
async fn component_default_links() {
    let client = mock_client(Connector::builder().build());
    let links = client.component("ecu1").links().clone();
    assert_eq!(links.id, "ecu1");
    assert_eq!(
        links.data.unwrap().0,
        "http://localhost/sovd/v1/components/ecu1/data"
    );
    assert_eq!(
        links.hosts.unwrap().0,
        "http://localhost/sovd/v1/components/ecu1/hosts"
    );
}

#[tokio::test]
async fn component_capabilities_follow_advertised_links() {
    let mut builder = Connector::builder();
    builder
        .expect()
        .with_uri("http://localhost/sovd/v1/components/ecu1")
        .returning(
            json!({
                "id": "ecu1",
                "name": "Engine ECU",
                "data": "http://localhost/sovd/v1/ecus/ecu1/data"
            })
            .to_string(),
        )
        .unwrap();
    builder
        .expect()
        .with_uri("http://localhost/sovd/v1/ecus/ecu1/data")
        .returning(json!({"items": []}).to_string())
        .unwrap();
    builder
        .expect()
        .with_uri("http://localhost/sovd/v1/ecus/ecu1/data/voltage")
        .returning(json!({"id": "voltage", "data": 12.6}).to_string())
        .unwrap();
    builder
        .expect()
        .with_uri("http://localhost/sovd/v1/components/ecu1/hosts")
        .returning(json!({"items": []}).to_string())
        .unwrap();
    let client = mock_client(builder.build());

    let ecu = client.component("ecu1").capabilities().await.unwrap();
    assert_eq!(ecu.links().name, "Engine ECU");

    ecu.list_data().send().await.unwrap();
    let read = ecu.data("voltage").read().send().await.unwrap();
    assert_eq!(read.id, "voltage");
    // Not advertised: keeps the default link.
    ecu.hosts().await.unwrap();
}

#[tokio::test]
async fn component_query_capabilities_with_schema() {
    let mut builder = Connector::builder();
    builder
        .expect()
        .with_uri("http://localhost/sovd/v1/components/ecu1?include-schema=true")
        .returning(
            json!({"id": "ecu1", "name": "Engine ECU", "schema": {"type": "object"}}).to_string(),
        )
        .unwrap();
    let client = mock_client(builder.build());
    let result = client
        .component("ecu1")
        .query_capabilities()
        .schema(true)
        .send()
        .await
        .unwrap();
    assert!(result.data.hosts.is_none());
    assert_eq!(result.schema.unwrap(), json!({"type": "object"}));
}
