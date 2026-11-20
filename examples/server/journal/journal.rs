// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Local journald logs for a systemd app and a Linux component.
//!
//! Run with: SOVD_JOURNAL_UNIT=APP01.service cargo run
//!   -p opensovd-examples-server --features journalctl --example journal

use opensovd_core::{App, Component, Topology};
use opensovd_providers::log::JournalctlLogProvider;
use opensovd_server::Server;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    libcli::init_tracing("info", None)?;

    let unit = std::env::var("SOVD_JOURNAL_UNIT")?;
    let app_id = std::env::var("SOVD_JOURNAL_APP_ID").or_else(|error| match error {
        std::env::VarError::NotPresent => {
            Ok(unit.strip_suffix(".service").unwrap_or(&unit).to_owned())
        }
        error @ std::env::VarError::NotUnicode(_) => Err(error),
    })?;
    let component_id = std::env::var("SOVD_JOURNAL_COMPONENT_ID").or_else(|error| match error {
        std::env::VarError::NotPresent => Ok("ECU01".into()),
        error @ std::env::VarError::NotUnicode(_) => Err(error),
    })?;

    let topology = Topology::new();
    {
        let mut guard = topology.write().await;
        guard.add_component(
            Component::new(&component_id, &component_id)
                .with_log_provider(JournalctlLogProvider::system()),
        );
        guard.add_app(
            App::new(&app_id, &app_id).with_log_provider(JournalctlLogProvider::new(unit)?),
        );
    }

    let listener = TcpListener::bind("127.0.0.1:7691").await?;
    let server = Server::builder()
        .base_uri("http://127.0.0.1:7691/sovd")?
        .listener(listener)
        .topology(topology)
        .build()?;
    server.serve().await?;
    Ok(())
}
