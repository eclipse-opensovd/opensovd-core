// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use http::{Request, Response, StatusCode};
use http_body_util::{BodyExt, Empty, Full};
use hyper::body::Incoming;
use opensovd_mocks::create_mock_topology;
use serde_json::json;
use tokio::time::sleep;

mod common;

async fn mock_server() -> common::TestServer {
    common::TestServer::builder()
        .topology(create_mock_topology().await)
        .build()
        .await
}

async fn request(
    url: &str,
    method: http::Method,
    body: Option<serde_json::Value>,
) -> Result<Response<Incoming>, Box<dyn std::error::Error>> {
    if let Some(json) = body {
        let client = common::client();
        let request = Request::builder()
            .method(method)
            .uri(url)
            .header("Content-Type", "application/json")
            .body(Full::<bytes::Bytes>::from(json.to_string()))?;
        Ok(client.request(request).await?)
    } else {
        let client = common::client();
        let request = Request::builder()
            .method(method)
            .uri(url)
            .header("Content-Type", "application/json")
            .body(Empty::<bytes::Bytes>::new())?;
        Ok(client.request(request).await?)
    }
}

#[rstest::rstest]
#[case("")]
#[case("apps/ota_manager/")]
#[tokio::test]
async fn update_lifecycle(#[case] entity_path: &str) {
    let server = mock_server().await;

    // Initially, the list of updates should be empty.
    let response = request(
        &server.url(&format!("/sovd/v1/{entity_path}updates")),
        http::Method::GET,
        None,
    )
    .await
    .unwrap();
    assert!(response.status().is_success());

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        json["items"].as_array().unwrap().as_slice(),
        [] as [&str; 0]
    );

    // Create a new update.
    let response = request(
        &server.url(&format!("/sovd/v1/{entity_path}updates")),
        http::Method::POST,
        Some(json!({
        "id": "firmware-v1",
        "update_name": "Firmware v1",
        "size": 0,
        "origin": ["remote", "proximity"],
        "targets": []
        })),
    )
    .await
    .unwrap();
    assert!(response.status().is_success());
    assert!(response.headers().contains_key("location"));
    assert_eq!(
        response.headers().get("location").unwrap(),
        &server.url(&format!("/sovd/v1/{entity_path}updates/firmware-v1"))
    );
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["id"].as_str().unwrap(), "firmware-v1");

    // Verify that the newly created update appears in the list.
    let response = request(
        &server.url(&format!("/sovd/v1/{entity_path}updates")),
        http::Method::GET,
        None,
    )
    .await
    .unwrap();
    assert!(response.status().is_success());

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["items"].as_array().unwrap().len(), 1);
    assert_eq!(json["items"][0].as_str().unwrap(), "firmware-v1");

    // Verify that the details of the newly created update can be retrieved.
    let response = request(
        &server.url(&format!("/sovd/v1/{entity_path}updates/firmware-v1")),
        http::Method::GET,
        None,
    )
    .await
    .unwrap();
    assert!(response.status().is_success());

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["id"].as_str().unwrap(), "firmware-v1");
    assert_eq!(json["update_name"].as_str().unwrap(), "Firmware v1");
    assert_eq!(json["size"].as_u64().unwrap(), 0);

    // Verify that the new update can be deleted.
    let response = request(
        &server.url(&format!("/sovd/v1/{entity_path}updates/firmware-v1")),
        http::Method::DELETE,
        None,
    )
    .await
    .unwrap();
    assert!(response.status().is_success());
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    // Verify that the update no longer appears in the list.
    let response = request(
        &server.url(&format!("/sovd/v1/{entity_path}updates")),
        http::Method::GET,
        None,
    )
    .await
    .unwrap();
    assert!(response.status().is_success());

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        json["items"].as_array().unwrap().as_slice(),
        [] as [&str; 0]
    );
}

