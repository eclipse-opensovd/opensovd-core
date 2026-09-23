// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

mod common;

use common::mock_client;
use mock_http_connector::Connector;
use serde_json::json;

#[tokio::test]
async fn list_apps() {
    let mut builder = Connector::builder();
    builder
        .expect()
        .with_uri("http://localhost/sovd/v1/apps")
        .returning(json!({"items": []}).to_string())
        .unwrap();
    let client = mock_client(builder.build());
    let result = client.list_apps().send().await.unwrap();
    assert!(result.data.items.is_empty());
}

#[tokio::test]
async fn app_is_located_on() {
    let mut builder = Connector::builder();
    builder
        .expect()
        .with_uri("http://localhost/sovd/v1/apps/diag/is-located-on")
        .returning(json!({"items": []}).to_string())
        .unwrap();
    let client = mock_client(builder.build());
    let result = client.app("diag").is_located_on().await.unwrap();
    assert!(result.items.is_empty());
}

#[tokio::test]
async fn app_belongs_to() {
    let mut builder = Connector::builder();
    builder
        .expect()
        .with_uri("http://localhost/sovd/v1/apps/diag/belongs-to")
        .returning(json!({"items": []}).to_string())
        .unwrap();
    let client = mock_client(builder.build());
    let result = client.app("diag").belongs_to().await.unwrap();
    assert!(result.items.is_empty());
}

#[tokio::test]
async fn list_apps_with_schema() {
    let mut builder = Connector::builder();
    builder
        .expect()
        .with_uri("http://localhost/sovd/v1/apps?include-schema=true")
        .returning(json!({"items": [], "schema": {"type": "object"}}).to_string())
        .unwrap();
    let client = mock_client(builder.build());
    let result = client.list_apps().schema(true).send().await.unwrap();
    assert!(result.data.items.is_empty());
    assert_eq!(result.schema.unwrap(), json!({"type": "object"}));
}

#[tokio::test]
async fn app_capabilities_keep_default_links() {
    let mut builder = Connector::builder();
    builder
        .expect()
        .with_uri("http://localhost/sovd/v1/apps/app1")
        .returning(json!({"id": "app1", "name": "App"}).to_string())
        .unwrap();
    builder
        .expect()
        .with_uri("http://localhost/sovd/v1/apps/app1/data")
        .returning(json!({"items": []}).to_string())
        .unwrap();
    let client = mock_client(builder.build());

    let app = client.app("app1").capabilities().await.unwrap();
    assert!(app.links().data.is_some());
    app.list_data().send().await.unwrap();
}
