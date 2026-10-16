// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

mod common;

use common::{mock_server, send};
use http_body_util::BodyExt;
use hyper::header::ALLOW;
use hyper::{Method, Request};

/// The path below the API version, as error messages name it.
fn api_path(path: &str) -> &str {
    path.trim_start_matches("/sovd/v1/")
}

#[tokio::test]
async fn routes_follow_the_entity_kind() {
    let server = mock_server().await;

    for path in [
        "/sovd/v1/areas/powertrain/data",
        "/sovd/v1/areas/powertrain/faults",
        "/sovd/v1/areas/powertrain/subcomponents",
        "/sovd/v1/components/ecu/subareas",
        "/sovd/v1/components/ecu/contains",
        "/sovd/v1/apps/engine_control/hosts",
        "/sovd/v1/apps/engine_control/subcomponents",
        "/sovd/v1/components/ecu/subcomponents/injector/unknown",
        "/sovd/v1/components/nope/unknown",
        "/sovd/v1/unknown/x/data",
        "/sovd/v1/apps//status",
        "/sovd/v1/functions//depends-on",
        "/sovd/v1/components//unknown",
        "/sovd/v1/components//subcomponents/injector",
        "/sovd/v1/components/ecu/subcomponents//data",
        "/sovd/v1/apps/%2E%2E/status",
        "/sovd/v1/areas/powertrain/subareas/%2E/contains",
        "/sovd/v1/components/ecu/docs",
        "/sovd/v1/components/ecu/%FF",
        "/sovd/v1/components/nope/%FF",
    ] {
        let (status, json) = send(&server, Method::GET, path).await;
        assert_eq!(status, 404, "{path}");
        assert_eq!(json["error_code"], "vendor-specific", "{path}");
        assert_eq!(json["vendor_code"], "resource-not-found", "{path}");
        assert_eq!(
            json["message"],
            format!("Resource not found: {}", api_path(path)),
            "{path}"
        );
    }
}

#[tokio::test]
async fn wrong_methods_answer_a_json_405() {
    let server = mock_server().await;

    for (method, path, allow) in [
        (Method::POST, "/sovd/v1/components/ecu/data", "GET,HEAD"),
        (Method::OPTIONS, "/sovd/version-info", "GET,HEAD"),
    ] {
        let request = Request::builder()
            .method(method.clone())
            .uri(server.url(path))
            .body(http_body_util::Empty::<bytes::Bytes>::new())
            .unwrap();
        let response = common::client().request(request).await.unwrap();
        assert_eq!(response.status(), 405, "{method} {path}");
        assert_eq!(response.headers()[ALLOW], allow, "{method} {path}");
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["vendor_code"], "method-not-allowed", "{method} {path}");
    }
}
