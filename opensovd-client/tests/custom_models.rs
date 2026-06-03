// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! A consumer supplies its own `Models` (CDA-shaped entity items with an
//! optional `id`, which `DefaultModels` would reject) and the typed client
//! deserializes into it.

use mock_http_connector::Connector;
use opensovd_client::{Client, Models};
use serde::Deserialize;
use serde_json::json;

#[derive(Debug, Deserialize)]
struct Resource {
    name: String,
    #[serde(default)]
    id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ResourceResponse {
    items: Vec<Resource>,
}

struct CdaModels;

impl Models for CdaModels {
    type Entities = ResourceResponse;
    type DataList = ResourceResponse;
    type ReadResponse = serde_json::Value;
    type DataCategories = serde_json::Value;
    type DataGroups = serde_json::Value;
}

#[tokio::test]
async fn custom_models_deserialize_entities() {
    let mut builder = Connector::builder();
    builder
        .expect()
        .with_uri("http://localhost/sovd/v1/components")
        .returning(json!({"items": [{"href": "components/ecu1", "name": "ECU 1"}]}).to_string())
        .unwrap();

    let client = Client::builder()
        .base_uri("http://localhost/sovd/v1")
        .unwrap()
        .models::<CdaModels>()
        .connector(builder.build())
        .build()
        .unwrap();

    let result = client.list_components().send().await.unwrap();
    assert_eq!(result.items.len(), 1);
    assert_eq!(result.items[0].name, "ECU 1");
    assert!(result.items[0].id.is_none());
}
