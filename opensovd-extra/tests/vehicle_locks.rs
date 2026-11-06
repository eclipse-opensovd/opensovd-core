// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! The vehicle lock: one lock of the server that holds on all upstreams.

#![expect(clippy::unwrap_used)]

use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::{Arc, Mutex};

use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::{Method, Request, Response};
use opensovd_extra::{Federation, ForwardRoute, Upstream};
use opensovd_models::discovery::EntityCapabilities;
use opensovd_models::locking::Lock;
use opensovd_models::{Collection, DateTime, ErrorCode, GenericError};
use opensovd_server::{Server, Topology};
use rstest::rstest;
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::sync::oneshot;

mod common;

/// An upstream with locks, each with its own id and owned by the token that
/// acquired it. It locks one client at a time, and renews for the owner.
#[derive(Clone)]
struct Backend {
    name: &'static str,
    expires: &'static str,
    locks: Arc<Mutex<HashMap<String, String>>>,
    /// Locks another client broke, by their id, which their owner has to delete.
    defunct: Arc<Mutex<HashMap<String, String>>>,
    refuse: Arc<Mutex<Option<u16>>>,
    seen: Arc<Mutex<Vec<String>>>,
}

impl Backend {
    fn new(name: &'static str, expires: &'static str) -> Self {
        Self {
            name,
            expires,
            locks: Arc::default(),
            defunct: Arc::default(),
            refuse: Arc::default(),
            seen: Arc::default(),
        }
    }

    fn refuse(&self, status: u16) {
        *self.refuse.lock().unwrap() = Some(status);
    }

    fn accept(&self) {
        *self.refuse.lock().unwrap() = None;
    }

    fn lose_locks(&self) {
        self.locks.lock().unwrap().clear();
    }

    /// Breaks the locks as a CDA does: they stay as defunct locks.
    fn break_locks(&self) {
        let broken = std::mem::take(&mut *self.locks.lock().unwrap());
        self.defunct.lock().unwrap().extend(broken);
    }

    /// A lock of `owner` acquired from this upstream directly.
    fn lock_directly(&self, owner: &str) -> String {
        let id = format!("{}-direct", self.name);
        self.locks
            .lock()
            .unwrap()
            .insert(id.clone(), owner.to_owned());
        id
    }

    fn defunct(&self) -> usize {
        self.defunct.lock().unwrap().len()
    }

    fn held(&self) -> usize {
        self.locks.lock().unwrap().len()
    }

    fn seen(&self) -> Vec<String> {
        self.seen.lock().unwrap().clone()
    }

    fn answer(&self, method: &Method, path: &str, token: &str) -> (u16, Value) {
        if let Some(status) = *self.refuse.lock().unwrap() {
            return (status, json!({}));
        }
        let mut locks = self.locks.lock().unwrap();
        let id = path.strip_prefix("/locks/");
        let mut defunct = self.defunct.lock().unwrap();
        if let Some(owner) = id.and_then(|id| defunct.get(id)).cloned() {
            return match method.as_str() {
                "GET" => (
                    200,
                    json!({
                        "lock_expiration": self.expires,
                        "owned": owner == token,
                        "x-sovd2uds-broken-by": "priority-app",
                    }),
                ),
                "PUT" => (
                    409,
                    json!({
                        "error_code": "lock-broken",
                        "message": "Client lock was broken",
                        "parameters": { "broken_by": "priority-app" },
                    }),
                ),
                _ => {
                    defunct.remove(id.unwrap_or_default());
                    (200, json!({}))
                }
            };
        }
        match (method.as_str(), id) {
            ("GET", None) if token.is_empty() => (401, json!({})),
            ("GET", None) => {
                let active = locks.iter().map(|(id, owner)| {
                    json!({
                        "id": id,
                        "lock_expiration": self.expires,
                        "owned": owner == token,
                        "from": self.name,
                    })
                });
                let broken = defunct.iter().map(|(id, owner)| {
                    json!({
                        "id": id,
                        "lock_expiration": self.expires,
                        "owned": owner == token,
                        "x-sovd2uds-broken-by": "priority-app",
                    })
                });
                (
                    200,
                    json!({ "items": active.chain(broken).collect::<Vec<_>>() }),
                )
            }
            ("POST", None) => {
                if let Some((id, owner)) = locks.iter().next() {
                    return if owner == token {
                        (200, json!({ "id": id, "owned": true }))
                    } else {
                        (409, json!({}))
                    };
                }
                let id = format!("{}-lock", self.name);
                locks.insert(id.clone(), token.to_owned());
                (201, json!({ "id": id, "owned": true }))
            }
            ("GET", Some(id)) => match locks.get(id) {
                Some(owner) => (
                    200,
                    json!({
                        "lock_expiration": self.expires,
                        "owned": owner == token,
                        "from": self.name,
                    }),
                ),
                None => (404, json!({})),
            },
            ("PUT" | "DELETE", Some(id)) => match locks.get(id).cloned() {
                Some(owner) if owner == token => {
                    if method == Method::DELETE {
                        locks.remove(id);
                    }
                    (200, json!({}))
                }
                Some(_) => (403, json!({})),
                None => (404, json!({})),
            },
            _ => (404, json!({})),
        }
    }

