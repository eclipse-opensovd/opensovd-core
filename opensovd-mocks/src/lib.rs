// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Mock data helpers for OpenSOVD tests and examples.

use std::collections::HashMap;

use opensovd_core::{App, Area, Component, Topology};
use opensovd_models::data::DataCategory;
use opensovd_providers::data::{Constant, DataProviderBuilder};

/// Creates a mock topology with sample ECU, gateway, and app entities.
///
/// # Panics
///
/// Panics if constant data values cannot be serialized (should not happen with valid literals).
#[must_use]
#[allow(clippy::too_many_lines, clippy::unwrap_used)]
pub async fn create_mock_topology() -> Topology {
    // ECU entity with sensor data and version info
    #[rustfmt::skip]
    let ecu_provider = DataProviderBuilder::new()
        .read_data(
            "voltage", "Battery Voltage",
            &DataCategory::CurrentData,
            Constant::new(12.6).unwrap(),
        )
            .groups(["power"])
            .tags(["sensor"])
        .read_data(
            "temperature", "Engine Temperature",
            &DataCategory::CurrentData,
            Constant::new(85.0).unwrap(),
        )
            .groups(["thermal"])
            .tags(["sensor"])
        .read_data(
            "sw.version", "Software Version",
            &DataCategory::IdentData,
            Constant::new("2.1.0").unwrap(),
        )
        .read_data(
            "sw.build_date", "Build Date",
            &DataCategory::IdentData,
            Constant::new("2025-12-15").unwrap(),
        )
        .read_data(
            "sw.sha1", "Git SHA1",
            &DataCategory::IdentData,
            Constant::new("b5d022b").unwrap(),
        )
        .read_data(
            "hw.version", "Hardware Version",
            &DataCategory::IdentData,
            Constant::new("1.0").unwrap(),
        )
        .read_data(
            "hw.revision", "Hardware Revision",
            &DataCategory::IdentData,
            Constant::new("A").unwrap(),
        )
        .read_data(
            "hw.sn", "Hardware Serial Number",
            &DataCategory::IdentData,
            Constant::new("ECU-HW-001-2025").unwrap(),
        )
        .build()
        .unwrap();

    let ecu = Component::new("ecu", "Engine Control Unit")
        .with_translation_id("ecu.name")
        .with_tags(vec!["powertrain".to_string(), "critical".to_string()])
        .with_metadata(HashMap::from([
            ("variant".to_string(), "v2".to_string()),
            ("manufacturer".to_string(), "ACME".to_string()),
        ]))
        .with_area_id("powertrain")
        .with_data_provider(ecu_provider);

    // Gateway entity with version data
    #[rustfmt::skip]
    let gateway_provider = DataProviderBuilder::new()
        .read_data(
            "sw.version", "Software Version",
            &DataCategory::IdentData,
            Constant::new("0.1.0-mock").unwrap(),
        )
        .read_data(
            "sw.build_date", "Build Date",
            &DataCategory::IdentData,
            Constant::new("2026-01-01").unwrap(),
        )
        .read_data(
            "sw.sha1", "Git SHA1",
            &DataCategory::IdentData,
            Constant::new("a4c011a").unwrap(),
        )
        .read_data(
            "hw.version", "Hardware Version",
            &DataCategory::IdentData,
            Constant::new("1.0").unwrap(),
        )
        .read_data(
            "hw.revision", "Hardware Revision",
            &DataCategory::IdentData,
            Constant::new("B").unwrap(),
        )
        .read_data(
            "hw.sn", "Hardware Serial Number",
            &DataCategory::IdentData,
            Constant::new("GW-HW-001-2025").unwrap(),
        )
        .read_data(
            "uptime", "System Uptime",
            &DataCategory::CurrentData,
            Constant::new(3600u64).unwrap(),
        )
        .build()
        .unwrap();

    let gateway = Component::new("gateway", "Vehicle Gateway")
        .with_tags(vec!["network".to_string()])
        .with_metadata(HashMap::from([(
            "firmware".to_string(),
            "1.2.3".to_string(),
        )]))
        .with_area_id("network")
        .with_data_provider(gateway_provider);

    // App 1: "engine_control" - Powertrain app with area membership
    #[rustfmt::skip]
    let engine_control_provider = DataProviderBuilder::new()
        .read_data(
            "app.version", "Application Version",
            &DataCategory::IdentData,
            Constant::new("3.2.1").unwrap(),
        )
        .read_data(
            "app.status", "Application Status",
            &DataCategory::CurrentData,
            Constant::new("running").unwrap(),
        )
        .read_data(
            "fuel_injection.rate", "Fuel Injection Rate",
            &DataCategory::CurrentData,
            Constant::new(2.5).unwrap(),
        )
            .tags(["sensor"])
        .build()
        .unwrap();

    let engine_control = App::new("engine_control", "Engine Control Application", "ecu")
        .with_translation_id("app.engine_control.name")
        .with_tags(vec!["powertrain".to_string(), "critical".to_string()])
        .with_metadata(HashMap::from([
            ("app_type".to_string(), "realtime".to_string()),
            ("priority".to_string(), "high".to_string()),
        ]))
        .with_area_id("powertrain")
        .with_data_provider(engine_control_provider);

    // App 2: "diagnostics" - Network app with minimal metadata
    #[rustfmt::skip]
    let diagnostics_provider = DataProviderBuilder::new()
        .read_data(
            "app.version", "Application Version",
            &DataCategory::IdentData,
            Constant::new("1.0.5").unwrap(),
        )
        .read_data(
            "active_connections", "Active Diagnostic Connections",
            &DataCategory::CurrentData,
            Constant::new(3u32).unwrap(),
        )
        .read_data(
            "messages_per_second", "Message Throughput",
            &DataCategory::CurrentData,
            Constant::new(150u32).unwrap(),
        )
        .build()
        .unwrap();

    let diagnostics = App::new("diagnostics", "Diagnostic Services", "gateway")
        .with_tags(vec!["network".to_string(), "service".to_string()])
        .with_area_id("network")
        .with_data_provider(diagnostics_provider);

    // App 3: "ota_manager" - No area membership (edge case)
    #[rustfmt::skip]
    let ota_provider = DataProviderBuilder::new()
        .read_data(
            "app.version", "Application Version",
            &DataCategory::IdentData,
            Constant::new("2.0.0").unwrap(),
        )
        .read_data(
            "update_available", "Update Available",
            &DataCategory::CurrentData,
            Constant::new(false).unwrap(),
        )
        .read_data(
            "last_check", "Last Update Check",
            &DataCategory::CurrentData,
            Constant::new("2026-01-28T10:00:00Z").unwrap(),
        )
        .build()
        .unwrap();

    let ota_manager = App::new("ota_manager", "OTA Update Manager", "gateway")
        .with_translation_id("app.ota.name")
        .with_tags(vec!["system".to_string(), "infrastructure".to_string()])
        .with_metadata(HashMap::from([(
            "update_channel".to_string(),
            "stable".to_string(),
        )]))
        .with_data_provider(ota_provider);
    // Note: NO with_area_id() - tests optional belongs-to

    // Areas
    let powertrain =
        Area::new("powertrain", "Powertrain Domain").with_tags(vec!["domain".to_string()]);

    let network = Area::new("network", "Network Domain").with_tags(vec!["domain".to_string()]);

    let topology = Topology::new();
    {
        let mut t = topology.write().await;
        t.add_component(ecu);
        t.add_component(gateway);
        t.add_app(engine_control);
        t.add_app(diagnostics);
        t.add_app(ota_manager);
        t.add_area(powertrain);
        t.add_area(network);
    }
    topology
}
