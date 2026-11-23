// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

#![expect(clippy::unwrap_used)]

//! Integration tests for the SOVD fault resources.

mod common;

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use async_trait::async_trait;
use hyper::Method;
use opensovd_core::{
    Component, Fault, FaultDataError, FaultDetail, FaultError, FaultFilter, FaultProvider, Topology,
};
use serde_json::{Map, Value};

#[derive(Clone, Default)]
struct RecordingFaultProvider {
    seen: Arc<Mutex<Option<FaultFilter>>>,
    fail_list: bool,
    read_schema: Option<Value>,
    read_errors: Vec<FaultDataError>,
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

    async fn fault_read(
        &self,
        fault_code: &str,
        _include_schema: bool,
    ) -> Result<FaultDetail, FaultError> {
        if fault_code != "P0001" {
            return Err(FaultError::NotFound(fault_code.to_owned()));
        }
        Ok(FaultDetail {
            fault: fault(fault_code),
            environment_data: None,
            errors: self.read_errors.clone(),
            schema: self.read_schema.clone(),
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

async fn server_with(provider: impl FaultProvider) -> common::TestServer {
    let topology = Topology::new();
    topology
        .write()
        .await
        .add_component(Component::new("ecu", "Engine Control Unit").with_fault_provider(provider));
    common::TestServer::builder()
        .topology(topology)
        .build()
        .await
}

/// A server and the filter its provider was last listed with.
async fn recording_server() -> (common::TestServer, Arc<Mutex<Option<FaultFilter>>>) {
    let provider = RecordingFaultProvider::default();
    let seen = Arc::clone(&provider.seen);
    (server_with(provider).await, seen)
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
    assert_eq!(codes, ["fuelPressureLow", "coolantSensorSignalLost"]);
    assert_eq!(
        list["items"][0]["fault_name"],
        "Fuel rail pressure below limit"
    );
    assert_eq!(list["items"][0]["tags"], serde_json::json!(["powertrain"]));
    assert_eq!(list["items"][1]["tags"], serde_json::json!(["emissions"]));

    let (status, detail) = common::send(
        &server,
        Method::GET,
        "/sovd/v1/components/ecu/faults/fuelPressureLow?include-schema=true",
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(detail["item"]["code"], "fuelPressureLow");
    assert_eq!(detail["environment_data"]["engineTemperature"], 93);
    assert!(detail.get("errors").is_none());
    assert!(detail["schema"].is_object());
    assert_eq!(
        detail.pointer("/schema/properties/environment_data"),
        Some(&serde_json::json!({
            "type": "object",
            "properties": { "engineTemperature": { "type": "integer" } },
            "required": ["engineTemperature"]
        }))
    );

    let (status, error) =
        common::send(&server, Method::GET, "/sovd/v1/components/ecu/faults/P9999").await;
    assert_eq!(status, 404);
    assert_eq!(error["vendor_code"], "resource-not-found");
}

#[tokio::test]
async fn list_query_passes_repeated_status_and_all_filters_to_provider() {
    let (server, seen) = recording_server().await;
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
    let (server, seen) = recording_server().await;
    let (status, _) = common::send(&server, Method::GET, "/sovd/v1/components/ecu/faults").await;
    assert_eq!(status, 200);

    let filter = recorded_filter(&seen);
    assert_eq!(
        filter.map(|filter| (filter.status, filter.severity, filter.scope, filter.tags)),
        Some((Vec::new(), None, None, Vec::new()))
    );
}

#[tokio::test]
async fn empty_tags_parameter_passes_no_tag_filter() {
    let (server, seen) = recording_server().await;
    let (status, _) =
        common::send(&server, Method::GET, "/sovd/v1/components/ecu/faults?tags=").await;
    assert_eq!(status, 200);
    assert_eq!(
        recorded_filter(&seen).map(|filter| filter.tags),
        Some(Vec::new())
    );
}

#[tokio::test]
async fn malformed_list_query_is_rejected_before_provider_call() {
    let (server, seen) = recording_server().await;
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
async fn malformed_status_filter_keys_are_rejected_before_provider_call() {
    let (server, seen) = recording_server().await;
    for query in [
        "status%5BtestFailed=1",
        "status%5B%5D=1",
        "status%5Ba%5D%5Bb%5D=1",
        "status%5BtestFailed%5Dx=1",
    ] {
        let (status, error) = common::send(
            &server,
            Method::GET,
            &format!("/sovd/v1/components/ecu/faults?{query}"),
        )
        .await;
        assert_eq!(status, 400, "{query}");
        assert_eq!(error["error_code"], "incomplete-request", "{query}");
        assert!(recorded_filter(&seen).is_none(), "{query}");
    }
}

#[tokio::test]
async fn detail_query_ignores_list_parameters() {
    let server = server_with(RecordingFaultProvider::default()).await;
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
async fn detail_schema_is_omitted_when_not_requested() {
    let server = server_with(RecordingFaultProvider {
        read_schema: Some(serde_json::json!({"type": "object"})),
        ..RecordingFaultProvider::default()
    })
    .await;
    let (status, detail) =
        common::send(&server, Method::GET, "/sovd/v1/components/ecu/faults/P0001").await;
    assert_eq!(status, 200);
    assert!(detail.get("schema").is_none());
}

#[tokio::test]
async fn detail_schema_uses_true_when_provider_has_no_schema() {
    let server = server_with(RecordingFaultProvider::default()).await;
    let (status, detail) = common::send(
        &server,
        Method::GET,
        "/sovd/v1/components/ecu/faults/P0001?include-schema=true",
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(
        detail.pointer("/schema/properties/environment_data"),
        Some(&Value::Bool(true))
    );
}

#[tokio::test]
async fn detail_maps_environment_data_errors_to_model_errors() {
    let server = server_with(RecordingFaultProvider {
        read_errors: vec![FaultDataError {
            path: "/temperature".into(),
            message: "sensor unavailable".into(),
        }],
        ..RecordingFaultProvider::default()
    })
    .await;
    let (status, detail) =
        common::send(&server, Method::GET, "/sovd/v1/components/ecu/faults/P0001").await;
    assert_eq!(status, 200);
    assert_eq!(detail["errors"][0]["path"], "/environment_data/temperature");
    assert_eq!(detail["errors"][0]["error"]["error_code"], "error-response");
}

#[tokio::test]
async fn internal_provider_error_is_sanitized() {
    let server = server_with(RecordingFaultProvider {
        fail_list: true,
        ..RecordingFaultProvider::default()
    })
    .await;
    let (status, error) =
        common::send(&server, Method::GET, "/sovd/v1/components/ecu/faults").await;
    assert_eq!(status, 500);
    assert_eq!(error["error_code"], "sovd-server-failure");
    assert_eq!(error["message"], "An internal error occurred");
}

#[tokio::test]
async fn deleting_all_faults_of_a_supported_entity_answers_not_implemented() {
    let server = server_with(RecordingFaultProvider::default()).await;
    let (status, error) =
        common::send(&server, Method::DELETE, "/sovd/v1/components/ecu/faults").await;
    assert_eq!(status, 501);
    assert_eq!(error["error_code"], "error-response");
}

#[tokio::test]
async fn deleting_an_existing_fault_answers_not_implemented() {
    let server = server_with(RecordingFaultProvider::default()).await;
    let (status, error) = common::send(
        &server,
        Method::DELETE,
        "/sovd/v1/components/ecu/faults/P0001",
    )
    .await;
    assert_eq!(status, 501);
    assert_eq!(error["error_code"], "error-response");
}

#[tokio::test]
async fn deleting_an_unknown_fault_answers_resource_not_found() {
    let server = server_with(RecordingFaultProvider::default()).await;
    let (status, error) = common::send(
        &server,
        Method::DELETE,
        "/sovd/v1/components/ecu/faults/P9999",
    )
    .await;
    assert_eq!(status, 404);
    assert_eq!(error["vendor_code"], "resource-not-found");
}

#[tokio::test]
async fn deleting_faults_without_a_provider_answers_resource_not_found() {
    let server = common::mock_server().await;
    for path in [
        "/sovd/v1/components/gateway/faults",
        "/sovd/v1/components/gateway/faults/P0001",
    ] {
        let (status, error) = common::send(&server, Method::DELETE, path).await;
        assert_eq!(status, 404, "{path}");
        assert_eq!(error["vendor_code"], "resource-not-found", "{path}");
    }
}

#[tokio::test]
async fn deleting_faults_of_a_missing_entity_answers_entity_not_found() {
    let server = common::mock_server().await;
    for path in [
        "/sovd/v1/components/nope/faults",
        "/sovd/v1/components/nope/faults/P0001",
    ] {
        let (status, error) = common::send(&server, Method::DELETE, path).await;
        assert_eq!(status, 404, "{path}");
        assert_eq!(error["vendor_code"], "entity-not-found", "{path}");
    }
}

/// A provider that takes the topology write lock while it answers.
struct TopologyWritingFaultProvider {
    topology: Topology,
}

#[async_trait]
impl FaultProvider for TopologyWritingFaultProvider {
    async fn fault_list(&self, _filter: FaultFilter) -> Result<Vec<Fault>, FaultError> {
        drop(self.topology.write().await);
        Ok(Vec::new())
    }

    async fn fault_read(
        &self,
        fault_code: &str,
        _include_schema: bool,
    ) -> Result<FaultDetail, FaultError> {
        drop(self.topology.write().await);
        Ok(FaultDetail {
            fault: fault(fault_code),
            environment_data: None,
            errors: Vec::new(),
            schema: None,
        })
    }
}

#[tokio::test]
async fn topology_lock_is_released_before_the_provider_is_awaited() {
    let topology = Topology::new();
    let provider = TopologyWritingFaultProvider {
        topology: topology.clone(),
    };
    topology
        .write()
        .await
        .add_component(Component::new("ecu", "Engine Control Unit").with_fault_provider(provider));
    let server = common::TestServer::builder()
        .topology(topology)
        .build()
        .await;

    for (method, path) in [
        (Method::GET, "/sovd/v1/components/ecu/faults"),
        (Method::GET, "/sovd/v1/components/ecu/faults/P0001"),
        (Method::DELETE, "/sovd/v1/components/ecu/faults/P0001"),
    ] {
        let answer = tokio::time::timeout(
            Duration::from_secs(5),
            common::send(&server, method.clone(), path),
        )
        .await;
        assert!(answer.is_ok(), "{method} {path} did not answer");
    }
}
