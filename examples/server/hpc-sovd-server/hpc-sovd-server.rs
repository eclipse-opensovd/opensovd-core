// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! HPC SOVD server with dynamic app registration.
//!
//! Starts a SOVD server on port 7690 with a single "HPC" component that
//! exposes OS identification, hardware specs, and live system metrics.
//!
//! Apps self-register via POST http://127.0.0.1:7691/register and deregister via
//! DELETE http://127.0.0.1:7691/register/{app_id}. No server restart needed.
//!
//! Run with: `cargo run -p opensovd-examples-server --example hpc-sovd-server`

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use opensovd_core::{App, Component, Data, DataError, DataFilter, DataProvider, Metadata, Topology};
use opensovd_models::data::DataCategory;
use opensovd_providers::data::{Constant, DataProviderBuilder, ReadableDataResource, Value};
use axum::{Json, Router, extract::{Path, State}, http::StatusCode, routing::{delete, post}};
use opensovd_server::Server;
use sysinfo::{Components, Disks, System};
use tokio::net::TcpListener;

// ---------------------------------------------------------------------------
// Dynamic data resources
// ---------------------------------------------------------------------------

struct Uptime;

#[async_trait]
impl ReadableDataResource for Uptime {
    type Value = Value<f64>;
    #[allow(clippy::cast_precision_loss)]
    async fn read(&self) -> Result<Self::Value, opensovd_core::DataError> {
        Ok(Value::new(System::uptime() as f64))
    }
}

struct CpuUsage(Arc<Mutex<System>>);
impl CpuUsage {
    fn new(sys: &Arc<Mutex<System>>) -> Self { Self(Arc::clone(sys)) }
}
#[async_trait]
impl ReadableDataResource for CpuUsage {
    type Value = Value<f64>;
    async fn read(&self) -> Result<Self::Value, opensovd_core::DataError> {
        let cpu: f64 = {
            let mut sys = self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            sys.refresh_cpu_usage();
            sys.global_cpu_usage().into()
        };
        Ok(Value::new(cpu))
    }
}

struct MemoryUsage(Arc<Mutex<System>>);
impl MemoryUsage {
    fn new(sys: &Arc<Mutex<System>>) -> Self { Self(Arc::clone(sys)) }
}
#[async_trait]
impl ReadableDataResource for MemoryUsage {
    type Value = Value<f64>;
    #[allow(clippy::cast_precision_loss)]
    async fn read(&self) -> Result<Self::Value, opensovd_core::DataError> {
        let pct: f64 = {
            let mut sys = self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            sys.refresh_memory();
            let total = sys.total_memory() as f64;
            let used = sys.used_memory() as f64;
            if total > 0.0 { used / total * 100.0 } else { 0.0 }
        };
        Ok(Value::new(pct))
    }
}

struct MemoryUsedMb(Arc<Mutex<System>>);
impl MemoryUsedMb {
    fn new(sys: &Arc<Mutex<System>>) -> Self { Self(Arc::clone(sys)) }
}
#[async_trait]
impl ReadableDataResource for MemoryUsedMb {
    type Value = Value<f64>;
    #[allow(clippy::cast_precision_loss)]
    async fn read(&self) -> Result<Self::Value, opensovd_core::DataError> {
        let mb: f64 = {
            let mut sys = self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            sys.refresh_memory();
            sys.used_memory() as f64 / 1_048_576.0
        };
        Ok(Value::new(mb))
    }
}

struct StorageUsedMb;
#[async_trait]
impl ReadableDataResource for StorageUsedMb {
    type Value = Value<f64>;
    #[allow(clippy::cast_precision_loss)]
    async fn read(&self) -> Result<Self::Value, opensovd_core::DataError> {
        let disks = Disks::new_with_refreshed_list();
        let used_mb: f64 = disks.iter()
            .map(|d| d.total_space().saturating_sub(d.available_space()) as f64)
            .sum::<f64>() / 1_048_576.0;
        Ok(Value::new(used_mb))
    }
}

struct Temperature;
#[async_trait]
impl ReadableDataResource for Temperature {
    type Value = Value<f64>;
    async fn read(&self) -> Result<Self::Value, opensovd_core::DataError> {
        let components = Components::new_with_refreshed_list();
        let temp = components.iter()
            .find(|c| {
                let label = c.label().to_lowercase();
                label.contains("cpu") || label.contains("core") || label.contains("temp")
            })
            .or_else(|| components.iter().next())
            .and_then(sysinfo::Component::temperature)
            .map_or(0.0, f64::from);
        Ok(Value::new(temp))
    }
}

