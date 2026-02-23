// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use http_body_util::BodyExt;
use hyper::Request;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use opensovd_server::{AuthError, Authenticator, Authorizer, Parts, Server};
use tokio::net::TcpListener;
use tokio::sync::oneshot;

#[derive(Clone, Debug)]
struct Claims {
    role: String,
}

#[derive(Clone)]
struct ExtractBearer;

impl Authenticator for ExtractBearer {
    type Identity = Claims;

    async fn authenticate(&self, parts: &Parts) -> Result<Self::Identity, AuthError> {
        let header = parts
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .ok_or(AuthError::Unauthenticated)?;

        let token = header
            .strip_prefix("Bearer ")
            .ok_or(AuthError::Unauthenticated)?;

        let role = match token {
            "admin-token" => "admin",
            "user-token" => "user",
            _ => return Err(AuthError::Unauthenticated),
        };

        Ok(Claims {
            role: role.to_string(),
        })
    }
}

#[derive(Clone)]
struct RequireAdmin;

impl Authorizer<Claims> for RequireAdmin {
    async fn authorize(
        &self,
        identity: &Claims,
        _parts: &opensovd_server::Parts,
    ) -> Result<(), AuthError> {
        if identity.role == "admin" {
            Ok(())
        } else {
            Err(AuthError::unauthorized())
        }
    }
}

struct TestServer {
    addr: std::net::SocketAddr,
    shutdown: Option<oneshot::Sender<()>>,
}

impl TestServer {
    async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (shutdown_tx, shutdown_rx) = oneshot::channel();

        let server = Server::builder()
            .listener(listener)
            .base_uri("/sovd")
            .unwrap()
            .shutdown(async {
                shutdown_rx.await.ok();
            })
            .authenticator(ExtractBearer)
            .authorizer(RequireAdmin)
            .build()
            .unwrap();

        tokio::spawn(async move {
            server.serve().await.unwrap();
        });

        Self {
            addr,
            shutdown: Some(shutdown_tx),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("http://{}{}", self.addr, path)
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
    }
}

fn client()
-> Client<hyper_util::client::legacy::connect::HttpConnector, http_body_util::Empty<bytes::Bytes>> {
    Client::builder(TokioExecutor::new()).build_http()
}

/// Assert 403 response with `GenericError` body.
async fn assert_forbidden(response: hyper::Response<hyper::body::Incoming>, error_code: &str) {
    assert_eq!(response.status(), hyper::StatusCode::FORBIDDEN);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error_code"], error_code);
}

/// Assert 401 response with empty body.
async fn assert_unauthorized(response: hyper::Response<hyper::body::Incoming>) {
    assert_eq!(response.status(), hyper::StatusCode::UNAUTHORIZED);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert!(body.is_empty(), "401 response must have no body");
}

#[tokio::test]
async fn test_auth_missing_header_is_unauthorized() {
    let server = TestServer::start().await;
    let client = client();

    let request = Request::builder()
        .uri(server.url("/sovd/version-info"))
        .body(http_body_util::Empty::<bytes::Bytes>::new())
        .unwrap();

    let response = client.request(request).await.unwrap();
    assert_unauthorized(response).await;
}

#[tokio::test]
async fn test_auth_rejects_non_admin() {
    let server = TestServer::start().await;
    let client = client();

    let request = Request::builder()
        .uri(server.url("/sovd/version-info"))
        .header("authorization", "Bearer user-token")
        .body(http_body_util::Empty::<bytes::Bytes>::new())
        .unwrap();

    let response = client.request(request).await.unwrap();
    assert_forbidden(response, "insufficient-access-rights").await;
}

#[tokio::test]
async fn test_auth_allows_admin() {
    let server = TestServer::start().await;
    let client = client();

    let request = Request::builder()
        .uri(server.url("/sovd/version-info"))
        .header("authorization", "Bearer admin-token")
        .body(http_body_util::Empty::<bytes::Bytes>::new())
        .unwrap();

    let response = client.request(request).await.unwrap();
    assert!(response.status().is_success());
}