    fn upstream(&self) -> Upstream {
        let backend = self.clone();
        Upstream::new(tower::service_fn(
            move |request: Request<opensovd_server::Body>| {
                let backend = backend.clone();
                async move {
                    let method = request.method().clone();
                    let path = request.uri().path().to_owned();
                    let token = request
                        .headers()
                        .get("authorization")
                        .map(|token| token.to_str().unwrap().to_owned())
                        .unwrap_or_default();
                    let body = request.into_body().collect().await.unwrap().to_bytes();
                    let seen = format!("{method} {path} {}", String::from_utf8_lossy(&body));
                    backend.seen.lock().unwrap().push(seen.trim().to_owned());
                    let (status, answer) = backend.answer(&method, &path, &token);
                    Ok::<_, Infallible>(
                        Response::builder()
                            .status(status)
                            .header("content-type", "application/json")
                            .body(Full::new(Bytes::from(answer.to_string())))
                            .unwrap(),
                    )
                }
            },
        ))
        .routes([ForwardRoute::Locks])
    }
}

struct TestServer {
    addr: std::net::SocketAddr,
    shutdown: Option<oneshot::Sender<()>>,
}

impl Drop for TestServer {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
    }
}

async fn start(backends: &[&Backend]) -> TestServer {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let upstreams = backends.iter().map(|backend| backend.upstream()).collect();
    let server = Server::builder()
        .listener(listener)
        .base_uri("/sovd")
        .unwrap()
        .layer(
            Federation::new(upstreams, Topology::default())
                .unwrap()
                .base("/sovd")
                .into_layer(),
        )
        .shutdown(async {
            shutdown_rx.await.ok();
        })
        .build()
        .unwrap();
    tokio::spawn(async move { server.serve().await.unwrap() });
    TestServer {
        addr,
        shutdown: Some(shutdown_tx),
    }
}

async fn send(
    server: &TestServer,
    token: &str,
    method: Method,
    path: &str,
    body: &'static [u8],
) -> (u16, Value) {
    let request = Request::builder()
        .method(method)
        .header("authorization", token)
        .uri(format!("http://{}/sovd/v1{path}", server.addr))
        .body(Full::new(Bytes::from_static(body)))
        .unwrap();
    let response = common::client().request(request).await.unwrap();
    let status = response.status().as_u16();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
}

const LOCK: &[u8] = br#"{"lock_expiration":60}"#;
const EARLY: &str = "2030-01-01T00:00:05Z";
const LATE: &str = "2030-01-01T00:00:10Z";

fn pair() -> (Backend, Backend) {
    (Backend::new("a", LATE), Backend::new("b", EARLY))
}

fn lock(json: Value) -> Lock {
    serde_json::from_value(json).unwrap()
}