#[rstest::rstest]
#[case("", &["remote-v1", "both-v1", "no-origin-v2"])]
#[case("origin=remote", &["remote-v1", "both-v1", "no-origin-v2"])]
#[case("origin=proximity", &["proximity-v1", "both-v1", "no-origin-v2"])]
#[case("target-version=v1.2", &["both-v1"])]
#[case("origin=proximity&target-version=beta", &["no-origin-v2"])]
#[tokio::test]
async fn update_query(#[case] query_string: &str, #[case] expected_ids: &[&str]) {
    let server = mock_server().await;

    // Create an update that should only be listed for the "proximity" origin.
    let response = request(
        &server.url("/sovd/v1/apps/ota_manager/updates"),
        http::Method::POST,
        Some(json!({
        "id": "proximity-v1",
        "update_name": "Proximity v1",
        "size": 0,
        "targets": [],
        "origin": ["proximity"]
        })),
    )
    .await
    .unwrap();
    assert!(response.status().is_success());

    // Create an update that should only be listed for the "remote" origin.
    let response = request(
        &server.url("/sovd/v1/apps/ota_manager/updates"),
        http::Method::POST,
        Some(json!({
        "id": "remote-v1",
        "update_name": "Remote v1.1",
        "size": 0,
        "targets": [],
        "origin": ["remote"]
        })),
    )
    .await
    .unwrap();
    assert!(response.status().is_success());

    // Create an update that should be listed for both origins.
    let response = request(
        &server.url("/sovd/v1/apps/ota_manager/updates"),
        http::Method::POST,
        Some(json!({
        "id": "both-v1",
        "update_name": "Both v1.2a",
        "size": 0,
        "targets": [],
        "origin": ["proximity", "remote"]
        })),
    )
    .await
    .unwrap();
    assert!(response.status().is_success());

    // Create an update that omits the origin field.
    let response = request(
        &server.url("/sovd/v1/apps/ota_manager/updates"),
        http::Method::POST,
        Some(json!({
        "id": "no-origin-v2",
        "update_name": "No Origin v2.0beta",
        "size": 0,
        "targets": []
        })),
    )
    .await
    .unwrap();
    assert!(response.status().is_success());

    // Query updates.
    let response = request(
        &server.url(&format!("/sovd/v1/apps/ota_manager/updates?{query_string}")),
        http::Method::GET,
        None,
    )
    .await
    .unwrap();
    assert!(response.status().is_success());
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["items"].as_array().unwrap().as_slice(), expected_ids);
}

#[rstest::rstest]
#[case("prepare")]
#[case("execute")]
#[tokio::test]
async fn update_installation(#[case] phase: &str) {
    let server = mock_server().await;

    // Create a new update.
    let response = request(
        &server.url("/sovd/v1/apps/ota_manager/updates"),
        http::Method::POST,
        Some(json!({
        "id": "firmware-v1",
        "update_name": "Firmware v1",
        "size": 0,
        "targets": [],
        "origin": ["proximity", "remote"]
        })),
    )
    .await
    .unwrap();
    assert!(response.status().is_success());

    // Install the update
    let response = request(
        &server.url(&format!(
            "/sovd/v1/apps/ota_manager/updates/firmware-v1/{phase}"
        )),
        http::Method::PUT,
        None,
    )
    .await
    .unwrap();
    assert!(response.status().is_success());
    assert!(response.headers().contains_key("location"));
    assert_eq!(
        response.headers().get("location").unwrap(),
        &server.url("/sovd/v1/apps/ota_manager/updates/firmware-v1/status")
    );

    // Get the status of the update
    let response = request(
        &server.url("/sovd/v1/apps/ota_manager/updates/firmware-v1/status"),
        http::Method::GET,
        None,
    )
    .await
    .unwrap();
    assert!(response.status().is_success());
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["phase"].as_str().unwrap(), phase);
    assert_eq!(json["status"].as_str().unwrap(), "inProgress");

    // wait for the mock update to finish
    sleep(Duration::from_millis(1200)).await;

    // Get the status of the update again
    let response = request(
        &server.url("/sovd/v1/apps/ota_manager/updates/firmware-v1/status"),
        http::Method::GET,
        None,
    )
    .await
    .unwrap();
    assert!(response.status().is_success());
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["phase"].as_str().unwrap(), phase);
    assert_eq!(json["status"].as_str().unwrap(), "completed");
}

#[rstest::rstest]
#[case("prepare")]
#[case("execute")]
#[tokio::test]
async fn update_installation_failing(#[case] phase: &str) {
    let server = mock_server().await;

    // Create a new update.
    let response = request(
        &server.url("/sovd/v1/updates"),
        http::Method::POST,
        Some(json!({
        "id": "firmware-v1",
        "update_name": "Firmware v1",
        "size": 0,
        "targets": []
        })),
    )
    .await
    .unwrap();
    assert!(response.status().is_success());

    // Install the update
    let response = request(
        &server.url(&format!("/sovd/v1/updates/firmware-v1/{phase}")),
        http::Method::PUT,
        None,
    )
    .await
    .unwrap();
    assert!(response.status().is_success());
    assert!(response.headers().contains_key("location"));
    assert_eq!(
        response.headers().get("location").unwrap(),
        &server.url("/sovd/v1/updates/firmware-v1/status")
    );

    // Get the status of the update
    let response = request(
        &server.url("/sovd/v1/updates/firmware-v1/status"),
        http::Method::GET,
        None,
    )
    .await
    .unwrap();
    assert!(response.status().is_success());
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["phase"].as_str().unwrap(), phase);
    assert_eq!(json["status"].as_str().unwrap(), "inProgress");

    // wait for the mock update to finish
    sleep(Duration::from_millis(1200)).await;

    // Get the status of the update again
    let response = request(
        &server.url("/sovd/v1/updates/firmware-v1/status"),
        http::Method::GET,
        None,
    )
    .await
    .unwrap();
    assert!(response.status().is_success());
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["phase"].as_str().unwrap(), phase);
    assert_eq!(json["status"].as_str().unwrap(), "failed");
}

