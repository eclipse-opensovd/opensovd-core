// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

#![cfg(unix)]
#![allow(clippy::unwrap_used)]

use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixListener;

/// Accept one connection, read HTTP headers, then write a canned 200 response.
async fn serve_one(listener: &UnixListener, body: &str) {
    let (mut stream, _) = listener.accept().await.unwrap();

    // Read until we see the end of the HTTP headers.
    let mut buf = Vec::with_capacity(1024);
    loop {
        let n = stream.read_buf(&mut buf).await.unwrap();
        assert!(n > 0, "client closed before sending full headers");
        if buf.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
    }

    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
        body.len(),
        body,
    );
    stream.write_all(response.as_bytes()).await.unwrap();
    stream.shutdown().await.unwrap();
}

#[tokio::test]
async fn connect_unix_path() {
    let dir = std::env::temp_dir().join(format!("opensovd-client-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let sock_path = dir.join("test.sock");

    // Clean up any leftover socket from a previous run.
    let _ = std::fs::remove_file(&sock_path);

    let listener = UnixListener::bind(&sock_path).unwrap();

    let body = r#"{"items":[]}"#;
    tokio::spawn(async move {
        serve_one(&listener, body).await;
    });

    let client =
        opensovd_client::Client::connect_unix("http://localhost/sovd/v1", &sock_path).unwrap();
    let result = client.list_components().send().await.unwrap();
    assert!(result.data.items.is_empty());

    // Clean up.
    let _ = std::fs::remove_file(&sock_path);
    let _ = std::fs::remove_dir(&dir);
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn connect_unix_abstract() {
    let name = format!("opensovd-test-{}", std::process::id());

    // Abstract sockets require the leading null byte for binding.
    let abstract_path = format!("\0{name}");
    let listener = UnixListener::bind(&abstract_path).unwrap();

    let body = r#"{"items":[]}"#;
    tokio::spawn(async move {
        serve_one(&listener, body).await;
    });

    let client =
        opensovd_client::Client::connect_unix_abstract("http://localhost/sovd/v1", &name).unwrap();
    let result = client.list_components().send().await.unwrap();
    assert!(result.data.items.is_empty());
}

#[tokio::test]
async fn connect_unix_not_found() {
    let path = std::env::temp_dir().join("opensovd-client-nonexistent.sock");
    let _ = std::fs::remove_file(&path); // ensure it doesn't exist

    let client = opensovd_client::Client::connect_unix("http://localhost/sovd/v1", &path).unwrap();
    let err = client.list_components().send().await.unwrap_err();
    assert!(
        matches!(err, opensovd_client::Error::Service { .. }),
        "expected Error::Service, got: {err:?}"
    );
}

#[tokio::test]
async fn builder_unix_socket_timeout_returns_typed_timeout_error() {
    let dir = std::env::temp_dir().join(format!(
        "opensovd-client-timeout-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let sock_path = dir.join("timeout.sock");

    let _ = std::fs::remove_file(&sock_path);

    let listener = UnixListener::bind(&sock_path).unwrap();

    tokio::spawn(async move {
        let (_stream, _) = listener.accept().await.unwrap();
        tokio::time::sleep(Duration::from_secs(2)).await;
    });

    let client = opensovd_client::Client::builder()
        .base_uri("http://localhost/sovd/v1")
        .unwrap()
        .timeout(Duration::from_secs(1))
        .unix_socket(&sock_path)
        .build()
        .unwrap();

    let err = client.list_components().send().await.unwrap_err();

    match err {
        opensovd_client::Error::Timeout(duration) => {
            assert_eq!(duration, Duration::from_secs(1));
        }
        other => panic!("expected Timeout error, got: {other:?}"),
    }

    let _ = std::fs::remove_file(&sock_path);
    let _ = std::fs::remove_dir(&dir);
}

#[tokio::test]
async fn builder_can_use_exported_unix_connector() {
    let dir = std::env::temp_dir().join(format!(
        "opensovd-client-builder-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let sock_path = dir.join("builder.sock");

    let _ = std::fs::remove_file(&sock_path);

    let listener = UnixListener::bind(&sock_path).unwrap();

    let body = r#"{"items":[]}"#;
    tokio::spawn(async move {
        serve_one(&listener, body).await;
    });

    let client = opensovd_client::Client::builder()
        .base_uri("http://localhost/sovd/v1")
        .unwrap()
        .connector(opensovd_client::UnixConnector::new(&sock_path))
        .build()
        .unwrap();
    let result = client.list_components().send().await.unwrap();
    assert!(result.data.items.is_empty());

    let _ = std::fs::remove_file(&sock_path);
    let _ = std::fs::remove_dir(&dir);
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn builder_unix_socket_abstract() {
    let name = format!("opensovd-builder-test-{}", std::process::id());

    let abstract_path = format!("\0{name}");
    let listener = UnixListener::bind(&abstract_path).unwrap();

    let body = r#"{"items":[]}"#;
    tokio::spawn(async move {
        serve_one(&listener, body).await;
    });

    let client = opensovd_client::Client::builder()
        .base_uri("http://localhost/sovd/v1")
        .unwrap()
        .unix_socket_abstract(&name)
        .build()
        .unwrap();
    let result = client.list_components().send().await.unwrap();
    assert!(result.data.items.is_empty());
}
