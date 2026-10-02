// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod common;

use std::time::Duration;

use common::mock_client;
use mock_http_connector::Connector;
use opensovd_client::RetryPolicy;
use serde_json::json;

fn retry_client(connector: Connector, policy: RetryPolicy) -> opensovd_client::Client {
    opensovd_client::Client::builder()
        .base_uri("http://localhost/sovd/v1")
        .expect("valid URI")
        .retry(policy)
        .connector(connector)
        .build()
        .expect("valid test client with retry policy")
}

// ── GET 503 → retry → 200 ──────────────────────────────────────────────────

#[tokio::test]
async fn retry_on_503_succeeds() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

    let counter = Arc::new(AtomicU32::new(0));
    let counter_clone = Arc::clone(&counter);

    let mut b = Connector::builder();
    b.expect()
        .with_uri("http://localhost/sovd/v1/components")
        .returning(move |_| {
            let c = counter_clone.fetch_add(1, Ordering::SeqCst);
            async move {
                if c == 0 {
                    (http::StatusCode::SERVICE_UNAVAILABLE, String::new())
                } else {
                    (http::StatusCode::OK, json!({"items": []}).to_string())
                }
            }
        })
        .unwrap();

    let client = retry_client(
        b.build(),
        RetryPolicy::new(1).backoff(Duration::from_millis(1)),
    );
    let result = client.list_components().send().await.unwrap();
    assert!(result.data.items.is_empty());
}

// ── All attempts exhausted → last error returned ───────────────────────────

#[tokio::test]
async fn retry_exhaustion_returns_last_error() {
    let mut b = Connector::builder();
    // max_retries=2 → 3 total attempts
    for _ in 0..3 {
        b.expect()
            .with_uri("http://localhost/sovd/v1/components")
            .returning((
                http::StatusCode::SERVICE_UNAVAILABLE,
                json!({"error_code": "service-unavailable", "message": "down"}).to_string(),
            ))
            .unwrap();
    }

    let client = retry_client(
        b.build(),
        RetryPolicy::new(2).backoff(Duration::from_millis(1)),
    );
    let err = client.list_components().send().await.unwrap_err();
    match err {
        opensovd_client::Error::ApiError { status, .. } => {
            assert_eq!(status.as_u16(), 503);
        }
        other => panic!("expected ApiError(503), got: {other:?}"),
    }
}

// ── PUT is never retried ───────────────────────────────────────────────────

#[tokio::test]
async fn no_retry_on_put() {
    let mut b = Connector::builder();
    // Only one expectation: if a retry were attempted the mock would error differently.
    b.expect()
        .with_uri("http://localhost/sovd/v1/resource")
        .returning((http::StatusCode::SERVICE_UNAVAILABLE, String::new()))
        .unwrap();

    let client = retry_client(
        b.build(),
        RetryPolicy::new(3).backoff(Duration::from_millis(1)),
    );
    let err = client.put("/resource", &[], &json!({})).await.unwrap_err();
    match err {
        opensovd_client::Error::ApiError { status, .. } => {
            assert_eq!(status.as_u16(), 503);
        }
        other => panic!("expected ApiError(503), got: {other:?}"),
    }
}

// ── 4xx is not retryable ───────────────────────────────────────────────────

#[tokio::test]
async fn no_retry_on_4xx() {
    let mut b = Connector::builder();
    b.expect()
        .with_uri("http://localhost/sovd/v1/components")
        .returning((
            http::StatusCode::NOT_FOUND,
            json!({"error_code": "not-found", "message": "not found"}).to_string(),
        ))
        .unwrap();

    let client = retry_client(
        b.build(),
        RetryPolicy::new(3).backoff(Duration::from_millis(1)),
    );
    let err = client.list_components().send().await.unwrap_err();
    match err {
        opensovd_client::Error::ApiError { status, .. } => {
            assert_eq!(status.as_u16(), 404);
        }
        other => panic!("expected ApiError(404), got: {other:?}"),
    }
}

// ── 429 is not retried (Retry-After follow-up) ────────────────────────────

