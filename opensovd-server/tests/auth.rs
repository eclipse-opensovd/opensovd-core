// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

#![expect(clippy::unwrap_used)]

use std::sync::{Arc, Mutex};

use http_body_util::BodyExt;
use hyper::Request;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use opensovd_server::{
    Access, Auth, AuthError, Authenticator, Authorizer, Parts, Principal, Server,
};
use tokio::net::TcpListener;
use tokio::sync::oneshot;

#[derive(Clone)]
struct ExtractBearer;

impl Authenticator for ExtractBearer {
    fn authenticate(&self, parts: &Parts) -> Result<Option<Principal>, AuthError> {
        let Some(header) = parts.headers.get("authorization") else {
            return Ok(None);
        };
        let role = match header.to_str() {
            Ok("Bearer admin-token") => "admin",
            Ok("Bearer user-token") => "user",
            _ => return Err(AuthError::InvalidToken),
        };
        Ok(Some(Principal {
            subject: format!("{role}@example.com"),
            roles: vec![role.to_owned()],
            scopes: Vec::new(),
            claims: serde_json::Map::new(),
        }))
    }
}

/// Admins may do anything; everyone may read the version.
#[derive(Clone, Default)]
struct Policy {
    seen: Arc<Mutex<Vec<serde_json::Value>>>,
}

impl Authorizer for Policy {
    fn authorize(&self, access: &Access<'_>) -> bool {
        self.seen
            .lock()
            .unwrap()
            .push(serde_json::to_value(access).unwrap());
        access.route == Some("/version-info")
            || access
                .principal
                .is_some_and(|p| p.roles.iter().any(|r| r == "admin"))
    }
}

struct TestServer {
    addr: std::net::SocketAddr,
    policy: Policy,
    shutdown: Option<oneshot::Sender<()>>,
}

impl TestServer {
    async fn start() -> Self {
        Self::mounted_at("/sovd").await
    }

    async fn mounted_at(base: &str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let policy = Policy::default();

        let server = Server::builder()
            .listener(listener)
            .base_uri(base)
            .unwrap()
            .shutdown(async {
                shutdown_rx.await.ok();
            })
            .auth(Auth::new(ExtractBearer, policy.clone()))
            .build()
            .unwrap();

        tokio::spawn(async move {
            server.serve().await.unwrap();
        });

        Self {
            addr,
            policy,
            shutdown: Some(shutdown_tx),
        }
    }

    async fn get(&self, path: &str, token: Option<&str>) -> hyper::Response<hyper::body::Incoming> {
        let mut request = Request::builder().uri(format!("http://{}{}", self.addr, path));
        if let Some(token) = token {
            request = request.header("authorization", format!("Bearer {token}"));
        }
        let request = request
            .body(http_body_util::Empty::<bytes::Bytes>::new())
            .unwrap();
        Client::builder(TokioExecutor::new())
            .build_http()
            .request(request)
            .await
            .unwrap()
    }

    fn last_access(&self) -> serde_json::Value {
        self.policy.seen.lock().unwrap().last().cloned().unwrap()
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
    }
}

#[tokio::test]
async fn test_auth_missing_header_is_unauthorized() {
    let server = TestServer::start().await;
    let response = server.get("/sovd/v1/components", None).await;
    assert_eq!(response.status(), hyper::StatusCode::UNAUTHORIZED);
    assert_eq!(response.headers()["www-authenticate"], "Bearer");
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert!(body.is_empty(), "401 response must have no body");
}

#[tokio::test]
async fn test_auth_invalid_token_is_unauthorized() {
    let server = TestServer::start().await;
    let response = server.get("/sovd/v1/components", Some("nope")).await;
    assert_eq!(response.status(), hyper::StatusCode::UNAUTHORIZED);
    assert_eq!(
        response.headers()["www-authenticate"],
        r#"Bearer error="invalid_token""#
    );
}

#[tokio::test]
async fn test_auth_rejects_non_admin() {
    let server = TestServer::start().await;
    let response = server.get("/sovd/v1/components", Some("user-token")).await;
    assert_eq!(response.status(), hyper::StatusCode::FORBIDDEN);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error_code"], "insufficient-access-rights");
}

#[tokio::test]
async fn test_auth_allows_admin() {
    let server = TestServer::start().await;
    let response = server.get("/sovd/v1/components", Some("admin-token")).await;
    assert!(response.status().is_success());
}

#[tokio::test]
async fn test_auth_policy_may_allow_anonymous_callers() {
    let server = TestServer::start().await;
    let response = server.get("/sovd/version-info", None).await;
    assert!(response.status().is_success());
    assert!(server.last_access().get("principal").is_none());
}

#[tokio::test]
async fn test_auth_routes_are_relative_to_a_mount_with_a_trailing_slash() {
    let server = TestServer::mounted_at("/sovd/").await;
    let response = server.get("/sovd/version-info", None).await;
    assert!(response.status().is_success());
    assert_eq!(server.last_access()["route"], "/version-info");
}

#[tokio::test]
async fn test_auth_policy_sees_route_template_and_params() {
    let server = TestServer::start().await;
    server
        .get("/sovd/v1/components/ec%75/data/speed", Some("admin-token"))
        .await;
    let access = server.last_access();
    assert_eq!(access["method"], "GET");
    assert_eq!(
        access["route"],
        "/v1/components/{component_id}/data/{data_id}"
    );
    assert_eq!(
        access["params"],
        serde_json::json!({"component_id": "ecu", "data_id": "speed"})
    );
    assert_eq!(access["principal"]["subject"], "admin@example.com");
}

#[tokio::test]
async fn test_auth_undecodable_params_are_rejected_before_the_policy() {
    let server = TestServer::start().await;
    let response = server
        .get("/sovd/v1/components/%FF/data/speed", Some("admin-token"))
        .await;
    assert_eq!(response.status(), hyper::StatusCode::BAD_REQUEST);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error_code"], "incomplete-request");
    assert_eq!(json["message"], "Invalid UTF-8 in `component_id`");
    assert!(server.policy.seen.lock().unwrap().is_empty());
}

#[tokio::test]
async fn test_auth_unknown_paths_have_no_route() {
    let server = TestServer::start().await;
    let response = server.get("/sovd/v1/nope", Some("admin-token")).await;
    assert_eq!(response.status(), hyper::StatusCode::NOT_FOUND);
    assert!(server.last_access().get("route").is_none());
}
