// SPDX-FileCopyrightText: 2026 Copyright (c) Contributors to the Eclipse Foundation
//
// See the NOTICE file(s) distributed with this work for additional
// information regarding copyright ownership.
//
// This program and the accompanying materials are made available under the
// terms of the Apache License Version 2.0 which is available at
// https://www.apache.org/licenses/LICENSE-2.0
//
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
        .with_uri("http://localhost/sovd/v1/apps/diag")
        .returning(
            json!({
                "id": "diag",
                "name": "Diagnostics",
                "is-located-on": "http://localhost/sovd/v1/components/ecu1"
            })
            .to_string(),
        )
        .unwrap();
    builder
        .expect()
        .with_uri("http://localhost/sovd/v1/components/ecu1")
        .returning(json!({"id": "ecu1", "name": "ECU 1"}).to_string())
        .unwrap();
    let client = mock_client(builder.build());
    let component = client.app("diag").is_located_on().await.unwrap().unwrap();
    assert_eq!(component.id, "ecu1");
}

#[tokio::test]
async fn app_belongs_to() {
    let mut builder = Connector::builder();
    builder
        .expect()
        .with_uri("http://localhost/sovd/v1/apps/diag")
        .returning(
            json!({
                "id": "diag",
                "name": "Diagnostics",
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
    let area = client.app("diag").belongs_to().await.unwrap().unwrap();
    assert_eq!(area.id, "body");
}

#[tokio::test]
async fn app_without_relation_links() {
    let mut builder = Connector::builder();
    builder
        .expect()
        .with_uri("http://localhost/sovd/v1/apps/diag")
        .returning(json!({"id": "diag", "name": "Diagnostics"}).to_string())
        .unwrap();
    let client = mock_client(builder.build());
    let app = client.app("diag");
    assert!(app.is_located_on().await.unwrap().is_none());
    assert!(app.belongs_to().await.unwrap().is_none());
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
