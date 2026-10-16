// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0
//
// AI Disclosure: This file was created with the assistance of OpenAI Codex.
// Assisted-by: OpenAI Codex

#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Integration tests for the SOVD log resources and live-log extension.

mod common;

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use futures::stream;
use http_body_util::BodyExt;
use hyper::Request;
use opensovd_core::{
    App, Component, LogConfiguration, LogContext, LogEntry, LogError, LogFilter, LogProvider,
    LogResult, LogSeverity, LogStream, Topology,
};
use serde_json::Value;

#[derive(Clone)]
struct RecordingProvider {
    entries: Vec<LogEntry>,
    seen_filter: Arc<Mutex<Option<LogFilter>>>,
}

#[async_trait]
impl LogProvider for RecordingProvider {
    async fn entries(&self, filter: LogFilter) -> LogResult<Vec<LogEntry>> {
        *self.seen_filter.lock().unwrap() = Some(filter);
        Ok(self.entries.clone())
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    async fn stream(&self, filter: LogFilter) -> LogResult<LogStream> {
        *self.seen_filter.lock().unwrap() = Some(filter);
        let entry = self.entries.first().cloned().expect("test entry");
        Ok(Box::pin(stream::once(async move { Ok(entry) })))
    }

    async fn configuration(&self) -> LogResult<Vec<LogConfiguration>> {
        Ok(Vec::new())
    }

    async fn configure(&self, _configuration: Vec<LogConfiguration>) -> LogResult<()> {
        Ok(())
    }

    async fn reset_configuration(&self) -> LogResult<()> {
        Ok(())
    }
}

#[derive(Clone)]
struct SnapshotOnlyProvider;

#[async_trait]
impl LogProvider for SnapshotOnlyProvider {
    async fn entries(&self, _filter: LogFilter) -> LogResult<Vec<LogEntry>> {
        Ok(Vec::new())
    }

    async fn configuration(&self) -> LogResult<Vec<LogConfiguration>> {
        Ok(Vec::new())
    }

    async fn configure(&self, _configuration: Vec<LogConfiguration>) -> LogResult<()> {
        Ok(())
    }

    async fn reset_configuration(&self) -> LogResult<()> {
        Ok(())
    }
}

#[derive(Clone)]
struct ErrorStreamProvider;

#[async_trait]
impl LogProvider for ErrorStreamProvider {
    async fn entries(&self, _filter: LogFilter) -> LogResult<Vec<LogEntry>> {
        Ok(Vec::new())
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    async fn stream(&self, _filter: LogFilter) -> LogResult<LogStream> {
        Ok(Box::pin(stream::once(async {
            Err(LogError::Internal("secret provider details".into()))
        })))
    }

    async fn configuration(&self) -> LogResult<Vec<LogConfiguration>> {
        Ok(Vec::new())
    }

    async fn configure(&self, _configuration: Vec<LogConfiguration>) -> LogResult<()> {
        Ok(())
    }

    async fn reset_configuration(&self) -> LogResult<()> {
        Ok(())
    }
}

fn test_entry(severity: LogSeverity, message: &str) -> LogEntry {
    let timestamp = Utc.with_ymd_and_hms(2026, 10, 7, 8, 0, 0).unwrap();
    LogEntry {
        timestamp,
        context: LogContext {
            context_type: "test".into(),
            attributes: [("source".into(), "diag".into())]
                .into_iter()
                .map(|(key, value)| (key, Value::String(value)))
                .collect(),
        },
        severity,
        msg: message.into(),
        href: None,
    }
}

async fn server_with_provider(
    provider: RecordingProvider,
) -> (common::TestServer, Arc<Mutex<Option<LogFilter>>>) {
    let seen_filter = Arc::clone(&provider.seen_filter);
    let topology = Topology::new();
    {
        let mut state = topology.write().await;
        state.add_component(
            Component::new("diag-ecu", "Diagnostic ECU").with_log_provider(provider.clone()),
        );
        state.add_app(
            App::new("diag-app", "Diagnostic Application")
                .with_component_id("diag-ecu")
                .with_log_provider(provider),
        );
    }

    let server = common::TestServer::builder()
        .topology(topology)
        .build()
        .await;
    (server, seen_filter)
}

async fn server_with_snapshot_only_provider() -> common::TestServer {
    let topology = Topology::new();
    {
        let mut state = topology.write().await;
        state.add_app(
            App::new("snapshot-app", "Snapshot Application")
                .with_log_provider(SnapshotOnlyProvider),
        );
    }

    common::TestServer::builder()
        .topology(topology)
        .build()
        .await
}

async fn server_with_error_stream_provider() -> common::TestServer {
    let topology = Topology::new();
    {
        let mut state = topology.write().await;
        state.add_app(
            App::new("error-app", "Error Application").with_log_provider(ErrorStreamProvider),
        );
    }

    common::TestServer::builder()
        .topology(topology)
        .build()
        .await
}

async fn get_json(server: &common::TestServer, path: &str) -> (hyper::StatusCode, Value) {
    let request = Request::builder()
        .uri(server.url(path))
        .body(http_body_util::Empty::<bytes::Bytes>::new())
        .unwrap();
    let response = common::client().request(request).await.unwrap();
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&body).unwrap())
}

