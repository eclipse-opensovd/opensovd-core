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
/// acquired it. It locks one client at a time, renews for the owner, and
/// breaks the lock of another client for `break_lock` as a CDA does, unless it
/// refuses to.
#[derive(Clone)]
struct Backend {
    name: &'static str,
    expires: &'static str,
    locks: Arc<Mutex<HashMap<String, String>>>,
    /// Locks another client broke, by their id, which their owner has to delete.
    defunct: Arc<Mutex<HashMap<String, String>>>,
    refuse: Arc<Mutex<Option<u16>>>,
    refuse_breaks: Arc<Mutex<bool>>,
    /// Grants locks with an answer that does not name them.
    unnamed: Arc<Mutex<bool>>,
    acquired: Arc<Mutex<usize>>,
    seen: Arc<Mutex<Vec<String>>>,
    /// The `Content-Length` of each request seen, if it had one.
    lengths: Arc<Mutex<Vec<Option<String>>>>,
}

impl Backend {
    fn new(name: &'static str, expires: &'static str) -> Self {
        Self {
            name,
            expires,
            locks: Arc::default(),
            defunct: Arc::default(),
            refuse: Arc::default(),
            refuse_breaks: Arc::default(),
            unnamed: Arc::default(),
            acquired: Arc::default(),
            seen: Arc::default(),
            lengths: Arc::default(),
        }
    }

    /// Refuses `break_lock` as a policy of this upstream does.
    fn refuse_breaks(&self) {
        *self.refuse_breaks.lock().unwrap() = true;
    }

    /// The id of a new lock: `{name}-lock`, and a number after the first.
    fn next_id(&self) -> String {
        let mut acquired = self.acquired.lock().unwrap();
        *acquired = acquired.saturating_add(1);
        match *acquired {
            1 => format!("{}-lock", self.name),
            n => format!("{}-lock-{n}", self.name),
        }
    }

