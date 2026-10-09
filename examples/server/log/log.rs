// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0
//
// This file was created with the assistance of generative AI.

//! Minimal SOVD logging example.
//!
//! Run with: `cargo run -p opensovd-examples-server --example log`

use chrono::Utc;
use opensovd_core::{Component, LogContext, LogEntry, LogSeverity, Topology};
use opensovd_extra::trace::server_layer;
use opensovd_mocks::InMemoryLogProvider;
use opensovd_server::Server;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let provider = InMemoryLogProvider::default().with_entries([LogEntry {
        timestamp: Utc::now(),
        context: LogContext::Rfc5424 {
            host: Some("localhost".into()),
            process: Some("opensovd-log-example".into()),
            pid: None,
        },
        severity: LogSeverity::Info,
        msg: "example log entry".into(),
        href: None,
    }]);
    let topology = Topology::new();
    topology
        .write()
        .await
        .add_component(Component::new("ecu", "Example ECU").with_log_provider(provider));

    let listener = TcpListener::bind("127.0.0.1:7690").await?;
    let server = Server::builder()
        .base_uri("http://127.0.0.1:7690/sovd")?
        .listener(listener)
        .topology(topology)
        .layer(server_layer())
        .build()?;
    server.serve().await?;
    Ok(())
}