// ---------------------------------------------------------------------------
// Generic HTTP proxy — forwards requests to any app's diagnostic HTTP server
// ---------------------------------------------------------------------------

/// Registration request sent by an app via any IPC transport.
/// The JSON shape is the contract — transport is pluggable (REST, D-Bus, iceoryx2…).
#[derive(Debug, serde::Deserialize)]
struct AppRegistrationRequest {
    app_id: String,
    app_name: String,
    port: u16,
    hosted_on: String,
}

/// Data provider that proxies requests to an app's opensovd-diagnostic-lib HTTP server.
struct GenericHttpProxy {
    base_url: String,
    client: Arc<reqwest::Client>,
}

impl GenericHttpProxy {
    fn new(base_url: String, client: Arc<reqwest::Client>) -> Self {
        Self { base_url, client }
    }
}

#[async_trait]
impl DataProvider for GenericHttpProxy {
    async fn list(&self, _filter: DataFilter) -> Result<Vec<Metadata>, DataError> {
        let url = format!("{}/data", self.base_url);
        let response = self.client.get(&url)
            .send()
            .await
            .map_err(|e| DataError::Internal(e.to_string()))?;

        if !response.status().is_success() {
            return Err(DataError::Internal(format!("HTTP {}", response.status())));
        }

        let items: Vec<serde_json::Value> = response.json()
            .await
            .map_err(|e| DataError::Internal(e.to_string()))?;

        let data_list = items.into_iter().map(|item| {
            let id = item.get("id").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
            let name = item.get("name").and_then(|v| v.as_str()).unwrap_or(&id).to_string();
            let category_str = item.get("category").and_then(|v| v.as_str()).unwrap_or("CurrentData");
            let category = match category_str {
                "IdentData" | "identData" => "IdentData",
                "SysInfo" | "sysInfo" => "SysInfo",
                "ConfigData" | "configData" => "ConfigData",
                _ => "CurrentData",
            };

            Metadata {
                id,
                name,
                category: category.to_string(),
                translation_id: None,
                groups: vec![],
                tags: vec![],
                schema: None,
                is_readable: true,
                is_writable: item.get("is_writable").and_then(serde_json::Value::as_bool)
                    .or_else(|| item.get("writable").and_then(serde_json::Value::as_bool))
                    .unwrap_or(false),
            }
        }).collect();

        Ok(data_list)
    }

    async fn read(&self, id: &str, _include_schema: bool) -> Result<Data, DataError> {
        let url = format!("{}/data/{}", self.base_url, id);
        let response = self.client.get(&url)
            .send()
            .await
            .map_err(|e| DataError::Internal(e.to_string()))?;

        if !response.status().is_success() {
            return Err(DataError::NotFound(id.to_string()));
        }

        let data: serde_json::Value = response.json()
            .await
            .map_err(|e| DataError::Internal(e.to_string()))?;

        Ok(Data {
            data: data.get("value").cloned().unwrap_or(serde_json::Value::Null),
            schema: None,
        })
    }

