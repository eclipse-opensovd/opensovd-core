// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

#![allow(clippy::expect_used)]

//! Online capability description example (ISO 17978-3).
//!
//! Starts a server on port 7691 with one "engine" component and one "diag"
//! app, each exposing data resources. The point of interest is the SOVD *online
//! capability description*: appending `/docs` to the data collection path returns
//! a self-contained OpenAPI 3.1 specification describing how to interact with
//! that endpoint.
//!
//! Run with: `cargo run -p opensovd-examples-server --example online_capability`
//!
//! Then query the online capability description:
//!
//! ```bash
//! curl -s http://localhost:7691/sovd/v1/components/engine/data/docs | jq
//! curl -s http://localhost:7691/sovd/v1/apps/diag/data/docs | jq
//! ```

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use opensovd_core::{App, Component};
use opensovd_models::data::DataCategory;
use opensovd_providers::data::{
    Constant, DataProviderBuilder, ReadableDataResource, Value, WriteableDataResource,
};
use opensovd_server::{Server, Topology};
use tokio::net::TcpListener;

/// In-memory engine RPM set-point for the example data collection.
struct Rpm(Arc<Mutex<f64>>);

impl Rpm {
    fn new() -> Self {
        Self(Arc::new(Mutex::new(800.0)))
    }
}

#[async_trait]
impl ReadableDataResource for Rpm {
    type Value = Value<f64>;

    async fn read(&self) -> Result<Self::Value, opensovd_core::DataError> {
        let rpm = *self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        Ok(Value::new(rpm))
    }
}

#[async_trait]
impl WriteableDataResource for Rpm {
    type Value = Value<f64>;

    async fn write(&self, value: &Self::Value) -> Result<(), opensovd_core::DataError> {
        *self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = value.value;
        Ok(())
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    libcli::init_tracing("info", None)?;

    // A mixed collection of engine resources so `/data/docs` shows a useful
    // metadata example instead of a single row.
    let provider = DataProviderBuilder::new()
        .data(
            "rpm",
            "Engine RPM set-point",
            &DataCategory::CurrentData,
            Rpm::new(),
        )
        .groups(["actuators", "powertrain"])
        .tags(["live", "control"])
        .translation_id("engine.rpm.setpoint")
        .read_data(
            "coolant_temperature",
            "Coolant Temperature",
            &DataCategory::CurrentData,
            Constant::new(92.5)?,
        )
        .groups(["sensors", "thermal"])
        .tags(["live", "safety"])
        .translation_id("engine.coolant.temperature")
        .read_data(
            "battery_voltage",
            "Battery Voltage",
            &DataCategory::SysInfo,
            Constant::new(13.8)?,
        )
        .groups(["electrical"])
        .tags(["live", "power"])
        .translation_id("engine.battery.voltage")
        .read_data(
            "serial_number",
            "ECU Serial Number",
            &DataCategory::IdentData,
            Constant::new("ENG-ECU-42")?,
        )
        .groups(["identity"])
        .tags(["inventory"])
        .translation_id("engine.ecu.serial")
        .build()?;

    let engine = Component::new("engine", "Engine Control Unit").with_data_provider(provider);
    let app_provider = DataProviderBuilder::new()
        .read_data(
            "health_status",
            "Diagnostics Health Status",
            &DataCategory::CurrentData,
            Constant::new("ok")?,
        )
        .groups(["diagnostics"])
        .tags(["live", "status"])
        .translation_id("diag.health.status")
        .read_data(
            "event_count",
            "Diagnostic Event Count",
            &DataCategory::SysInfo,
            Constant::new(3_u32)?,
        )
        .groups(["diagnostics"])
        .tags(["counter"])
        .translation_id("diag.event.count")
        .build()?;
    let diag = App::new("diag", "Diagnostics App", "engine").with_data_provider(app_provider);

    let topology = Topology::new();
    {
        let mut t = topology.write().await;
        t.add_component(engine);
        t.add_app(diag);
    }

    let listener = TcpListener::bind("127.0.0.1:7691").await?;
    let server = Server::builder()
        .base_uri("http://127.0.0.1:7691/sovd")?
        .listener(listener)
        .topology(topology)
        .layer(libcli::trace::trace_layer())
        .build()?;

    tracing::info!(
           "Server running. Try: curl -s \
            http://localhost:7691/sovd/v1/components/engine/data/docs | jq; \
            curl -s http://localhost:7691/sovd/v1/apps/diag/data/docs | jq"
    );
    server.serve().await?;
    Ok(())
}
