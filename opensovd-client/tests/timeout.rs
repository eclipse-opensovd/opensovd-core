// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use http::StatusCode;
use mock_http_connector::Connector;
use tokio::time::sleep;

#[tokio::test]
async fn timeout_layer_rejects_slow_request() {
    let call_count = Arc::new(AtomicU32::new(0));
    let call_count_clone = Arc::clone(&call_count);

    let mut builder = Connector::builder();
    builder
        .expect()
        .with_uri("http://localhost/sovd/v1/components")
        .returning(move |_| {
            let _ = call_count_clone.fetch_add(1, Ordering::SeqCst);
            async move {
                // Delay response beyond timeout (500ms timeout < 1s delay)
                sleep(Duration::from_secs(1)).await;
                (StatusCode::OK, String::new())
            }
        })
        .expect("mock builder");

    let client = opensovd_client::Client::builder()
        .base_uri("http://localhost/sovd/v1")
        .expect("valid URI")
        .timeout(Duration::from_millis(500))
        .connector(builder.build())
        .build()
        .expect("valid client");

    let result = client.list_components().send().await;
    assert!(result.is_err());
    if let opensovd_client::Error::Timeout(d) = result.unwrap_err() {
        assert_eq!(d, Duration::from_millis(500));
    } else {
        panic!("Expected Timeout error");
    }
}

#[tokio::test]
async fn timeout_layer_allows_fast_request() {
    let mut builder = Connector::builder();
    builder
        .expect()
        .with_uri("http://localhost/sovd/v1/components")
        .returning(|_| async move {
            // Quick response (10ms << 1000ms timeout)
            sleep(Duration::from_millis(10)).await;
            (StatusCode::OK, r#"{"items":[]}"#.to_string())
        })
        .expect("mock builder");

    let client = opensovd_client::Client::builder()
        .base_uri("http://localhost/sovd/v1")
        .expect("valid URI")
        .timeout(Duration::from_secs(1))
        .connector(builder.build())
        .build()
        .expect("valid client");

    let result = client.list_components().send().await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn timeout_layer_passes_through_without_timeout_config() {
    let call_count = Arc::new(AtomicU32::new(0));
    let call_count_clone = Arc::clone(&call_count);

    let mut builder = Connector::builder();
    builder
        .expect()
        .with_uri("http://localhost/sovd/v1/components")
        .returning(move |_| {
            let _ = call_count_clone.fetch_add(1, Ordering::SeqCst);
            async move {
                // Even a long delay is fine without timeout
                sleep(Duration::from_millis(100)).await;
                (StatusCode::OK, r#"{"items":[]}"#.to_string())
            }
        })
        .expect("mock builder");

    let client = opensovd_client::Client::builder()
        .base_uri("http://localhost/sovd/v1")
        .expect("valid URI")
        // No timeout set
        .connector(builder.build())
        .build()
        .expect("valid client");

    let result = client.list_components().send().await;
    assert!(result.is_ok());
    assert_eq!(call_count.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn timeout_then_retry_on_next_attempt() {
    let call_count = Arc::new(AtomicU32::new(0));
    let call_count_clone = Arc::clone(&call_count);

    let mut builder = Connector::builder();
    builder
        .expect()
        .with_uri("http://localhost/sovd/v1/components")
        .returning(move |_| {
            let call_num = call_count_clone.fetch_add(1, Ordering::SeqCst);
            async move {
                if call_num == 0 {
                    // First call: timeout (1s delay with 500ms timeout)
                    sleep(Duration::from_secs(1)).await;
                } else {
                    // Retry: quick response (10ms)
                    sleep(Duration::from_millis(10)).await;
                }
                (StatusCode::OK, r#"{"items":[]}"#.to_string())
            }
        })
        .expect("mock builder");

    let client = opensovd_client::Client::builder()
        .base_uri("http://localhost/sovd/v1")
        .expect("valid URI")
        .timeout(Duration::from_millis(500))
        .retry(opensovd_client::RetryPolicy::new(2).backoff(Duration::from_millis(50)))
        .connector(builder.build())
        .build()
        .expect("valid client");

    let result = client.list_components().send().await;
    // First attempt times out, retry should succeed
    assert!(result.is_ok());
    // Verify both attempts were made
    assert_eq!(call_count.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn timeout_all_retry_attempts_exhausts() {
    let call_count = Arc::new(AtomicU32::new(0));
    let call_count_clone = Arc::clone(&call_count);

    let mut builder = Connector::builder();
    builder
        .expect()
        .with_uri("http://localhost/sovd/v1/components")
        .returning(move |_| {
            let _ = call_count_clone.fetch_add(1, Ordering::SeqCst);
            async move {
                // Every attempt times out (1s delay with 500ms timeout)
                sleep(Duration::from_secs(1)).await;
                (StatusCode::OK, r#"{"items":[]}"#.to_string())
            }
        })
        .expect("mock builder");

    let client = opensovd_client::Client::builder()
        .base_uri("http://localhost/sovd/v1")
        .expect("valid URI")
        .timeout(Duration::from_millis(500))
        .retry(opensovd_client::RetryPolicy::new(2).backoff(Duration::from_millis(50)))
        .connector(builder.build())
        .build()
        .expect("valid client");

    let result = client.list_components().send().await;
    // All attempts time out
    assert!(result.is_err());
    if let opensovd_client::Error::Timeout(d) = result.unwrap_err() {
        assert_eq!(d, Duration::from_millis(500));
    } else {
        panic!("Expected Timeout error");
    }
    // 3 attempts: initial + 2 retries
    assert_eq!(call_count.load(Ordering::SeqCst), 3);
}