    async fn write(&self, id: &str, value: serde_json::Value) -> Result<(), DataError> {
        let url = format!("{}/data/{}", self.base_url, id);
        let response = self.client.put(&url)
            .json(&serde_json::json!({"value": value}))
            .send()
            .await
            .map_err(|e| DataError::Internal(e.to_string()))?;

        if !response.status().is_success() {
            return Err(DataError::Internal(format!("HTTP {}", response.status())));
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Registration endpoint handler
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct RegistrationState {
    topology: Topology,
    client: Arc<reqwest::Client>,
}

async fn handle_registration(
    State(state): State<RegistrationState>,
    Json(req): Json<AppRegistrationRequest>,
) -> Json<serde_json::Value> {
    let proxy = GenericHttpProxy::new(format!("http://127.0.0.1:{}/api", req.port), state.client);
    let app = App::new(&req.app_id, &req.app_name, &req.hosted_on)
        .with_data_provider(proxy);

    {
        let mut topo = state.topology.write().await;
        topo.add_app(app);
    }

    tracing::info!(
        "Registered app '{}' ('{}') hosted on '{}' at port {}",
        req.app_id, req.app_name, req.hosted_on, req.port
    );

    Json(serde_json::json!({ "status": "registered", "app_id": req.app_id }))
}

async fn handle_deregistration(
    State(state): State<RegistrationState>,
    Path(app_id): Path<String>,
) -> StatusCode {
    let mut topo = state.topology.write().await;
    topo.remove_app(&app_id);
    tracing::info!("Deregistered app '{}'", app_id);
    StatusCode::OK
}

// ---------------------------------------------------------------------------

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    libcli::init_tracing("info", None)?;

    // OS identification via sysinfo (cross-platform)
    let os_name = System::name().unwrap_or_else(|| "Unknown".to_string());
    let os_version = System::os_version().unwrap_or_default();
    let os_pretty = System::long_os_version().unwrap_or_else(|| os_name.clone());
    let os_id = System::distribution_id();

    // CPU brand via sysinfo (cross-platform)
    let cpu_brand = {
        let sys_info = System::new_all();
        sys_info.cpus().first()
            .map_or_else(|| "Unknown Processor".to_string(), |c| c.brand().to_string())
    };

    let sys = Arc::new(Mutex::new(System::new()));

    #[allow(clippy::cast_precision_loss)]
    let mem_total_mb: f64 = {
        let mut s = System::new();
        s.refresh_memory();
        s.total_memory() as f64 / 1_048_576.0
    };

    #[allow(clippy::cast_precision_loss)]
    let storage_total_mb: f64 = {
        let disks = Disks::new_with_refreshed_list();
        disks.iter().map(|d| d.total_space() as f64).sum::<f64>() / 1_048_576.0
    };

    let builder = DataProviderBuilder::new()
        // OS
        .read_data("os.version", "OS Version", &DataCategory::IdentData, Constant::new(os_version)?)
        .read_data("os.name", "OS Name", &DataCategory::IdentData, Constant::new(os_name)?)
        .read_data("os.pretty_name", "OS Pretty Name", &DataCategory::IdentData, Constant::new(os_pretty)?)
        .read_data("os.id", "OS Identifier", &DataCategory::IdentData, Constant::new(os_id)?)
        .read_data("os.uptime", "System Uptime", &DataCategory::SysInfo, Uptime)
        // CPU / Memory
        .read_data("cpu.usage", "CPU Usage %", &DataCategory::SysInfo, CpuUsage::new(&sys))
        .read_data("mem.usage", "Memory Usage %", &DataCategory::SysInfo, MemoryUsage::new(&sys))
        .read_data("hw.processor", "Processor", &DataCategory::IdentData, Constant::new(cpu_brand)?)
        .read_data("mem.total", "Memory Total MB", &DataCategory::SysInfo, Constant::new(mem_total_mb)?)
        .read_data("mem.used", "Memory Used MB", &DataCategory::SysInfo, MemoryUsedMb::new(&sys))
        // Storage
        .read_data("hw.storage.total", "Storage Total MB", &DataCategory::SysInfo, Constant::new(storage_total_mb)?)
        .read_data("hw.storage.used", "Storage Used MB", &DataCategory::SysInfo, StorageUsedMb)
        // Temperature
        .read_data("hw.temperature", "CPU Temperature °C", &DataCategory::SysInfo, Temperature);

    let provider = builder.build()?;

    let component = Component::new("HPC", "HPC - v1").with_data_provider(provider);

    let topology = Topology::new();
    {
        let mut topo = topology.write().await;
        topo.add_component(component);
        // Apps register themselves at runtime — no hardcoded entries here
    }

    let http_client = Arc::new(reqwest::Client::new());

    // Spawn dynamic app registration endpoint on port 7691
    let reg_state = RegistrationState { topology: topology.clone(), client: Arc::clone(&http_client) };
    tokio::spawn(async move {
        let app = Router::new()
            .route("/register", post(handle_registration))
            .route("/register/{app_id}", delete(handle_deregistration))
            .with_state(reg_state);

        let Ok(listener) = TcpListener::bind("127.0.0.1:7691").await else {
            tracing::error!("Failed to bind registration port 7691");
            return;
        };
        tracing::info!("App registration endpoint on http://127.0.0.1:7691/register");
        if let Err(e) = axum::serve(listener, app).await {
            tracing::error!("Registration server error: {e}");
        }
    });

    let listener = TcpListener::bind("0.0.0.0:7690").await?;
    let server = Server::builder()
        .base_uri("http://127.0.0.1:7690/sovd")?
        .listener(listener)
        .topology(topology)
        .layer(libcli::trace::trace_layer())
        .build()?;

    tracing::info!("SOVD server running on port 7690");
    server.serve().await?;
    Ok(())
}
