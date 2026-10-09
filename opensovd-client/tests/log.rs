// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0
//
// This file was created with the assistance of generative AI.

mod common;

use common::mock_client;
use mock_http_connector::Connector;
use opensovd_client::LogSeverity;
use serde_json::json;

#[tokio::test]
async fn component_logs_send_filters() {
    let mut builder = Connector::builder();
    builder
        .expect()
        .with_uri("http://localhost/sovd/v1/components/ecu1/logs/entries?severity=DLT_INFO&include-schema=true")
        .returning(json!({"items": [], "schema": {"type": "object"}}).to_string())
        .unwrap();
    let client = mock_client(builder.build());
    let result = client
        .component("ecu1")
        .logs()
        .severity(LogSeverity::DltInfo)
        .schema(true)
        .send()
        .await
        .unwrap();
    assert!(result.data.items.is_empty());
    assert!(result.schema.is_some());
}

#[tokio::test]
async fn app_log_configuration_uses_config_resource() {
    let mut builder = Connector::builder();
    builder
        .expect()
        .with_uri("http://localhost/sovd/v1/apps/diag/logs/config")
        .returning(json!({"contexts": []}).to_string())
        .unwrap();
    let client = mock_client(builder.build());
    let result = client.app("diag").log_configuration().await.unwrap();
    assert!(result.data.contexts.is_empty());
}
