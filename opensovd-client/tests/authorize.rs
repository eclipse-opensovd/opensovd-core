// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

mod common;

use common::mock_client;
use mock_http_connector::Connector;
use serde_json::json;

#[tokio::test]
async fn authorize_posts_the_credentials_and_returns_the_token() {
    let mut builder = Connector::builder();
    builder
        .expect()
        .with_method("POST")
        .with_uri("http://localhost/sovd/v1/authorize")
        .with_header("content-type", "application/json")
        .returning(
            json!({"access_token": "a.b.c", "token_type": "Bearer", "expires_in": 3600})
                .to_string(),
        )
        .unwrap();
    let client = mock_client(builder.build());
    let token = client.authorize("client", "secret").await.unwrap();
    assert_eq!(token.access_token, "a.b.c");
    assert_eq!(token.token_type, "Bearer");
    assert_eq!(token.expires_in, 3600);
}

#[tokio::test]
async fn authorize_rejected() {
    let mut builder = Connector::builder();
    builder
        .expect()
        .with_method("POST")
        .with_uri("http://localhost/sovd/v1/authorize")
        .returning((
            http::StatusCode::UNAUTHORIZED,
            json!({"error_code": "vendor-specific", "message": "bad credentials"}).to_string(),
        ))
        .unwrap();
    let client = mock_client(builder.build());
    let err = client.authorize("client", "wrong").await.unwrap_err();
    assert!(
        matches!(err, opensovd_client::Error::ApiError { status, .. } if status.as_u16() == 401),
        "{err:?}"
    );
}

#[test]
fn credentials_and_tokens_stay_out_of_debug_output() {
    let response: opensovd_client::AuthorizeResponse = serde_json::from_value(json!({
        "access_token": "secret-token", "token_type": "Bearer", "expires_in": 1
    }))
    .unwrap();
    assert!(!format!("{response:?}").contains("secret-token"));
    let request = opensovd_models::auth::AuthorizeRequest {
        client_id: "client".to_owned(),
        client_secret: "hunter2".to_owned(),
    };
    assert!(!format!("{request:?}").contains("hunter2"));
}
