// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! ECU battery voltage monitor.
//!
//! Exposes 4 data items via opensovd-diagnostic-lib and self-registers with hpc-sovd-server.
//!
//! Run with: `cargo run -p MyApp --bin myapp`

use std::time::{Duration, SystemTime};

use async_trait::async_trait;
use opensovd_diagnostic_lib::registration::{AppEndpoint, HttpRegistrar};
use opensovd_diagnostic_lib::{
    DataCategory, DataItem, DataProvider, DataValue, DiagnosticError, DiagnosticServer, Result,
};

struct EcuMonitor {
    start_time: SystemTime,
}

impl EcuMonitor {
    fn uptime_secs(&self) -> u64 {
        self.start_time
            .elapsed()
            .unwrap_or(Duration::ZERO)
            .as_secs()
    }

    /// Simulates battery voltage oscillating between 11.8 V and 14.4 V.
    fn voltage(&self) -> f64 {
        let t = self.uptime_secs() as f64;
        13.1 + 1.3 * (t * 0.05).sin()
    }
}

#[async_trait]
impl DataProvider for EcuMonitor {
    async fn list_data(&self) -> Vec<DataItem> {
        vec![
            DataItem {
                id: "ecu.id".to_string(),
                name: "ECU Identifier".to_string(),
                category: DataCategory::IdentData,
                translation_id: None,
                groups: vec!["ecu".to_string()],
                tags: vec!["static".to_string()],
                schema: None,
                is_readable: true,
                is_writable: false,
            },
            DataItem {
                id: "ecu.version".to_string(),
                name: "Software Version".to_string(),
                category: DataCategory::IdentData,
                translation_id: None,
                groups: vec!["ecu".to_string()],
                tags: vec!["static".to_string()],
                schema: None,
                is_readable: true,
                is_writable: false,
            },
            DataItem {
                id: "battery.voltage".to_string(),
                name: "Battery Voltage".to_string(),
                category: DataCategory::CurrentData,
                translation_id: None,
                groups: vec!["battery".to_string()],
                tags: vec!["sensor".to_string(), "voltage".to_string()],
                schema: None,
                is_readable: true,
                is_writable: false,
            },
            DataItem {
                id: "battery.uptime".to_string(),
                name: "ECU Uptime".to_string(),
                category: DataCategory::SysInfo,
                translation_id: None,
                groups: vec!["battery".to_string()],
                tags: vec!["uptime".to_string()],
                schema: None,
                is_readable: true,
                is_writable: false,
            },
        ]
    }

    async fn read_data(&self, id: &str) -> Result<DataValue> {
        let value = match id {
            "ecu.id" => serde_json::json!("APP01-ECU-001"),
            "ecu.version" => serde_json::json!("1.0.0"),
            "battery.voltage" => serde_json::json!(self.voltage()),
            "battery.uptime" => serde_json::json!(self.uptime_secs()),
            _ => return Err(DiagnosticError::NotFound(id.to_string())),
        };
        Ok(DataValue {
            value,
            schema: None,
        })
    }

    async fn write_data(&self, id: &str, _value: serde_json::Value) -> Result<()> {
        Err(DiagnosticError::ReadOnly(id.to_string()))
    }
}

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().with_env_filter("info").init();

    tracing::info!("Starting APP01 ECU battery monitor");

    DiagnosticServer::new(
        EcuMonitor {
            start_time: SystemTime::now(),
        },
        8081,
    )
    .with_registration(
        HttpRegistrar::new("http://127.0.0.1:7691/register"),
        AppEndpoint {
            app_id: "APP01".to_string(),
            app_name: "APP01 ECU Monitor".to_string(),
            port: 8081,
            hosted_on: "HPC".to_string(),
        },
    )
    .serve()
    .await?;

    Ok(())
}
