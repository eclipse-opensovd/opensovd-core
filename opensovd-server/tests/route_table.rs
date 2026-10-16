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
async fn missing_entities_are_not_found() {
    let server = mock_server().await;

    for (method, path, entity) in [
        (Method::GET, "components/nope", "components/nope"),
        (Method::GET, "components/nope/data", "components/nope"),
        (Method::GET, "apps/nope", "apps/nope"),
        (Method::GET, "apps/nope/bulk-data", "apps/nope"),
        (Method::DELETE, "apps/nope/bulk-data/logs", "apps/nope"),
        (Method::DELETE, "apps/nope/logs/config", "apps/nope"),
        (Method::GET, "components/nope/data/x", "components/nope"),
        (
            Method::GET,
            "components/x%2Fsubcomponents%2Fy/data",
            "components/x%2Fsubcomponents%2Fy",
        ),
    ] {
        let path = format!("/sovd/v1/{path}");
        let (status, json) = send(&server, method.clone(), &path).await;
        assert_eq!(status, 404, "{method} {path}");
        assert_eq!(json["vendor_code"], "entity-not-found", "{method} {path}");
        assert_eq!(
            json["message"],
            format!("Entity not found: {entity}"),
            "{method} {path}"
        );
    }
}

#[tokio::test]
async fn a_malformed_request_answers_before_the_entity() {
    let server = mock_server().await;

    let query = "include-schema=maybe";
    for (method, path, status) in [
        (Method::GET, format!("areas/nope?{query}"), 400),
        (Method::GET, format!("areas/nope/contains?{query}"), 400),
        (Method::GET, format!("components/nope?{query}"), 400),
        (Method::GET, format!("components/nope/hosts?{query}"), 400),
        (Method::GET, format!("components/nope/data?{query}"), 400),
        (
            Method::GET,
            format!("components/ecu/bulk-data?{query}"),
            400,
        ),
        (Method::GET, format!("apps/nope?{query}"), 400),
        (Method::GET, format!("apps/nope/logs/entries?{query}"), 400),
        // Without a JSON body.
        (Method::PUT, "components/nope/data/x".to_owned(), 415),
        (Method::PUT, "apps/nope/logs/config".to_owned(), 415),
        // Without the upload headers.
        (Method::POST, "apps/nope/bulk-data/logs".to_owned(), 400),
    ] {
        let path = format!("/sovd/v1/{path}");
        let (got, json) = send(&server, method.clone(), &path).await;
        assert_eq!(got, status, "{method} {path}");
        assert_eq!(json["error_code"], "incomplete-request", "{method} {path}");
    }
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
        "/sovd/v1/components//data",
        "/sovd/v1/apps//status",
        "/sovd/v1/areas//contains",
        "/sovd/v1/functions//depends-on",
        "/sovd/v1/components//unknown",
        "/sovd/v1/components//subcomponents/injector",
        "/sovd/v1/components/ecu/subcomponents//data",
        "/sovd/v1/components/%2E/data",
        "/sovd/v1/apps/%2E%2E/status",
        "/sovd/v1/areas/powertrain/subareas/%2E/contains",
        "/sovd/v1/components/ecu/docs",
        "/sovd/v1/components/ecu/%FF",
        "/sovd/v1/components/nope/%FF",
        // Served collections without a provider.
        "/sovd/v1/components/ecu/bulk-data",
        "/sovd/v1/apps/engine_control/bulk-data",
        "/sovd/v1/components/ecu/logs",
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