    fn grant_unnamed(&self) {
        *self.unnamed.lock().unwrap() = true;
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

    fn lengths(&self) -> Vec<Option<String>> {
        self.lengths.lock().unwrap().clone()
    }

    fn answer(&self, method: &Method, path: &str, token: &str, body: &[u8]) -> (u16, Value) {
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
            ("POST", None) => self.acquire(&mut locks, &mut defunct, token, body),
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

    /// Acquires a lock for `token`, renews the one it owns, or breaks the lock
    /// of another client for `break_lock`.
    fn acquire(
        &self,
        locks: &mut HashMap<String, String>,
        defunct: &mut HashMap<String, String>,
        token: &str,
        body: &[u8],
    ) -> (u16, Value) {
        if let Some((id, owner)) = locks.iter().next() {
            if owner == token {
                return (200, json!({ "id": id, "owned": true }));
            }
            let request: Value = serde_json::from_slice(body).unwrap_or_default();
            if request.get("break_lock") != Some(&Value::Bool(true)) {
                return (409, json!({}));
            }
            if *self.refuse_breaks.lock().unwrap() {
                return (
                    423,
                    json!({
                        "error_code": "vendor-specific",
                        "vendor_code": "lock-priority-denied",
                        "message": "Lock preemption denied",
                    }),
                );
            }
            defunct.extend(locks.drain());
        }
        let id = self.next_id();
        locks.insert(id.clone(), token.to_owned());
        if *self.unnamed.lock().unwrap() {
            return (201, json!({ "owned": true }));
        }
        (201, json!({ "id": id, "owned": true }))
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
                    let length = request
                        .headers()
                        .get("content-length")
                        .map(|length| length.to_str().unwrap().to_owned());
                    backend.lengths.lock().unwrap().push(length);
                    let body = request.into_body().collect().await.unwrap().to_bytes();
                    let seen = format!("{method} {path} {}", String::from_utf8_lossy(&body));
                    backend.seen.lock().unwrap().push(seen.trim().to_owned());
                    let (status, answer) = backend.answer(&method, &path, &token, &body);
                    // A refused token gets a challenge and no body, as from
                    // the auth of opensovd-server.
                    let response = if status == 401 {
                        Response::builder()
                            .status(status)
                            .header("www-authenticate", r#"Bearer error="invalid_token""#)
                            .body(Full::new(Bytes::new()))
                    } else {
                        Response::builder()
                            .status(status)
                            .header("content-type", "application/json")
                            .body(Full::new(Bytes::from(answer.to_string())))
                    };
                    Ok::<_, Infallible>(response.unwrap())
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
    acquire_as(server, "x").await
}

async fn acquire_as(server: &TestServer, token: &str) -> String {
    let (status, json) = send(server, token, Method::POST, "/locks", LOCK).await;
    assert_eq!(status, 201);
    lock(json).id.unwrap()
}

/// Asserts that the lock `id` of the server is a lock of `target` alone: its
/// details are asked of `target`, and of none of the `others`.
async fn assert_part_of(server: &TestServer, id: &str, target: &Backend, others: &[&Backend]) {
    let before: Vec<_> = others.iter().map(|other| other.seen().len()).collect();
    let (status, _) = send(server, "x", Method::GET, &format!("/locks/{id}"), b"").await;
    assert_eq!(status, 200);
    assert!(target.seen().last().unwrap().starts_with("GET /locks/"));
    let after: Vec<_> = others.iter().map(|other| other.seen().len()).collect();
    assert_eq!(after, before);
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
#[case(Method::POST, "/locks", LOCK)]
#[case(Method::GET, "/locks", b"")]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn a_refused_token_keeps_the_challenge_of_the_upstream(
    #[case] method: Method,
    #[case] path: &str,
    #[case] body: &'static [u8],
) {
    let (a, b) = pair();
    b.refuse(401);
    let server = start(&[&a, &b]).await;

    let request = Request::builder()
        .method(method)
        .header("authorization", "x")
        .uri(format!("http://{}/sovd/v1{path}", server.addr))
        .body(Full::new(Bytes::from_static(body)))
        .unwrap();
    let response = common::client().request(request).await.unwrap();

    assert_eq!(response.status(), 401);
    assert_eq!(
        response.headers()["www-authenticate"],
        r#"Bearer error="invalid_token""#
    );
    assert_eq!(a.held(), 0);
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
    // a read ends nothing, and neither does a change of the expiration
    assert_eq!(a.held(), 1);
    let (status, _) = send(&server, "x", Method::PUT, &path, LOCK).await;
    assert_eq!(status, 409);
    assert_eq!(a.held(), 1);
    // the client's DELETE does
    let (status, _) = send(&server, "x", Method::DELETE, &path, b"").await;
    assert_eq!(status, 200);
    assert_eq!(a.held(), 0);
    let (status, _) = send(&server, "x", Method::GET, &path, b"").await;
    assert_eq!(status, 404);
}

/// A lock an upstream grants without naming it cannot be part of the lock, and
/// is released with the others.
#[rstest]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn a_lock_granted_without_a_name_is_released() {
    let (a, b) = pair();
    b.grant_unnamed();
    let server = start(&[&a, &b]).await;

    let (status, _) = send(&server, "x", Method::POST, "/locks", LOCK).await;

    assert_eq!(status, 502);
    assert_eq!((a.held(), b.held()), (0, 0));
}

#[rstest]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn a_lock_whose_first_part_is_gone_lists_what_is_left() {
    let (a, b) = pair();
    let server = start(&[&a, &b]).await;
    let id = acquire(&server).await;
    a.lose_locks();

    let (status, json) = send(&server, "x", Method::GET, "/locks", b"").await;

    assert_eq!(status, 200);
    let items = locks(json);
    assert_eq!(items.len(), 1);
    let left = items.first().unwrap().id.clone().unwrap();
    assert_ne!(left, id);
    assert_part_of(&server, &left, &b, &[&a]).await;
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

/// The upstreams get a `DELETE` without a body, so no length of the client's
/// body may come with it.
#[rstest]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn releasing_sends_no_body_and_no_length_of_one() {
    let (a, b) = pair();
    let server = start(&[&a, &b]).await;
    let id = acquire(&server).await;

    let (status, _) = send(&server, "x", Method::DELETE, &format!("/locks/{id}"), LOCK).await;

    assert_eq!(status, 200);
    assert_eq!((a.held(), b.held()), (0, 0));
    for backend in [&a, &b] {
        assert_eq!(
            backend.seen().last().unwrap(),
            &format!("DELETE /locks/{}-lock", backend.name)
        );
        assert_eq!(backend.lengths().last().unwrap(), &None);
    }
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

/// An id is a lock of the server only if it names a part on each of its
/// upstreams, or none: anything else reaches no upstream.
#[rstest]
#[case::not_encoded("unknown")]
#[case::too_few_parts("WyJhLWxvY2siXQ")]
#[case::too_many_parts("WyJhLWxvY2siLCJiLWxvY2siLCJjIl0")]
#[case::no_part("W251bGwsbnVsbF0")]
#[case::no_list("eyJhIjoxfQ")]
#[case::too_long(&"A".repeat(5000))]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn an_id_that_names_no_parts_is_not_found(#[case] id: &str) {
    let (a, b) = pair();
    let server = start(&[&a, &b]).await;

    let (status, _) = send(&server, "x", Method::GET, &format!("/locks/{id}"), b"").await;

    assert_eq!(status, 404);
    assert_eq!(a.seen(), Vec::<String>::new());
    assert_eq!(b.seen(), Vec::<String>::new());
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
async fn a_lock_whose_later_part_is_gone_lists_what_is_left() {
    let (a, b) = pair();
    let server = start(&[&a, &b]).await;
    let id = acquire(&server).await;
    b.lose_locks();

    let (status, json) = send(&server, "x", Method::GET, "/locks", b"").await;

    assert_eq!(status, 200);
    let items = locks(json);
    assert_eq!(items.len(), 1);
    let left = items.first().unwrap().id.clone().unwrap();
    assert_ne!(left, id);
    assert_part_of(&server, &left, &a, &[&b]).await;
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

/// A lock acquired from an upstream directly is a lock of the server of a
/// single part, which a client can ask for and its owner release.
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
    let id = lock.id.clone().unwrap();
    assert_ne!(id, direct);
    assert_eq!(lock.owned, Some(false));
    assert_part_of(&server, &id, &b, &[&a]).await;

    let (status, _) = send(&server, "x", Method::POST, "/locks", LOCK).await;
    assert_eq!(status, 409);

    let path = format!("/locks/{id}");
    let (status, _) = send(&server, "x", Method::DELETE, &path, b"").await;
    assert_eq!(status, 403);
    let (status, _) = send(&server, "z", Method::DELETE, &path, b"").await;
    assert_eq!(status, 200);
    assert_eq!(b.held(), 0);
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

/// The parts a client owns on all upstreams are its lock, whatever the server
/// remembers: the lock lasts as long as its parts do.
#[rstest]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn the_parts_a_client_owns_on_all_upstreams_are_its_lock() {
    let (a, b) = pair();
    let server = start(&[&a, &b]).await;
    let body = br#"{"lock_expiration":0}"#;
    let (status, json) = send(&server, "x", Method::POST, "/locks", body).await;
    assert_eq!(status, 201);
    let id = lock(json).id.unwrap();

    let (_, json) = send(&server, "x", Method::GET, "/locks", b"").await;

    let ids: Vec<_> = locks(json).into_iter().filter_map(|lock| lock.id).collect();
    assert_eq!(ids, [id.as_str()]);
    let (status, _) = send(&server, "x", Method::GET, &format!("/locks/{id}"), b"").await;
    assert_eq!(status, 200);
}

const BREAK: &[u8] = br#"{"lock_expiration":60,"break_lock":true}"#;

/// X holds the vehicle, Y breaks it: `a` grants `break_lock` and breaks X's
/// part, `b` refuses. Returns X's lock and Y's answer.
async fn break_partly(server: &TestServer, a: &Backend, b: &Backend) -> (String, u16, Value) {
    let id = acquire_as(server, "x").await;
    b.refuse_breaks();
    let (status, json) = send(server, "y", Method::POST, "/locks", BREAK).await;
    assert_eq!(
        a.seen().get(1).unwrap(),
        &format!("POST /locks {}", std::str::from_utf8(BREAK).unwrap())
    );
    (id, status, json)
}

/// A refusal after an upstream granted `break_lock` keeps the refusing
/// upstream's status and error, and tells how many upstreams granted it, as
/// each may have broken a lock that nothing restores.
#[rstest]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn a_refused_break_tells_how_many_upstreams_granted_it() {
    let (a, b) = pair();
    let server = start(&[&a, &b]).await;

    let (_, status, json) = break_partly(&server, &a, &b).await;

    assert_eq!(status, 423);
    let error: GenericError = serde_json::from_value(json).unwrap();
    assert!(matches!(error.error_code, ErrorCode::VendorSpecific));
    assert_eq!(error.vendor_code.as_deref(), Some("lock-priority-denied"));
    assert_eq!(error.message, "An upstream refused the lock");
    let parameters = error.parameters.unwrap();
    assert_eq!(parameters.get("break_lock_granted"), Some(&Value::from(1)));
    // X's part on `a` is broken for good, Y's part there is released, and X
    // still holds `b`.
    assert_eq!((a.defunct(), a.held()), (1, 0));
    assert_eq!((b.defunct(), b.held()), (0, 1));
}

/// Without a grant before the refusal nothing was broken, and the refusal is
/// the upstream's as it is.
#[rstest]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn a_break_refused_first_breaks_nothing() {
    let (a, b) = pair();
    let server = start(&[&a, &b]).await;
    acquire_as(&server, "x").await;
    a.refuse_breaks();

    let (status, json) = send(&server, "y", Method::POST, "/locks", BREAK).await;

    assert_eq!(status, 423);
    let error: GenericError = serde_json::from_value(json).unwrap();
    assert_eq!(error.parameters, None);
    assert_eq!(b.seen().len(), 1);
    assert_eq!((a.defunct(), b.defunct()), (0, 0));
    assert_eq!((a.held(), b.held()), (1, 1));
}

/// A refusal that does not follow `break_lock` says nothing about breaking,
/// whatever the upstreams before it granted.
#[rstest]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn a_refusal_without_break_lock_tells_nothing_about_breaking() {
    let (a, b) = pair();
    b.refuse(409);
    let server = start(&[&a, &b]).await;

