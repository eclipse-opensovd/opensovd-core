// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

#![expect(clippy::unwrap_used)]

//! Integration tests for the SOVD fault resources.

mod common;

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use hyper::Method;
use opensovd_core::{
    Component, Fault, FaultDetail, FaultError, FaultFilter, FaultProvider, Topology,
};
use serde_json::{Map, Value};

#[derive(Clone)]
struct RecordingFaultProvider {
    seen: Arc<Mutex<Option<FaultFilter>>>,
    fail_list: bool,
}

#[async_trait]
impl FaultProvider for RecordingFaultProvider {
    async fn fault_list(&self, filter: FaultFilter) -> Result<Vec<Fault>, FaultError> {
        *self.seen.lock().unwrap() = Some(filter);
        if self.fail_list {
            Err(FaultError::Internal("secret provider detail".into()))
        } else {
            Ok(Vec::new())
        }
    }

    async fn fault_read(&self, fault_code: &str) -> Result<FaultDetail, FaultError> {
        Ok(FaultDetail {
            fault: fault(fault_code),
            environment_data: None,
        })
    }
}

fn fault(code: &str) -> Fault {
    Fault {
        code: code.into(),
        scope: Some("engine".into()),
        display_code: None,
        fault_name: format!("Fault {code}"),
        fault_translation_id: None,
        severity: Some(2),
        status: Some(Map::from_iter([("state".into(), Value::from("active"))])),
        tags: Vec::new(),
        symptom: None,
        symptom_translation_id: None,
    }
}

async fn server_with_provider(
    fail_list: bool,
) -> (common::TestServer, Arc<Mutex<Option<FaultFilter>>>) {
    let seen = Arc::new(Mutex::new(None));
    let provider = RecordingFaultProvider {
        seen: Arc::clone(&seen),
        fail_list,
    };
    let topology = Topology::new();
    topology
        .write()
        .await
        .add_component(Component::new("ecu", "Engine Control Unit").with_fault_provider(provider));
    let server = common::TestServer::builder()
        .topology(topology)
        .build()
        .await;
    (server, seen)
}

fn recorded_filter(seen: &Mutex<Option<FaultFilter>>) -> Option<FaultFilter> {
    seen.lock().unwrap().clone()
}

#[tokio::test]
async fn mock_fault_capabilities_are_advertised_only_when_supported() {
    let server = common::mock_server().await;

    for (path, collection) in [
        ("/sovd/v1/components/ecu", "/components/ecu/faults"),
        (
            "/sovd/v1/apps/engine_control",
            "/apps/engine_control/faults",
        ),
    ] {
        let (status, capabilities) = common::send(&server, Method::GET, path).await;
        assert_eq!(status, 200);
        assert_eq!(
            capabilities["faults"],
            format!("http://{}/sovd/v1{collection}", server.addr)
        );
    }

    let (status, gateway) = common::send(&server, Method::GET, "/sovd/v1/components/gateway").await;
    assert_eq!(status, 200);
    assert!(gateway.get("faults").is_none());

    let (status, error) =
        common::send(&server, Method::GET, "/sovd/v1/components/gateway/faults").await;
    assert_eq!(status, 404);
    assert_eq!(error["vendor_code"], "resource-not-found");
}

#[tokio::test]
async fn mock_list_and_detail_return_fault_data() {
    let server = common::mock_server().await;

    let (status, list) = common::send(
        &server,
        Method::GET,
        "/sovd/v1/components/ecu/faults?include-schema=true",
    )
    .await;
    assert_eq!(status, 200);
    assert!(list["schema"].is_object());
    let codes: Vec<_> = list["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| item["code"].as_str())
        .collect();
    assert_eq!(codes, ["P0001", "P0002"]);
    assert_eq!(
        list["items"][0]["fault_name"],
        "Fuel Volume Regulator Control Circuit/Open"
    );
    assert_eq!(list["items"][0]["tags"], serde_json::json!(["powertrain"]));

    let (status, detail) = common::send(
        &server,
        Method::GET,
        "/sovd/v1/components/ecu/faults/P0001?include-schema=true",
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(detail["item"]["code"], "P0001");
    assert_eq!(detail["environment_data"]["engineTemperature"], 93);
    assert!(detail["schema"].is_object());

    let (status, error) =
        common::send(&server, Method::GET, "/sovd/v1/components/ecu/faults/P9999").await;
    assert_eq!(status, 404);
    assert_eq!(error["vendor_code"], "resource-not-found");
}

#[tokio::test]
async fn list_query_passes_repeated_status_and_all_filters_to_provider() {
    let (server, seen) = server_with_provider(false).await;
    let (status, _) = common::send(
        &server,
        Method::GET,
        "/sovd/v1/components/ecu/faults?status%5Bstate%5D=active\
         &status%5Bstate%5D=inactive&severity=3&scope=engine&tags=OBD&tags=ePTI",
    )
    .await;
    assert_eq!(status, 200);

    let filter = recorded_filter(&seen);
    assert_eq!(
        filter.map(|filter| (filter.status, filter.severity, filter.scope, filter.tags)),
        Some((
            vec![
                ("state".into(), "active".into()),
                ("state".into(), "inactive".into()),
            ],
            Some(3),
            Some("engine".into()),
            vec!["OBD".into(), "ePTI".into()],
        ))
    );
}

#[tokio::test]
async fn list_without_query_passes_the_default_filter() {
    let (server, seen) = server_with_provider(false).await;
    let (status, _) = common::send(&server, Method::GET, "/sovd/v1/components/ecu/faults").await;
    assert_eq!(status, 200);

    let filter = recorded_filter(&seen);
    assert_eq!(
        filter.map(|filter| (filter.status, filter.severity, filter.scope, filter.tags)),
        Some((Vec::new(), None, None, Vec::new()))
    );
}

#[tokio::test]
async fn list_query_passes_tags_to_provider() {
    let (server, seen) = server_with_provider(false).await;
    let (status, _) = common::send(
        &server,
        Method::GET,
        "/sovd/v1/components/ecu/faults?tags=powertrain",
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(
        recorded_filter(&seen).map(|filter| filter.tags),
        Some(vec!["powertrain".into()])
    );
}

#[tokio::test]
async fn malformed_list_query_is_rejected_before_provider_call() {
    let (server, seen) = server_with_provider(false).await;
    let (status, error) = common::send(
        &server,
        Method::GET,
        "/sovd/v1/components/ecu/faults?severity=abc",
    )
    .await;
    assert_eq!(status, 400);
    assert_eq!(error["error_code"], "incomplete-request");
    assert!(recorded_filter(&seen).is_none());
}

#[tokio::test]
async fn detail_query_ignores_list_parameters() {
    let (server, _) = server_with_provider(false).await;
    let (status, detail) = common::send(
        &server,
        Method::GET,
        "/sovd/v1/components/ecu/faults/P0001?severity=abc&tags=unknown",
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(detail["item"]["code"], "P0001");
}

#[tokio::test]
async fn internal_provider_error_is_sanitized() {
    let (server, _) = server_with_provider(true).await;
    let (status, error) =
        common::send(&server, Method::GET, "/sovd/v1/components/ecu/faults").await;
    assert_eq!(status, 500);
    assert_eq!(error["error_code"], "sovd-server-failure");
    assert_eq!(error["message"], "An internal error occurred");
}