#[tokio::test]
async fn update_automated() {
    let server = mock_server().await;

    // Create a new update.
    let response = request(
        &server.url("/sovd/v1/apps/ota_manager/updates"),
        http::Method::POST,
        Some(json!({
        "id": "firmware-v1",
        "update_name": "Firmware v1",
        "size": 0,
        "automated": true,
        "targets": []
        })),
    )
    .await
    .unwrap();
    assert!(response.status().is_success());

    // install the update
    let response = request(
        &server.url("/sovd/v1/apps/ota_manager/updates/firmware-v1/automated"),
        http::Method::PUT,
        None,
    )
    .await
    .unwrap();

    assert!(response.status().is_success());
    assert!(response.headers().contains_key("location"));
    assert_eq!(
        response.headers().get("location").unwrap(),
        &server.url("/sovd/v1/apps/ota_manager/updates/firmware-v1/status")
    );

    for (phase, status) in [
        ("prepare", "inProgress"),
        ("prepare", "completed"),
        ("execute", "inProgress"),
        ("execute", "completed"),
    ] {
        // Get the status of the update
        let response = request(
            &server.url("/sovd/v1/apps/ota_manager/updates/firmware-v1/status"),
            http::Method::GET,
            None,
        )
        .await
        .unwrap();
        assert!(response.status().is_success());
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["phase"].as_str().unwrap(), phase);
        assert_eq!(json["status"].as_str().unwrap(), status);

        // wait for the mock update to finish
        sleep(Duration::from_millis(1200)).await;
    }
}

#[rstest::rstest]
#[case(json!({
        "id": "firmware-v1",
        "update_name": "Firmware v1",
        "size": 0,
        "automated": false,
        "targets": []
        }))]
#[case(json!({
        "id": "firmware-v1",
        "update_name": "Firmware v1",
        "size": 0,
        "targets": []
        }))]
#[tokio::test]
async fn update_automated_not_supported(#[case] update: serde_json::Value) {
    let server = mock_server().await;

    // Create a new update.
    let response = request(
        &server.url("/sovd/v1/apps/ota_manager/updates"),
        http::Method::POST,
        Some(update),
    )
    .await
    .unwrap();
    assert!(response.status().is_success());

    // install the update
    let response = request(
        &server.url("/sovd/v1/apps/ota_manager/updates/firmware-v1/automated"),
        http::Method::PUT,
        None,
    )
    .await
    .unwrap();

    assert!(response.status().is_client_error());
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(
        json["error_code"].as_str().unwrap(),
        "update-automated-not-supported"
    );
}

#[tokio::test]
async fn delete_update_during_installation() {
    let server = mock_server().await;

    // Create a new update.
    let response = request(
        &server.url("/sovd/v1/apps/ota_manager/updates"),
        http::Method::POST,
        Some(json!({
        "id": "firmware-v1",
        "update_name": "Firmware v1",
        "size": 0,
        "targets": []
        })),
    )
    .await
    .unwrap();
    assert!(response.status().is_success());

    // install the update
    let response = request(
        &server.url("/sovd/v1/apps/ota_manager/updates/firmware-v1/prepare"),
        http::Method::PUT,
        None,
    )
    .await
    .unwrap();
    assert!(response.status().is_success());

    sleep(Duration::from_millis(100)).await;

    let response = request(
        &server.url("/sovd/v1/apps/ota_manager/updates/firmware-v1"),
        http::Method::DELETE,
        None,
    )
    .await
    .unwrap();
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn parallel_update_execution_is_conflict() {
    let server = mock_server().await;

    // Create a new update.
    let response = request(
        &server.url("/sovd/v1/apps/ota_manager/updates"),
        http::Method::POST,
        Some(json!({
        "id": "firmware-v1",
        "update_name": "Firmware v1",
        "size": 0,
        "targets": []
        })),
    )
    .await
    .unwrap();
    assert!(response.status().is_success());

    // Create a second update.
    let response = request(
        &server.url("/sovd/v1/apps/ota_manager/updates"),
        http::Method::POST,
        Some(json!({
        "id": "firmware-v2",
        "update_name": "Firmware v2",
        "size": 0,
        "automated": true,
        "targets": []
        })),
    )
    .await
    .unwrap();
    assert!(response.status().is_success());

    // install the first update
    let response = request(
        &server.url("/sovd/v1/apps/ota_manager/updates/firmware-v1/execute"),
        http::Method::PUT,
        None,
    )
    .await
    .unwrap();
    assert!(response.status().is_success());

    sleep(Duration::from_millis(100)).await;

    // attempt to install the second update while the first is in progress
    let response = request(
        &server.url("/sovd/v1/apps/ota_manager/updates/firmware-v2/execute"),
        http::Method::PUT,
        None,
    )
    .await
    .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);

    // attempt to install the second update while the first is in progress
    let response = request(
        &server.url("/sovd/v1/apps/ota_manager/updates/firmware-v2/automated"),
        http::Method::PUT,
        None,
    )
    .await
    .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);

    // attempt to prepare the second update while the first is in progress
    let response = request(
        &server.url("/sovd/v1/apps/ota_manager/updates/firmware-v2/prepare"),
        http::Method::PUT,
        None,
    )
    .await
    .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
}