#[tokio::test]
async fn log_resources_advertise_snapshot_config_and_live_stream() {
    let provider = RecordingProvider {
        entries: vec![test_entry(LogSeverity::Info, "heartbeat")],
        seen_filter: Arc::new(Mutex::new(None)),
    };
    let (server, _) = server_with_provider(provider).await;

    let (status, body) = get_json(&server, "/sovd/v1/apps/diag-app/logs?include-schema=true").await;

    assert_eq!(status, hyper::StatusCode::OK);
    assert_eq!(
        body["entries"],
        format!("http://{}/sovd/v1/apps/diag-app/logs/entries", server.addr)
    );
    assert_eq!(
        body["config"],
        format!("http://{}/sovd/v1/apps/diag-app/logs/config", server.addr)
    );
    assert_eq!(
        body["x-opensovd-log-streams"],
        format!(
            "http://{}/sovd/v1/apps/diag-app/logs/x-opensovd-log-streams",
            server.addr
        )
    );
    assert!(body["schema"].is_object());
}

#[tokio::test]
async fn log_resources_do_not_advertise_live_stream_for_snapshot_only_provider() {
    let server = server_with_snapshot_only_provider().await;

    let (status, body) = get_json(&server, "/sovd/v1/apps/snapshot-app/logs").await;

    assert_eq!(status, hyper::StatusCode::OK);
    assert!(body.get("x-opensovd-log-streams").is_none());
}

#[tokio::test]
async fn unsupported_live_stream_returns_not_implemented() {
    let server = server_with_snapshot_only_provider().await;

    let (status, body) = get_json(
        &server,
        "/sovd/v1/apps/snapshot-app/logs/x-opensovd-log-streams",
    )
    .await;

    assert_eq!(status, hyper::StatusCode::NOT_IMPLEMENTED);
    assert_eq!(body["error_code"], "error-response");
}

#[tokio::test]
async fn components_serve_logs() {
    let provider = RecordingProvider {
        entries: vec![test_entry(LogSeverity::Info, "component heartbeat")],
        seen_filter: Arc::new(Mutex::new(None)),
    };
    let (server, _) = server_with_provider(provider).await;

    let (status, body) = get_json(
        &server,
        "/sovd/v1/components/diag-ecu/logs/entries?severity=info",
    )
    .await;

    assert_eq!(status, hyper::StatusCode::OK);
    assert_eq!(body["items"][0]["msg"], "component heartbeat");
}

#[tokio::test]
async fn log_configuration_honors_include_schema() {
    let provider = RecordingProvider {
        entries: vec![],
        seen_filter: Arc::new(Mutex::new(None)),
    };
    let (server, _) = server_with_provider(provider).await;

    let (status, body) = get_json(
        &server,
        "/sovd/v1/apps/diag-app/logs/config?include-schema=true",
    )
    .await;

    assert_eq!(status, hyper::StatusCode::OK);
    assert!(body["schema"].is_object());
}

