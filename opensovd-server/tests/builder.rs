// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

#![allow(clippy::indexing_slicing)]

mod common;

use http_body_util::BodyExt;
use hyper::Request;
use opensovd_core::{
    Component, DiscoveryError, DiscoveryProvider, DiscoveryStream, EntityCollection, Topology,
};
use opensovd_server::Server;
use tokio::time::Duration;

#[tokio::test]
async fn test_base_path_slash() {
    let server = common::TestServer::builder().base_uri("/").build().await;
    let client = common::client();

    let request = Request::builder()
        .uri(server.url("/version-info"))
        .body(http_body_util::Empty::<bytes::Bytes>::new())
        .unwrap();

    let response = client.request(request).await.unwrap();
    assert!(response.status().is_success());
}

#[tokio::test]
async fn test_base_path_with_slash() {
    let server = common::TestServer::builder()
        .base_uri("/sovd")
        .build()
        .await;
    let client = common::client();

    let request = Request::builder()
        .uri(server.url("/sovd/version-info"))
        .body(http_body_util::Empty::<bytes::Bytes>::new())
        .unwrap();

    let response = client.request(request).await.unwrap();
    assert!(response.status().is_success());
}

#[tokio::test]
async fn test_topology_component_lookup() {
    let topology = Topology::new();
    topology
        .write()
        .await
        .add_component(common::TestEntity::new("ECU", "Engine Control Unit"));

    let server = common::TestServer::builder()
        .topology(topology)
        .build()
        .await;
    let client = common::client();

    // GET /components/{id} uses topology to look up component details
    let request = Request::builder()
        .uri(server.url("/sovd/v1/components/ECU"))
        .body(http_body_util::Empty::<bytes::Bytes>::new())
        .unwrap();

    let response = client.request(request).await.unwrap();
    assert!(response.status().is_success());

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["id"], "ECU");
    assert_eq!(json["name"], "Engine Control Unit");
}

#[tokio::test]
async fn test_list_components() {
    let topology = opensovd_mocks::create_mock_topology().await;
    let server = common::TestServer::builder()
        .topology(topology)
        .build()
        .await;
    let client = common::client();

    let request = Request::builder()
        .uri(server.url("/sovd/v1/components"))
        .body(http_body_util::Empty::<bytes::Bytes>::new())
        .unwrap();

    let response = client.request(request).await.unwrap();
    assert!(response.status().is_success());

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    let items = json["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
}

#[tokio::test]
async fn test_list_apps() {
    let topology = opensovd_mocks::create_mock_topology().await;
    let server = common::TestServer::builder()
        .topology(topology)
        .build()
        .await;
    let client = common::client();

    let request = Request::builder()
        .uri(server.url("/sovd/v1/apps"))
        .body(http_body_util::Empty::<bytes::Bytes>::new())
        .unwrap();

    let response = client.request(request).await.unwrap();
    assert!(response.status().is_success());

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    let items = json["items"].as_array().unwrap();
    assert_eq!(items.len(), 3);
}

#[tokio::test]
async fn test_list_areas() {
    let topology = opensovd_mocks::create_mock_topology().await;
    let server = common::TestServer::builder()
        .topology(topology)
        .build()
        .await;
    let client = common::client();

    let request = Request::builder()
        .uri(server.url("/sovd/v1/areas"))
        .body(http_body_util::Empty::<bytes::Bytes>::new())
        .unwrap();

    let response = client.request(request).await.unwrap();
    assert!(response.status().is_success());

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    let items = json["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
}

struct MockDiscoveryProvider {
    component: Component,
}

#[async_trait::async_trait]
impl DiscoveryProvider for MockDiscoveryProvider {
    async fn discover(&self) -> Result<DiscoveryStream, DiscoveryError> {
        let entities = EntityCollection {
            components: vec![Component::new(self.component.id(), self.component.name())],
            ..Default::default()
        };
        Ok(Box::pin(futures::stream::once(async {
            Ok((vec![], entities))
        })))
    }
}

#[tokio::test]
async fn test_discovery_adds_component() {
    let provider = MockDiscoveryProvider {
        component: Component::new("discovered-ecu", "Discovered ECU"),
    };

    let server = common::TestServer::builder()
        .discovery(provider)
        .build()
        .await;
    let client = common::client();

    // Give the discovery task time to process the event.
    tokio::time::sleep(Duration::from_millis(100)).await;

    let request = Request::builder()
        .uri(server.url("/sovd/v1/components/discovered-ecu"))
        .body(http_body_util::Empty::<bytes::Bytes>::new())
        .unwrap();

    let response = client.request(request).await.unwrap();
    assert!(
        response.status().is_success(),
        "Expected discovered component to be present, got {}",
        response.status()
    );

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["id"], "discovered-ecu");
    assert_eq!(json["name"], "Discovered ECU");
}

#[tokio::test]
async fn test_service() {
    let svc = tower::service_fn(|_req: http::Request<opensovd_server::Body>| async {
        Ok::<_, std::convert::Infallible>(http::Response::new(http_body_util::Full::new(
            bytes::Bytes::from("from service"),
        )))
    });

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    let server = Server::builder()
        .listener(listener)
        .base_uri("/sovd")
        .unwrap()
        .service("/custom", svc)
        .shutdown(async {
            shutdown_rx.await.ok();
        })
        .build()
        .unwrap();

    tokio::spawn(async move { server.serve().await.unwrap() });

    let client = common::client();
    let request = Request::builder()
        .uri(format!("http://{addr}/custom"))
        .body(http_body_util::Empty::<bytes::Bytes>::new())
        .unwrap();

    let response = client.request(request).await.unwrap();
    assert!(response.status().is_success());

    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&body[..], b"from service");

    let _ = shutdown_tx.send(());
}