fn locks(json: Value) -> Vec<Lock> {
    let list: Collection<Lock> = serde_json::from_value(json).unwrap();
    list.items
}

fn time(text: &str) -> DateTime {
    text.parse().unwrap()
}

async fn acquire(server: &TestServer) -> String {
    let (status, json) = send(server, "x", Method::POST, "/locks", LOCK).await;
    assert_eq!(status, 201);
    lock(json).id.unwrap()
}

#[rstest]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn a_lock_holds_on_all_upstreams_under_one_id() {
    let (a, b) = pair();
    let server = start(&[&a, &b]).await;

    let id = acquire(&server).await;

    assert!(id != "a-lock" && id != "b-lock");
    assert_eq!(a.seen(), [r#"POST /locks {"lock_expiration":60}"#]);
    assert_eq!(b.seen(), [r#"POST /locks {"lock_expiration":60}"#]);
    let (status, json) = send(&server, "x", Method::GET, "/locks", b"").await;
    assert_eq!(status, 200);
    let items = locks(json);
    assert_eq!(items.len(), 1);
    assert_eq!(items.first().unwrap().id.as_ref(), Some(&id));
}

#[rstest]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn a_single_upstream_also_gets_a_lock_of_the_server() {
    let a = Backend::new("a", LATE);
    let server = start(&[&a]).await;

    let id = acquire(&server).await;

    assert_ne!(id, "a-lock");
    let (status, json) = send(&server, "x", Method::GET, &format!("/locks/{id}"), b"").await;
    assert_eq!(status, 200);
    assert_eq!(lock(json).lock_expiration, Some(time(LATE)));
}

#[rstest]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn a_lock_expires_with_its_first_part_and_keeps_what_the_upstreams_say() {
    let (a, b) = pair();
    let server = start(&[&a, &b]).await;
    let id = acquire(&server).await;

    let (status, json) = send(&server, "x", Method::GET, &format!("/locks/{id}"), b"").await;

    assert_eq!(status, 200);
    let lock = lock(json);
    assert_eq!(lock.lock_expiration, Some(time(EARLY)));
    assert_eq!(lock.owned, Some(true));
    assert_eq!(lock.extensions.get("from"), Some(&Value::from("a")));
}

#[rstest]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn the_upstreams_tell_who_owns_a_lock() {
    let (a, b) = pair();
    let server = start(&[&a, &b]).await;
    let id = acquire(&server).await;

    let (_, json) = send(&server, "y", Method::GET, "/locks", b"").await;
    assert_eq!(locks(json).first().unwrap().owned, Some(false));
    let (_, json) = send(&server, "y", Method::GET, &format!("/locks/{id}"), b"").await;
    assert_eq!(lock(json).owned, Some(false));

    let path = format!("/locks/{id}");
    let (status, _) = send(&server, "y", Method::DELETE, &path, b"").await;
    assert_eq!(status, 403);
    let (_, json) = send(&server, "x", Method::GET, "/locks", b"").await;
    assert_eq!(locks(json).len(), 1);
}

#[rstest]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn a_refused_lock_releases_what_was_acquired() {
    let (a, b) = pair();
    b.refuse(409);
    let server = start(&[&a, &b]).await;

    let (status, _) = send(&server, "x", Method::POST, "/locks", LOCK).await;

    assert_eq!(status, 409);
    assert_eq!(a.held(), 0);
    assert_eq!(a.seen().last().unwrap(), "DELETE /locks/a-lock");
    b.accept();
    let (_, json) = send(&server, "x", Method::GET, "/locks", b"").await;
    assert_eq!(locks(json).len(), 0);
}

#[rstest]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn an_upstream_without_locks_fails_the_lock() {
    let (a, b) = pair();
    b.refuse(404);
    let server = start(&[&a, &b]).await;

    let (status, _) = send(&server, "x", Method::POST, "/locks", LOCK).await;

    assert_eq!(status, 404);
    assert_eq!(a.held(), 0);
}