#[tokio::test]
async fn invalid_time_range_is_rejected_before_snapshot_provider() {
    let provider = RecordingProvider {
        entries: vec![],
        seen_filter: Arc::new(Mutex::new(None)),
    };
    let (server, seen_filter) = server_with_provider(provider).await;

    let (status, body) = get_json(
        &server,
        "/sovd/v1/apps/diag-app/logs/entries?created-after=2026-10-07T09:00:00Z&created-before=2026-10-07T08:00:00Z",
    )
    .await;

    assert_eq!(status, hyper::StatusCode::BAD_REQUEST);
    assert_eq!(body["error_code"], "incomplete-request");
    assert!(seen_filter.lock().unwrap().is_none());
}

#[tokio::test]
async fn invalid_time_range_is_rejected_before_stream_provider() {
    let provider = RecordingProvider {
        entries: vec![test_entry(LogSeverity::Info, "heartbeat")],
        seen_filter: Arc::new(Mutex::new(None)),
    };
    let (server, seen_filter) = server_with_provider(provider).await;

    let (status, body) = get_json(
        &server,
        "/sovd/v1/apps/diag-app/logs/x-opensovd-log-streams?created-after=2026-10-07T09:00:00Z&created-before=2026-10-07T08:00:00Z",
    )
    .await;

    assert_eq!(status, hyper::StatusCode::BAD_REQUEST);
    assert_eq!(body["error_code"], "incomplete-request");
    assert!(seen_filter.lock().unwrap().is_none());
}

#[tokio::test]
async fn log_entries_accept_severity_filters() {
    let seen_filter = Arc::new(Mutex::new(None));
    let provider = RecordingProvider {
        entries: vec![test_entry(LogSeverity::Info, "heartbeat")],
        seen_filter: Arc::clone(&seen_filter),
    };
    let (server, _) = server_with_provider(provider).await;

    let (status, body) =
        get_json(&server, "/sovd/v1/apps/diag-app/logs/entries?severity=info").await;

    assert_eq!(status, hyper::StatusCode::OK);
    assert_eq!(body["items"][0]["severity"], "info");
    assert_eq!(
        seen_filter.lock().unwrap().as_ref().unwrap().severity,
        Some(LogSeverity::Info)
    );
}

#[tokio::test]
async fn log_entry_stream_returns_sse_event_envelope() {
    let seen_filter = Arc::new(Mutex::new(None));
    let provider = RecordingProvider {
        entries: vec![test_entry(LogSeverity::Info, "live heartbeat")],
        seen_filter: Arc::clone(&seen_filter),
    };
    let (server, _) = server_with_provider(provider).await;
    let request = Request::builder()
        .uri(server.url("/sovd/v1/apps/diag-app/logs/x-opensovd-log-streams?severity=info"))
        .body(http_body_util::Empty::<bytes::Bytes>::new())
        .unwrap();

    let response = common::client().request(request).await.unwrap();
    assert_eq!(response.status(), hyper::StatusCode::OK);
    assert!(
        response
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("text/event-stream")
    );

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body = String::from_utf8(body.to_vec()).unwrap();
    assert!(body.starts_with("data: "));
    assert!(body.contains("\"payload\""));
    assert!(body.contains("\"severity\":\"info\""));
    assert_eq!(
        seen_filter.lock().unwrap().as_ref().unwrap().severity,
        Some(LogSeverity::Info)
    );
}

#[tokio::test]
async fn log_entry_stream_emits_sanitized_provider_errors() {
    let server = server_with_error_stream_provider().await;
    let request = Request::builder()
        .uri(server.url("/sovd/v1/apps/error-app/logs/x-opensovd-log-streams"))
        .body(http_body_util::Empty::<bytes::Bytes>::new())
        .unwrap();

    let response = common::client().request(request).await.unwrap();
    assert_eq!(response.status(), hyper::StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body = String::from_utf8(body.to_vec()).unwrap();
    assert!(body.contains("\"payload\":null"));
    assert!(body.contains("\"error\":{"));
    assert!(body.contains("the log provider failed to produce a stream item"));
    assert!(!body.contains("secret provider details"));
}