#[tokio::test]
async fn no_retry_on_429() {
    let mut b = Connector::builder();
    // Only one expectation: 429 must not be retried.
    b.expect()
        .with_uri("http://localhost/sovd/v1/components")
        .returning((
            http::StatusCode::TOO_MANY_REQUESTS,
            json!({"error_code": "rate-limited", "message": "slow down"}).to_string(),
        ))
        .unwrap();

    let client = retry_client(
        b.build(),
        RetryPolicy::new(3).backoff(Duration::from_millis(1)),
    );
    let err = client.list_components().send().await.unwrap_err();
    match err {
        opensovd_client::Error::ApiError { status, .. } => {
            assert_eq!(status.as_u16(), 429);
        }
        other => panic!("expected ApiError(429), got: {other:?}"),
    }
}

// ── No policy → single attempt ────────────────────────────────────────────

#[tokio::test]
async fn default_no_retry() {
    let mut b = Connector::builder();
    b.expect()
        .with_uri("http://localhost/sovd/v1/components")
        .returning((http::StatusCode::SERVICE_UNAVAILABLE, String::new()))
        .unwrap();

    let client = mock_client(b.build());
    let err = client.list_components().send().await.unwrap_err();
    match err {
        opensovd_client::Error::ApiError { status, .. } => {
            assert_eq!(status.as_u16(), 503);
        }
        other => panic!("expected ApiError(503), got: {other:?}"),
    }
}

// ── Per-attempt timeout is retried ────────────────────────────────────────

#[tokio::test]
async fn retry_on_timeout() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

    let counter = Arc::new(AtomicU32::new(0));
    let counter_clone = Arc::clone(&counter);

    let mut b = Connector::builder();
    b.expect()
        .with_uri("http://localhost/sovd/v1/components")
        .returning(move |_| {
            let c = counter_clone.fetch_add(1, Ordering::SeqCst);
            async move {
                if c == 0 {
                    // First attempt: delay past the per-attempt timeout.
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
                (http::StatusCode::OK, json!({"items": []}).to_string())
            }
        })
        .unwrap();

    let client = opensovd_client::Client::builder()
        .base_uri("http://localhost/sovd/v1")
        .expect("valid URI")
        .timeout(Duration::from_millis(200))
        .retry(RetryPolicy::new(1).backoff(Duration::from_millis(1)))
        .connector(b.build())
        .build()
        .expect("valid test client with timeout and retry");

    let result = client.list_components().send().await.unwrap();
    assert!(result.data.items.is_empty());
    assert_eq!(counter.load(Ordering::SeqCst), 2);
}

// ── Discovery::select inherits retry policy ───────────────────────────────

#[tokio::test]
async fn discovery_select_inherits_retry() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

    use opensovd_client::{Client, SovdInfo};

    let counter = Arc::new(AtomicU32::new(0));
    let counter_clone = Arc::clone(&counter);

    let mut b = Connector::builder();
    b.expect()
        .with_uri("http://localhost:7690/sovd/version-info")
        .returning(
            json!({"sovd_info": [{
                "version": "1.1",
                "base_uri": "http://localhost:7690/sovd/v1"
            }]})
            .to_string(),
        )
        .unwrap();
    b.expect()
        .with_uri("http://localhost:7690/sovd/v1/components")
        .returning(move |_| {
            let c = counter_clone.fetch_add(1, Ordering::SeqCst);
            async move {
                if c == 0 {
                    (http::StatusCode::SERVICE_UNAVAILABLE, String::new())
                } else {
                    (http::StatusCode::OK, json!({"items": []}).to_string())
                }
            }
        })
        .unwrap();

    let client = Client::builder()
        .base_uri("http://localhost:7690/sovd")
        .expect("valid URI")
        .retry(RetryPolicy::new(1).backoff(Duration::from_millis(1)))
        .connector(b.build())
        .discovery()
        .expect("valid discovery client")
        .select(|s: &SovdInfo<serde_json::Value>| s.version == "1.1")
        .await
        .unwrap();

    // Selected client must inherit the retry policy and succeed on the second attempt.
    let result = client.list_components().send().await.unwrap();
    assert!(result.data.items.is_empty());
}