#[rstest]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn upstreams_are_locked_in_order_and_stop_at_the_first_refusal() {
    let (a, b) = pair();
    a.refuse(409);
    let server = start(&[&a, &b]).await;

    let (status, _) = send(&server, "x", Method::POST, "/locks", LOCK).await;

    assert_eq!(status, 409);
    assert_eq!(b.seen(), Vec::<String>::new());
}

#[rstest]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn acquiring_again_renews_the_lock() {
    let (a, b) = pair();
    let server = start(&[&a, &b]).await;
    let id = acquire(&server).await;

    let (status, json) = send(&server, "x", Method::POST, "/locks", LOCK).await;

    assert_eq!(status, 200);
    assert_eq!(lock(json).id, Some(id));
    let (_, json) = send(&server, "x", Method::GET, "/locks", b"").await;
    assert_eq!(locks(json).len(), 1);
}

#[rstest]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn a_renewal_that_fails_keeps_the_lock_of_the_client() {
    let (a, b) = pair();
    let server = start(&[&a, &b]).await;
    acquire(&server).await;
    b.refuse(500);

    let (status, _) = send(&server, "x", Method::POST, "/locks", LOCK).await;

    assert_eq!(status, 500);
    assert_eq!(a.held(), 1);
}

#[rstest]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn a_lost_part_breaks_the_lock() {
    let (a, b) = pair();
    let server = start(&[&a, &b]).await;
    let id = acquire(&server).await;
    b.lose_locks();

    let path = format!("/locks/{id}");
    let (status, json) = send(&server, "x", Method::GET, &path, b"").await;

    assert_eq!(status, 409);
    let error: GenericError = serde_json::from_value(json).unwrap();
    assert!(matches!(error.error_code, ErrorCode::LockBroken));
    assert_eq!(a.held(), 0);
    let (status, _) = send(&server, "x", Method::GET, &path, b"").await;
    assert_eq!(status, 404);
}

#[rstest]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn a_lock_whose_first_part_is_gone_is_not_listed() {
    let (a, b) = pair();
    let server = start(&[&a, &b]).await;
    acquire(&server).await;
    a.lose_locks();

    let (status, json) = send(&server, "x", Method::GET, "/locks", b"").await;

    assert_eq!(status, 200);
    assert_eq!(locks(json).len(), 0);
}

#[rstest]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn modifying_and_releasing_reach_all_parts() {
    let (a, b) = pair();
    let server = start(&[&a, &b]).await;
    let id = acquire(&server).await;
    let path = format!("/locks/{id}");

    let (status, _) = send(&server, "x", Method::PUT, &path, LOCK).await;
    assert_eq!(status, 200);
    assert_eq!(
        a.seen().last().unwrap(),
        r#"PUT /locks/a-lock {"lock_expiration":60}"#
    );
    assert_eq!(
        b.seen().last().unwrap(),
        r#"PUT /locks/b-lock {"lock_expiration":60}"#
    );

    let (status, _) = send(&server, "x", Method::DELETE, &path, b"").await;
    assert_eq!(status, 200);
    assert_eq!((a.held(), b.held()), (0, 0));
    let (status, _) = send(&server, "x", Method::GET, &path, b"").await;
    assert_eq!(status, 404);
}

#[rstest]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn releasing_a_lock_with_a_lost_part_releases_the_rest() {
    let (a, b) = pair();
    let server = start(&[&a, &b]).await;
    let id = acquire(&server).await;
    b.lose_locks();

    let (status, _) = send(&server, "x", Method::DELETE, &format!("/locks/{id}"), b"").await;

    assert_eq!(status, 200);
    assert_eq!(a.held(), 0);
}

#[rstest]
#[case(Method::GET)]
#[case(Method::PUT)]
#[case(Method::DELETE)]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn an_unknown_lock_is_not_found(#[case] method: Method) {
    let (a, b) = pair();
    let server = start(&[&a, &b]).await;

    let (status, _) = send(&server, "x", method, "/locks/unknown", LOCK).await;

    assert_eq!(status, 404);
    assert_eq!(a.seen(), Vec::<String>::new());
}

