// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

#![allow(clippy::expect_used, clippy::indexing_slicing)]

//! Simple example demonstrating a Linux system component with real data.
//!
//! Starts a server on port 7690 with a single "Linux" component that
//! exposes os-release identification data and live system uptime.
//!
//! Run with: `cargo run -p opensovd-examples-server --example simple`

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use async_trait::async_trait;
use opensovd_core::Component;
use opensovd_models::data::DataCategory;
use opensovd_providers::data::{Constant, DataProviderBuilder, ReadableDataResource, Value};
use opensovd_server::{Server, Topology};
use sysinfo::System;
use tokio::net::TcpListener;

const OS_RELEASE_FALLBACK: &str = "\
NAME=Fedora Linux
VERSION=43 (Workstation Edition)
ID=fedora
VERSION_ID=43
PRETTY_NAME=Fedora Linux 43 (Workstation Edition)";

/// Parse `/etc/os-release` into key=value `HashMap`.
async fn parse_os_release() -> HashMap<String, String> {
    let content = tokio::fs::read_to_string("/etc/os-release")
        .await
        .unwrap_or_else(|_| OS_RELEASE_FALLBACK.to_string());
    let mut map = HashMap::new();
    for line in content.lines() {
        if let Some((key, value)) = line.split_once('=') {
            map.insert(key.to_string(), value.trim_matches('"').to_string());
        }
    }
    map
}

/// Dynamic data resource that reads `/proc/uptime` on each request.
/// Falls back to time since app start on non-Linux platforms.
struct Uptime(Instant);

impl Uptime {
    fn new() -> Self {
        Self(Instant::now())
    }
}

#[async_trait]
impl ReadableDataResource for Uptime {
    type Value = Value<f64>;

    async fn read(&self) -> Result<Self::Value, opensovd_core::DataError> {
        let seconds: f64 = tokio::fs::read_to_string("/proc/uptime")
            .await
            .ok()
            .and_then(|c| c.split_whitespace().next()?.parse().ok())
            .unwrap_or_else(|| self.0.elapsed().as_secs_f64());
        Ok(Value::new(seconds))
    }
}

/// Dynamic data resource that reports global average CPU usage as a percentage.
struct CpuUsage(Arc<Mutex<System>>);

impl CpuUsage {
    fn new(sys: &Arc<Mutex<System>>) -> Self {
        Self(Arc::clone(sys))
    }
}

#[async_trait]
impl ReadableDataResource for CpuUsage {
    type Value = Value<f64>;

    async fn read(&self) -> Result<Self::Value, opensovd_core::DataError> {
        let cpu: f64 = {
            let mut sys = self
                .0
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            sys.refresh_cpu_usage();
            sys.global_cpu_usage().into()
        };
        Ok(Value::new(cpu))
    }
}

/// Dynamic data resource that reports memory usage as a percentage.
struct MemoryUsage(Arc<Mutex<System>>);

impl MemoryUsage {
    fn new(sys: &Arc<Mutex<System>>) -> Self {
        Self(Arc::clone(sys))
    }
}

#[async_trait]
impl ReadableDataResource for MemoryUsage {
    type Value = Value<f64>;

    async fn read(&self) -> Result<Self::Value, opensovd_core::DataError> {
        let pct: f64 = {
            let mut sys = self
                .0
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            sys.refresh_memory();
            #[allow(clippy::cast_precision_loss)]
            let total = sys.total_memory() as f64;
            #[allow(clippy::cast_precision_loss)]
            let used = sys.used_memory() as f64;
            if total > 0.0 {
                used / total * 100.0
            } else {
                0.0
            }
        };
        Ok(Value::new(pct))
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    libcli::init_tracing("info", None)?;

    // Parse os-release at startup (static data)
    let os = parse_os_release().await;

    // Shared sysinfo::System instance for CPU and memory metrics
    let sys = Arc::new(Mutex::new(System::new()));

    // Build data provider with os-release constants + dynamic uptime
    let provider = DataProviderBuilder::new()
        .read_data(
            "os.version",
            "OS Version",
            &DataCategory::IdentData,
            Constant::new(os["VERSION_ID"].clone())?,
        )
        .read_data(
            "os.name",
            "OS Name",
            &DataCategory::IdentData,
            Constant::new(os["NAME"].clone())?,
        )
        .read_data(
            "os.pretty_name",
            "OS Pretty Name",
            &DataCategory::IdentData,
            Constant::new(os["PRETTY_NAME"].clone())?,
        )
        .read_data(
            "os.id",
            "OS Identifier",
            &DataCategory::IdentData,
            Constant::new(os["ID"].clone())?,
        )
        .read_data(
            "os.uptime",
            "System Uptime",
            &DataCategory::SysInfo,
            Uptime::new(),
        )
        .read_data(
            "cpu.usage",
            "CPU Usage",
            &DataCategory::SysInfo,
            CpuUsage::new(&sys),
        )
        .read_data(
            "mem.usage",
            "Memory Usage",
            &DataCategory::SysInfo,
            MemoryUsage::new(&sys),
        )
        .build()?;

    // Single "Linux" component with the provider
    let linux = Component::new("linux", &os["PRETTY_NAME"]).with_data_provider(provider);

    let topology = Topology::new();
    {
        let mut t = topology.write().await;
        t.add_component(linux);
    }

    let listener = TcpListener::bind("127.0.0.1:7690").await?;
    let server = Server::builder()
        .base_uri("http://127.0.0.1:7690/sovd")?
        .listener(listener)
        .topology(topology)
        .layer(libcli::trace::trace_layer())
        .build()?;

    tracing::info!("Server running");
    server.serve().await?;
    Ok(())
}
