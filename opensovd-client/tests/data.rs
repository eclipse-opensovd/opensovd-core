// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

mod common;

use common::mock_client;
use mock_http_connector::Connector;
use opensovd_client::DataCategory;
use serde_json::json;

#[tokio::test]
async fn list_component_data() {
    let mut builder = Connector::builder();
    builder
        .expect()
        .with_uri("http://localhost/sovd/v1/components/ecu1/data")
        .returning(json!({"items": []}).to_string())
        .unwrap();
    let client = mock_client(builder.build());
    let result = client.component("ecu1").list_data().send().await.unwrap();
    assert!(result.data.items.is_empty());
}

#[tokio::test]
async fn data_categories() {
    let mut builder = Connector::builder();
    builder
        .expect()
        .with_uri("http://localhost/sovd/v1/components/ecu1/data-categories")
        .returning(json!({"items": []}).to_string())
        .unwrap();
    let client = mock_client(builder.build());
    let result = client.component("ecu1").data_categories().await.unwrap();
    assert!(result.items.is_empty());
}

#[tokio::test]
async fn data_groups() {
    let mut builder = Connector::builder();
    builder
        .expect()
        .with_uri("http://localhost/sovd/v1/components/ecu1/data-groups")
        .returning(json!({"items": []}).to_string())
        .unwrap();
    let client = mock_client(builder.build());
    let result = client.component("ecu1").data_groups().await.unwrap();
    assert!(result.items.is_empty());
}

#[tokio::test]
async fn read_data() {
    let mut builder = Connector::builder();
    builder
        .expect()
        .with_uri("http://localhost/sovd/v1/components/ecu1/data/voltage")
        .returning(
            json!({
                "id": "voltage",
                "data": {"value": 12.6}
            })
            .to_string(),
        )
        .unwrap();
    let client = mock_client(builder.build());
    let result = client
        .component("ecu1")
        .data("voltage")
        .read()
        .send()
        .await
        .unwrap();
    assert_eq!(result.id, "voltage");
}

#[tokio::test]
async fn write_data() {
    let mut builder = Connector::builder();
    builder
        .expect()
        .with_method("PUT")
        .with_uri("http://localhost/sovd/v1/components/ecu1/data/param1")
        .returning("")
        .unwrap();
    let client = mock_client(builder.build());
    client
        .component("ecu1")
        .data("param1")
        .write(&json!({"value": 42}))
        .unwrap()
        .send()
        .await
        .unwrap();
}

#[tokio::test]
async fn app_data() {
    let mut builder = Connector::builder();
    builder
        .expect()
        .with_uri("http://localhost/sovd/v1/apps/diag/data")
        .returning(json!({"items": []}).to_string())
        .unwrap();
    let client = mock_client(builder.build());
    let result = client.app("diag").list_data().send().await.unwrap();
    assert!(result.data.items.is_empty());
}

#[tokio::test]
async fn read_data_with_schema() {
    let mut builder = Connector::builder();
    builder
        .expect()
        .with_uri("http://localhost/sovd/v1/components/ecu1/data/voltage?include-schema=true")
        .returning(
            json!({
                "id": "voltage",
                "data": {"value": 12.6},
                "schema": {"type": "number"}
            })
            .to_string(),
        )
        .unwrap();
    let client = mock_client(builder.build());
    let result = client
        .component("ecu1")
        .data("voltage")
        .read()
        .schema(true)
        .send()
        .await
        .unwrap();
    assert_eq!(result.id, "voltage");
    assert_eq!(result.schema.unwrap(), json!({"type": "number"}));
}

#[tokio::test]
async fn list_data_with_schema() {
    let mut builder = Connector::builder();
    builder
        .expect()
        .with_uri("http://localhost/sovd/v1/components/ecu1/data?include-schema=true")
        .returning(json!({"items": [], "schema": {"type": "object"}}).to_string())
        .unwrap();
    let client = mock_client(builder.build());
    let result = client
        .component("ecu1")
        .list_data()
        .schema(true)
        .send()
        .await
        .unwrap();
    assert!(result.data.items.is_empty());
    assert_eq!(result.schema.unwrap(), json!({"type": "object"}));
}

#[tokio::test]
async fn list_data_with_groups() {
    let mut builder = Connector::builder();
    builder
        .expect()
        .with_uri("http://localhost/sovd/v1/components/ecu1/data?groups=a&groups=b")
        .returning(json!({"items": []}).to_string())
        .unwrap();
    let client = mock_client(builder.build());
    let result = client
        .component("ecu1")
        .list_data()
        .groups(["a", "b"])
        .send()
        .await
        .unwrap();
    assert!(result.data.items.is_empty());
}

#[tokio::test]
async fn list_data_with_categories() {
    let mut builder = Connector::builder();
    builder
        .expect()
        .with_uri(
            "http://localhost/sovd/v1/components/ecu1/data?categories=identData&categories=currentData",
        )
        .returning(json!({"items": []}).to_string())
        .unwrap();
    let client = mock_client(builder.build());
    let result = client
        .component("ecu1")
        .list_data()
        .categories([DataCategory::IdentData, DataCategory::CurrentData])
        .send()
        .await
        .unwrap();
    assert!(result.data.items.is_empty());
}

#[tokio::test]
async fn list_data_with_all_filters() {
    let mut builder = Connector::builder();
    builder
        .expect()
        .with_uri(
            "http://localhost/sovd/v1/components/ecu1/data?groups=x&categories=storedData&tags=urgent&include-schema=true",
        )
        .returning(json!({"items": [], "schema": {"type": "object"}}).to_string())
        .unwrap();
    let client = mock_client(builder.build());
    let result = client
        .component("ecu1")
        .list_data()
        .groups(["x"])
        .categories([DataCategory::StoredData])
        .tags(["urgent"])
        .schema(true)
        .send()
        .await
        .unwrap();
    assert!(result.data.items.is_empty());
    assert_eq!(result.schema.unwrap(), json!({"type": "object"}));
}

#[tokio::test]
async fn list_data_with_tags() {
    let mut builder = Connector::builder();
    builder
        .expect()
        .with_uri("http://localhost/sovd/v1/apps/diag/data?tags=safety&tags=critical")
        .returning(json!({"items": []}).to_string())
        .unwrap();
    let client = mock_client(builder.build());
    let result = client
        .app("diag")
        .list_data()
        .tags(["safety", "critical"])
        .send()
        .await
        .unwrap();
    assert!(result.data.items.is_empty());
}

#[tokio::test]
async fn list_data_percent_encodes_filter_values() {
    let mut builder = Connector::builder();
    builder
        .expect()
        .with_uri(
            "http://localhost/sovd/v1/components/ecu1/data?groups=engine%20control&tags=a%26b",
        )
        .returning(json!({"items": []}).to_string())
        .unwrap();
    let client = mock_client(builder.build());
    let result = client
        .component("ecu1")
        .list_data()
        .groups(["engine control"])
        .tags(["a&b"])
        .send()
        .await
        .unwrap();
    assert!(result.data.items.is_empty());
}
