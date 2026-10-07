// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0
//
// This file was created with the assistance of generative AI.

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
    App, Component, LogConfiguration, LogContext, LogEntry, LogFilter, LogProvider, LogResult,
    LogSeverity, LogStream, Topology,
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

fn dlt_entry(severity: LogSeverity, message: &str) -> LogEntry {
    let timestamp = Utc.with_ymd_and_hms(2026, 10, 7, 8, 0, 0).unwrap();
    LogEntry {
        timestamp,
        context: LogContext::AutosarDlt {
            session: Some("DIAG".into()),
            session_id: Some("session-1".into()),
            application_id: Some("TRAC".into()),
            context_id: Some("Main".into()),
            message_id: None,
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
        state.add_component(Component::new("diag-ecu", "Diagnostic ECU"));
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
        entries: vec![dlt_entry(LogSeverity::DltInfo, "heartbeat")],
        seen_filter: Arc::new(Mutex::new(None)),
    };
    let (server, _) = server_with_provider(provider).await;

    let (status, body) = get_json(&server, "/sovd/v1/apps/diag-app/logs").await;

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
        body["x-opensovd-live-entries"],
        format!(
            "http://{}/sovd/v1/apps/diag-app/logs/entries/stream",
            server.addr
        )
    );
}

#[tokio::test]
async fn log_entries_accept_dlt_severity_filters() {
    let seen_filter = Arc::new(Mutex::new(None));
    let provider = RecordingProvider {
        entries: vec![dlt_entry(LogSeverity::DltInfo, "heartbeat")],
        seen_filter: Arc::clone(&seen_filter),
    };
    let (server, _) = server_with_provider(provider).await;

    let (status, body) = get_json(
        &server,
        "/sovd/v1/apps/diag-app/logs/entries?severity=DLT_INFO",
    )
    .await;

    assert_eq!(status, hyper::StatusCode::OK);
    assert_eq!(body["items"][0]["severity"], "DLT_INFO");
    assert_eq!(
        seen_filter.lock().unwrap().as_ref().unwrap().severity,
        Some(LogSeverity::DltInfo)
    );
}

#[tokio::test]
async fn log_entry_stream_returns_sse_event_envelope() {
    let seen_filter = Arc::new(Mutex::new(None));
    let provider = RecordingProvider {
        entries: vec![dlt_entry(LogSeverity::DltInfo, "live heartbeat")],
        seen_filter: Arc::clone(&seen_filter),
    };
    let (server, _) = server_with_provider(provider).await;
    let request = Request::builder()
        .uri(server.url("/sovd/v1/apps/diag-app/logs/entries/stream?severity=DLT_INFO"))
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
    assert!(body.contains("\"severity\":\"DLT_INFO\""));
    assert_eq!(
        seen_filter.lock().unwrap().as_ref().unwrap().severity,
        Some(LogSeverity::DltInfo)
    );
}