#[rstest]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn an_expired_lock_is_forgotten() {
    let (a, b) = pair();
    let server = start(&[&a, &b]).await;

    let body = br#"{"lock_expiration":0}"#;
    let (status, json) = send(&server, "x", Method::POST, "/locks", body).await;
    assert_eq!(status, 201);
    let id = lock(json).id.unwrap();
    // the upstreams' locks expire by themselves
    a.lose_locks();
    b.lose_locks();

    let (_, json) = send(&server, "x", Method::GET, "/locks", b"").await;
    assert_eq!(locks(json).len(), 0);
    // over, not broken: the server knows it expired
    let (status, _) = send(&server, "x", Method::GET, &format!("/locks/{id}"), b"").await;
    assert_eq!(status, 404);
}

#[rstest]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn the_root_advertises_locks_even_if_no_upstream_does() {
    let (a, b) = pair();
    let server = start(&[&a, &b]).await;

    let (status, json) = send(&server, "x", Method::GET, "", b"").await;

    assert_eq!(status, 200);
    let root: EntityCapabilities = serde_json::from_value(json).unwrap();
    assert!(root.locks.unwrap().0.ends_with("/sovd/v1/locks"));
}

#[rstest]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn an_upstream_decides_who_may_list_locks_even_without_locks() {
    let (a, b) = pair();
    let server = start(&[&a, &b]).await;

    let (status, _) = send(&server, "", Method::GET, "/locks", b"").await;
    assert_eq!(status, 401);

    let (status, json) = send(&server, "x", Method::GET, "/locks", b"").await;
    assert_eq!(status, 200);
    assert_eq!(locks(json).len(), 0);
}

/// The list shows what any part says, not only the first.
#[rstest]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn the_list_shows_a_part_only_a_later_upstream_broke() {
    let (a, b) = pair();
    let server = start(&[&a, &b]).await;
    let id = acquire(&server).await;
    b.break_locks();

    let (status, json) = send(&server, "x", Method::GET, "/locks", b"").await;

    assert_eq!(status, 200);
    let items = locks(json);
    let lock = items.first().unwrap();
    assert_eq!(lock.id, Some(id));
    assert_eq!(
        lock.extensions.get("x-sovd2uds-broken-by"),
        Some(&Value::from("priority-app"))
    );
}

#[rstest]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn a_lock_whose_later_part_is_gone_is_not_listed() {
    let (a, b) = pair();
    let server = start(&[&a, &b]).await;
    acquire(&server).await;
    b.lose_locks();

    let (status, json) = send(&server, "x", Method::GET, "/locks", b"").await;

    assert_eq!(status, 200);
    assert_eq!(locks(json).len(), 0);
}

/// A broken part stays with the lock until the client that lost it deletes it,
/// and the client learns who broke it from the upstream.
#[rstest]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn a_broken_part_answers_for_itself_until_it_is_deleted() {
    let (a, b) = pair();
    let server = start(&[&a, &b]).await;
    let id = acquire(&server).await;
    b.break_locks();
    let path = format!("/locks/{id}");

    let (status, json) = send(&server, "x", Method::GET, &path, b"").await;
    assert_eq!(status, 200);
    assert_eq!(
        lock(json).extensions.get("x-sovd2uds-broken-by"),
        Some(&Value::from("priority-app"))
    );

    let (status, json) = send(&server, "x", Method::PUT, &path, LOCK).await;
    assert_eq!(status, 409);
    let error: GenericError = serde_json::from_value(json).unwrap();
    assert!(matches!(error.error_code, ErrorCode::LockBroken));
    assert_eq!(
        error.message,
        format!("An upstream reports the lock {id} as broken")
    );
    let parameters = error.parameters.unwrap();
    assert_eq!(
        parameters.get("broken_by"),
        Some(&Value::from("priority-app"))
    );
    assert_eq!(b.defunct(), 1);

    let (status, _) = send(&server, "x", Method::DELETE, &path, b"").await;
    assert_eq!(status, 200);
    assert_eq!(a.held(), 0);
    assert_eq!(b.defunct(), 0);
    let (_, json) = send(&server, "x", Method::GET, "/locks", b"").await;
    assert_eq!(locks(json).len(), 0);
}