    let (status, json) = send(&server, "x", Method::POST, "/locks", LOCK).await;

    assert_eq!(status, 409);
    let error: GenericError = serde_json::from_value(json).unwrap();
    assert_eq!(error.parameters, None);
}

/// The client whose lock was partly broken learns it from the server alone,
/// as it cannot reach the upstreams, and releases what is left by its `DELETE`.
/// Then the client that broke it gets the vehicle without `break_lock`.
#[rstest]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn both_clients_recover_from_a_partial_break_through_the_server() {
    let (a, b) = pair();
    let server = start(&[&a, &b]).await;
    let (id, _, _) = break_partly(&server, &a, &b).await;
    let path = format!("/locks/{id}");

    // X: the details show who broke it, a modification fails with lock-broken
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
    let (status, json) = send(&server, "x", Method::GET, "/locks", b"").await;
    assert_eq!(status, 200);
    let items = locks(json);
    assert_eq!(items.len(), 1);
    let listed = items.first().unwrap();
    assert_eq!(listed.id.as_ref(), Some(&id));
    assert_eq!(
        listed.extensions.get("x-sovd2uds-broken-by"),
        Some(&Value::from("priority-app"))
    );

    // Y: retrying the break fails the same way, as X still holds `b`
    let (status, _) = send(&server, "y", Method::POST, "/locks", BREAK).await;
    assert_eq!(status, 423);

    // X: its DELETE clears the broken part and releases the rest
    let (status, _) = send(&server, "x", Method::DELETE, &path, b"").await;
    assert_eq!(status, 200);
    assert_eq!((a.defunct(), a.held(), b.held()), (0, 0, 0));

    // Y: the vehicle is free, no break needed
    let (status, _) = send(&server, "y", Method::POST, "/locks", LOCK).await;
    assert_eq!(status, 201);
    assert_eq!((a.defunct(), b.defunct()), (0, 0));
}

/// The id names the parts, so a restart of the server loses no lock.
#[rstest]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn a_lock_outlives_a_restart_of_the_server() {
    let (a, b) = pair();
    let id = acquire(&start(&[&a, &b]).await).await;

    let server = start(&[&a, &b]).await;

    let path = format!("/locks/{id}");
    let (status, _) = send(&server, "x", Method::GET, &path, b"").await;
    assert_eq!(status, 200);
    let (_, json) = send(&server, "x", Method::GET, "/locks", b"").await;
    let ids: Vec<_> = locks(json).into_iter().filter_map(|lock| lock.id).collect();
    assert_eq!(ids, [id.as_str()]);
    let (status, json) = send(&server, "x", Method::POST, "/locks", LOCK).await;
    assert_eq!(status, 200);
    assert_eq!(lock(json).id, Some(id));
    let (status, _) = send(&server, "x", Method::PUT, &path, LOCK).await;
    assert_eq!(status, 200);
    let (status, _) = send(&server, "x", Method::DELETE, &path, b"").await;
    assert_eq!(status, 200);
    assert_eq!((a.held(), b.held()), (0, 0));
}
