// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Use the gateway through its public API, as an embedding application does.

use opensovd_core::{Component, Topology};
use opensovd_gateway::{cli, run_with_shutdown};
use opensovd_models::data::DataCategory;
use opensovd_providers::data::{Constant, DataProviderBuilder};

#[tokio::test]
async fn application_supplies_a_topology_and_handles_configuration_errors() {
    let provider = DataProviderBuilder::new()
        .read_data(
            "version",
            "Version",
            &DataCategory::IdentData,
            Constant::new("1.0.0").unwrap(),
        )
        .build()
        .unwrap();
    let topology = Topology::default();
    topology
        .write()
        .await
        .add_component(Component::new("component", "Component").with_data_provider(provider));
    let options = cli::try_parse_from([
        "gateway",
        "--url",
        "http://127.0.0.1:0/sovd",
        "--cors-origin",
        "*",
        "--cors-credentials",
    ])
    .unwrap();
    let error = run_with_shutdown(options, topology, std::future::pending())
        .await
        .unwrap_err();
    assert!(error.to_string().contains("wildcard '*'"));
}