/// The server only needs the expiration, and the upstreams judge the rest.
#[rstest]
#[case(br#"{"Lock_Expiration":60}"#)]
#[case(br#"{"lock_expiration":60,"break_lock":"TRUE"}"#)]
#[case(br#"{"lock_expiration":60,"priority":{"level":1}}"#)]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn a_lock_lasts_whatever_else_its_request_holds(#[case] body: &'static [u8]) {
    let (a, b) = pair();
    let server = start(&[&a, &b]).await;

    let (status, _) = send(&server, "x", Method::POST, "/locks", body).await;
    assert_eq!(status, 201);

    let (_, json) = send(&server, "x", Method::GET, "/locks", b"").await;
    assert_eq!(locks(json).len(), 1);
}

/// A lock the server does not know, such as one acquired from an upstream
/// directly, is listed as the upstream lists it.
#[rstest]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn the_list_shows_a_lock_acquired_from_an_upstream_directly() {
    let (a, b) = pair();
    let server = start(&[&a, &b]).await;
    let direct = b.lock_directly("z");

    let (status, json) = send(&server, "x", Method::GET, "/locks", b"").await;

    assert_eq!(status, 200);
    let items = locks(json);
    assert_eq!(items.len(), 1);
    let lock = items.first().unwrap();
    assert_eq!(lock.id, Some(direct));
    assert_eq!(lock.owned, Some(false));

    let (status, _) = send(&server, "x", Method::POST, "/locks", LOCK).await;
    assert_eq!(status, 409);
}

/// Every answer about a lock is the server's: no upstream's lock shows.
#[rstest]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn the_server_names_its_own_lock_only() {
    let (a, b) = pair();
    let server = start(&[&a, &b]).await;
    let request = Request::builder()
        .method(Method::POST)
        .header("authorization", "x")
        .uri(format!("http://{}/sovd/v1/locks", server.addr))
        .body(Full::new(Bytes::from_static(LOCK)))
        .unwrap();
    let response = common::client().request(request).await.unwrap();
    assert_eq!(response.status(), 201);
    let location = response
        .headers()
        .get("location")
        .unwrap()
        .to_str()
        .unwrap();
    let location = location.to_owned();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let id = lock(serde_json::from_slice(&body).unwrap()).id.unwrap();
    assert_eq!(
        location,
        format!("http://{}/sovd/v1/locks/{id}", server.addr)
    );

    let path = format!("/locks/{id}");
    let (_, details) = send(&server, "x", Method::GET, &path, b"").await;
    let (status, refused) = send(&server, "y", Method::PUT, &path, LOCK).await;
    assert_eq!(status, 403);
    a.lose_locks();
    b.lose_locks();
    let (status, gone) = send(&server, "x", Method::DELETE, &path, b"").await;
    assert_eq!(status, 404);
    for answer in [details, refused, gone] {
        let text = answer.to_string();
        assert!(
            !text.contains("a-lock") && !text.contains("b-lock"),
            "{text}"
        );
    }
}

/// A lock of an upstream whose lock of the server expired is the upstream's
/// alone, and is listed as such.
#[rstest]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn an_upstream_lock_outliving_the_lock_of_the_server_is_listed() {
    let (a, b) = pair();
    let server = start(&[&a, &b]).await;
    let body = br#"{"lock_expiration":0}"#;
    let (status, _) = send(&server, "x", Method::POST, "/locks", body).await;
    assert_eq!(status, 201);

    let (_, json) = send(&server, "x", Method::GET, "/locks", b"").await;

    let mut ids: Vec<_> = locks(json).into_iter().filter_map(|lock| lock.id).collect();
    ids.sort();
    assert_eq!(ids, ["a-lock", "b-lock"]);
}
