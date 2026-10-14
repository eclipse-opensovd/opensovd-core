// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0
#![allow(clippy::expect_used, clippy::unwrap_used)]

use mock_http_connector::Connector;
use opensovd_client::{Auth, BuilderError, Client, SovdInfo};
use serde_json::json;

type Request = http::Request<String>;

#[tokio::test]
async fn client_sends_bearer_authorization_header() {
    let mut builder = Connector::builder();
    builder
        .expect()
        .with_uri("http://localhost/sovd/v1/components")
        .returning(|req: Request| async move {
            let authorization = req
                .headers()
                .get(http::header::AUTHORIZATION)
                .expect("authorization header present");
            assert_eq!(authorization, "Bearer test-token");
            json!({"items": []}).to_string()
        })
        .unwrap();

    let client = Client::builder()
        .base_uri("http://localhost/sovd/v1")
        .expect("valid URI")
        .auth(Auth::bearer("test-token"))
        .connector(builder.build())
        .build()
        .expect("valid test client");

    let result = client.list_components().send().await.unwrap();
    assert!(result.data.items.is_empty());
}

#[tokio::test]
async fn discovery_selected_client_inherits_authorization_header() {
    let mut builder = Connector::builder();
    builder
        .expect()
        .with_uri("http://localhost/sovd/version-info")
        .returning(|req: Request| async move {
            let authorization = req
                .headers()
                .get(http::header::AUTHORIZATION)
                .expect("authorization header present");
            assert_eq!(authorization, "Bearer inherited-token");
            json!({"sovd_info": [{
                "version": "1.1",
                "base_uri": "http://localhost/sovd/v1"
            }]})
            .to_string()
        })
        .unwrap();
    builder
        .expect()
        .with_uri("http://localhost/sovd/v1/components")
        .returning(|req: Request| async move {
            let authorization = req
                .headers()
                .get(http::header::AUTHORIZATION)
                .expect("authorization header present");
            assert_eq!(authorization, "Bearer inherited-token");
            json!({"items": []}).to_string()
        })
        .unwrap();

    let client = Client::builder()
        .base_uri("http://localhost/sovd")
        .expect("valid URI")
        .auth(Auth::bearer("inherited-token"))
        .connector(builder.build())
        .discovery()
        .expect("valid discovery client")
        .select(|s: &SovdInfo<serde_json::Value>| s.version == "1.1")
        .await
        .unwrap();

    let result = client.list_components().send().await.unwrap();
    assert!(result.data.items.is_empty());
}

#[test]
fn build_rejects_invalid_bearer_token() {
    let result = Client::builder()
        .base_uri("http://localhost/sovd/v1")
        .expect("valid URI")
        .auth(Auth::bearer("bad\ntoken"))
        .build();

    assert!(matches!(result, Err(BuilderError::InvalidToken)));
}
