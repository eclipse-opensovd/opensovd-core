// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0
#![allow(clippy::expect_used)]

mod common;

use common::mock_client;
use mock_http_connector::Connector;
use serde_json::json;

#[tokio::test]
async fn percent_encodes_path_segments() {
    let mut builder = Connector::builder();
    builder
        .expect()
        .with_uri("http://localhost/sovd/v1/components/ecu%2F1/data")
        .returning(json!({"items": []}).to_string())
        .unwrap();
    let client = mock_client(builder.build());
    let result = client.component("ecu/1").list_data().send().await.unwrap();
    assert!(result.data.items.is_empty());
}

#[tokio::test]
async fn http_error_status() {
    let mut builder = Connector::builder();
    builder
        .expect()
        .with_uri("http://localhost/sovd/v1/components")
        .returning((
            http::StatusCode::NOT_FOUND,
            json!({
                "error_code": "vendor-specific",
                "message": "not found"
            })
            .to_string(),
        ))
        .unwrap();
    let client = mock_client(builder.build());
    let err = client.list_components().send().await.unwrap_err();
    match err {
        opensovd_client::Error::ApiError { status, error } => {
            assert_eq!(status.as_u16(), 404);
            assert!(error.is_some());
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

#[tokio::test]
async fn with_layer_builds_client() {
    use tower::layer::util::Identity;

    let mut builder = Connector::builder();
    builder
        .expect()
        .with_uri("http://localhost/sovd/v1/components")
        .returning(json!({"items": []}).to_string())
        .unwrap();

    let client = opensovd_client::Client::builder()
        .base_uri("http://localhost/sovd/v1")
        .expect("valid URI")
        .layer(Identity::new())
        .connector(builder.build())
        .build()
        .expect("valid test client with layer");

    let list = client.list_components().send().await.unwrap();
    assert!(list.data.items.is_empty());
}

// TODO: Add tests for hyper-timeout (https://github.com/hjr3/hyper-timeout)

#[tokio::test]
async fn timeout_layer_times_out() {
    use std::time::Duration;

    use tokio::io::AsyncWriteExt;
    use tokio::net::TcpListener;
    use tower::timeout::TimeoutLayer;

    // Spawn a slow server that delays 2s before responding
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        // Wait longer than the client timeout
        tokio::time::sleep(Duration::from_secs(2)).await;
        // Send a valid HTTP response (client won't see this due to timeout)
        let response = "HTTP/1.1 200 OK\r\nContent-Length: 13\r\n\r\n{\"items\": []}";
        let _ = socket.write_all(response.as_bytes()).await;
    });

    // Build client with 1s timeout
    let client = opensovd_client::Client::builder()
        .base_uri(format!("http://{addr}/sovd/v1"))
        .expect("valid URI")
        .layer(TimeoutLayer::new(Duration::from_secs(1)))
        .build()
        .expect("valid test client with timeout layer");

    let err = client.list_components().send().await.unwrap_err();

    // Verify it's a Service error containing a timeout
    let opensovd_client::Error::Service { source } = err else {
        panic!("expected Service error, got: {err:?}");
    };

    // Check for Tower's Elapsed (timeout) error
    assert!(
        source.is::<tower::timeout::error::Elapsed>(),
        "expected timeout error, got: {source:?}"
    );
}
