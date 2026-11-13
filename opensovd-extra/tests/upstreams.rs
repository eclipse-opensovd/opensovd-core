// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Forwarding to several upstreams: who answers for what.

#![expect(clippy::unwrap_used)]

use std::collections::HashMap;
use std::convert::Infallible;
use std::future::pending;
use std::sync::{Arc, Mutex};

use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::{Method, Request, Response};
use opensovd_extra::{Federation, ForwardRoute, SharedForwardRoute, Upstream};
use opensovd_models::GenericError;
use opensovd_models::discovery::{Entities, EntityCapabilities, EntityReference};
use opensovd_server::{Server, Topology};
use rstest::rstest;
use serde::Deserialize;
use serde_json::{Map, Value};
use tokio::net::TcpListener;
use tokio::sync::oneshot;

mod common;

/// A SOVD server with the entities `ids`, which change at runtime, and that
/// answers every request with `status` if set. It names itself in its answers
/// and filters its list by the `tags` of the query, as a SOVD server does.
#[derive(Clone, Default)]
struct Backend {
    name: &'static str,
    ids: Arc<Mutex<Vec<String>>>,
    tags: Arc<Mutex<HashMap<String, String>>>,
    status: Arc<Mutex<Option<u16>>>,
    seen: Arc<Mutex<Vec<String>>>,
}

impl Backend {
    fn new(name: &'static str, ids: &[&str]) -> Self {
        let backend = Self {
            name,
            ..Self::default()
        };
        for id in ids {
            backend.add(id);
        }
        backend
    }

    fn add(&self, id: &str) {
        self.ids.lock().unwrap().push(id.to_owned());
    }

    /// Gives the entity `id` the one tag `tag`.
    fn tag(&self, id: &str, tag: &str) {
        self.tags
            .lock()
            .unwrap()
            .insert(id.to_owned(), tag.to_owned());
    }

    fn remove(&self, id: &str) {
        self.ids.lock().unwrap().retain(|held| held != id);
    }

    fn set_status(&self, status: u16) {
        *self.status.lock().unwrap() = Some(status);
    }

    fn clear_status(&self) {
        *self.status.lock().unwrap() = None;
    }

    /// The requests so far, such as `PUT /components/ecu`.
    fn seen(&self) -> Vec<String> {
        self.seen.lock().unwrap().clone()
    }

    fn answer(&self, path: &str, query: &str) -> (u16, String) {
        if let Some(status) = *self.status.lock().unwrap() {
            return (status, "{}".to_owned());
        }
        let ids = self.ids.lock().unwrap();
        let tags = self.tags.lock().unwrap();
        let wanted: Vec<_> = query
            .split('&')
            .filter_map(|pair| pair.strip_prefix("tags="))
            .collect();
        let name = self.name;
        if path.ends_with("/missing") {
            return (404, "{}".to_owned());
        }
        let mut segments = path.trim_start_matches('/').split('/');
        match (segments.next(), segments.next()) {
            (Some("components"), None) => {
                let items: Vec<_> = ids
                    .iter()
                    .filter(|id| {
                        wanted.is_empty()
                            || tags
                                .get(*id)
                                .is_some_and(|tag| wanted.contains(&tag.as_str()))
                    })
                    .map(|id| {
                        let tags = tags
                            .get(id)
                            .map_or_else(String::new, |tag| format!(r#","tags":["{tag}"]"#));
                        format!(r#"{{"id":"{id}","name":"{name}","href":"http://x/{id}"{tags}}}"#)
                    })
                    .collect();
                (200, format!(r#"{{"items":[{}]}}"#, items.join(",")))
            }
            (Some("components"), Some(id)) if !ids.iter().any(|held| held == id) => {
                (404, "{}".to_owned())
            }
            (Some(""), None) => (
                200,
                format!(
                    r#"{{"functions":"http://x/f","locks":"http://x/l","x-{name}":"http://x"}}"#
                ),
            ),
            _ => (200, format!(r#"{{"backend":"{name}"}}"#)),
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
                    let query = request.uri().query().unwrap_or_default().to_owned();
                    backend
                        .seen
                        .lock()
                        .unwrap()
                        .push(format!("{method} {path}"));
                    // as a server that answers a matching condition does
                    if request.headers().contains_key("if-none-match") {
                        return Ok::<_, Infallible>(
                            Response::builder()
                                .status(304)
                                .body(Full::new(Bytes::new()))
                                .unwrap(),
                        );
                    }
                    let length = request.headers().get("content-length").cloned();
                    let body = request.into_body().collect().await.unwrap().to_bytes();
                    let declared = length.map(|l| l.to_str().unwrap().parse::<usize>().unwrap());
                    assert_eq!(
                        declared.unwrap_or(body.len()),
                        body.len(),
                        "{method} {path}"
                    );
                    let (status, answer) = backend.answer(&path, &query);
                    Ok::<_, Infallible>(
                        Response::builder()
                            .status(status)
                            .header("content-type", "application/json")
                            .body(Full::new(Bytes::from(answer)))
                            .unwrap(),
                    )
                }
            },
        ))
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

async fn start(upstreams: Vec<Upstream>) -> TestServer {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let topology = opensovd_mocks::create_mock_topology().await;
    let server = Server::builder()
        .listener(listener)
        .base_uri("/sovd")
        .unwrap()
        .topology(topology.clone())
        .inner_layer(Federation::new(upstreams, topology).unwrap().into_layer())
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

/// Two upstreams the way the gateway sets them up: the first takes everything.
async fn start_two(first: &Backend, second: &Backend) -> TestServer {
    start(vec![
        first.upstream(),
        second.upstream().routes([ForwardRoute::Entities]),
    ])
    .await
}

async fn send(
    server: &TestServer,
    method: Method,
    path: &str,
    body: &'static [u8],
) -> (u16, Value) {
    let request = Request::builder()
        .method(method)
        .header("authorization", "Bearer t")
        .uri(format!("http://{}/sovd/v1{path}", server.addr))
        .body(Full::new(Bytes::from_static(body)))
        .unwrap();
    let response = common::client().request(request).await.unwrap();
    let status = response.status().as_u16();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&body).unwrap())
}

/// The name a [`Backend`] gives in its answers, empty if another answered.
fn backend(json: Value) -> String {
    #[derive(Deserialize)]
    struct Answer {
        backend: String,
    }
    serde_json::from_value::<Answer>(json)
        .map(|answer| answer.backend)
        .unwrap_or_default()
}

fn entities(json: Value) -> Vec<EntityReference> {
    let list: Entities = serde_json::from_value(json).unwrap();
    list.items
}

fn vendor_code(json: Value) -> Option<String> {
    let error: GenericError = serde_json::from_value(json).unwrap();
    error.vendor_code
}

async fn get(server: &TestServer, path: &str) -> (u16, String) {
    let (status, json) = send(server, Method::GET, path, b"").await;
    (status, backend(json))
}

#[rstest]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn a_tag_filter_lists_no_upstream_entity_that_the_server_holds() {
    // The server's `ecu` has the tags `powertrain` and `critical` only.
    let a = Backend::new("a", &["ecu", "tagged"]);
    a.tag("ecu", "network");
    a.tag("tagged", "network");
    let server = start_two(&a, &Backend::new("b", &[])).await;

    let (status, json) = send(&server, Method::GET, "/components?tags=network", b"").await;

    assert_eq!(status, 200);
    let mut ids: Vec<_> = entities(json).into_iter().map(|e| e.id).collect();
    ids.sort();
    assert_eq!(ids, ["gateway", "tagged"]);
}

#[rstest]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn a_tag_filter_lists_an_entity_of_the_upstream_that_has_its_id() {
    let a = Backend::new("a", &["both", "first"]);
    let b = Backend::new("b", &["both", "second"]);
    a.tag("first", "x");
    b.tag("both", "x");
    b.tag("second", "x");
    let server = start_two(&a, &b).await;

    let (_, json) = send(&server, Method::GET, "/components?tags=x", b"").await;

    // `both` is a's, which does not have the tag, and requests for it reach a.
    let found: Vec<_> = entities(json).into_iter().map(|e| (e.id, e.name)).collect();
    assert_eq!(
        found,
        [
            ("first".to_owned(), "a".to_owned()),
            ("second".to_owned(), "b".to_owned())
        ]
    );
    assert_eq!(
        get(&server, "/components/both").await,
        (200, "a".to_owned())
    );
}

#[rstest]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn lists_hold_the_entities_of_all_upstreams_once() {
    let a = Backend::new("a", &["flxc1000", "both"]);
    let b = Backend::new("b", &["both", "hovr4000"]);
    let server = start_two(&a, &b).await;

    let (status, json) = send(&server, Method::GET, "/components", b"").await;

    assert_eq!(status, 200);
    let items = entities(json);
    let ids: Vec<_> = items.iter().map(|item| item.id.as_str()).collect();
    assert_eq!(ids, ["ecu", "gateway", "flxc1000", "both", "hovr4000"]);
    assert_eq!(items.get(3).unwrap().name, "a");
}

#[rstest]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn a_list_holds_what_the_other_upstreams_answer() {
    let a = Backend::new("a", &["flxc1000"]);
    let b = Backend::new("b", &["hovr4000"]);
    a.set_status(503);
    let server = start_two(&a, &b).await;

    let (status, json) = send(&server, Method::GET, "/components", b"").await;

    assert_eq!(status, 200);
    assert_eq!(entities(json).len(), 3);
}

#[rstest]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn the_root_holds_the_links_each_upstream_serves() {
    let a = Backend::new("a", &[]);
    let b = Backend::new("b", &[]);
    let server = start(vec![
        a.upstream()
            .routes([ForwardRoute::Entities, ForwardRoute::Functions]),
        b.upstream()
            .routes([ForwardRoute::Entities, ForwardRoute::CustomCollections]),
    ])
    .await;

    let (status, json) = send(&server, Method::GET, "", b"").await;

    assert_eq!(status, 200);
    let links: Map<String, Value> = serde_json::from_value(json.clone()).unwrap();
    let root: EntityCapabilities = serde_json::from_value(json).unwrap();
    assert!(root.components.is_some());
    assert!(root.functions.is_some());
    // custom collections have no field
    assert!(links.contains_key("x-b"));
    // both offer these, but a client would not reach them
    assert!(root.locks.is_none());
    assert!(!links.contains_key("x-a"));
}

#[rstest]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn a_request_goes_to_the_owner_of_its_entity_only() {
    let a = Backend::new("a", &["flxc1000"]);
    let b = Backend::new("b", &["hovr4000"]);
    let server = start_two(&a, &b).await;

    let path = "/components/hovr4000/modes/session";
    let (status, json) = send(&server, Method::PUT, path, b"{}").await;

    assert_eq!(status, 200);
    assert_eq!(backend(json), "b");
    assert_eq!(a.seen(), ["GET /components/hovr4000"]);
    assert_eq!(
        b.seen(),
        [
            "GET /components/hovr4000",
            "PUT /components/hovr4000/modes/session"
        ]
    );
}

#[rstest]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn a_known_owner_is_not_looked_for_again() {
    let a = Backend::new("a", &[]);
    let b = Backend::new("b", &["hovr4000"]);
    let server = start_two(&a, &b).await;

    assert_eq!(get(&server, "/components/hovr4000/faults").await.0, 200);
    assert_eq!(get(&server, "/components/hovr4000/faults").await.0, 200);

    assert_eq!(a.seen().len(), 1);
}

/// An upstream that refuses the client may have the entity, so an upstream
/// after it does not get it: an id names the same entity for every client.
#[rstest]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn an_upstream_that_refuses_keeps_the_entity_from_later_ones() {
    let a = Backend::new("a", &["both"]);
    let b = Backend::new("b", &["both"]);
    let server = start_two(&a, &b).await;

    a.set_status(403);
    assert_eq!(get(&server, "/components/both/faults").await.0, 403);
    assert_eq!(b.seen(), ["GET /components/both"]);
    a.clear_status();
    assert_eq!(
        get(&server, "/components/both/faults").await,
        (200, "a".to_owned())
    );
}

/// A 404 below an entity, such as for an unknown data identifier, keeps the
/// owner, so that the upstreams are not asked again.
#[rstest]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn a_404_below_the_entity_keeps_its_owner() {
    let a = Backend::new("a", &[]);
    let b = Backend::new("b", &["hovr4000"]);
    let server = start_two(&a, &b).await;
    assert_eq!(get(&server, "/components/hovr4000/x").await.0, 200);

    assert_eq!(get(&server, "/components/hovr4000/missing").await.0, 404);
    assert_eq!(get(&server, "/components/hovr4000/missing").await.0, 404);

    assert_eq!(a.seen(), ["GET /components/hovr4000"]);
}

/// The upstreams answer a list for the server, so the client's conditions,
/// which apply to the server's answer, do not reach them.
#[rstest]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn a_conditional_list_request_lists_the_upstreams_entities() {
    let a = Backend::new("a", &["flxc1000"]);
    let server = start_two(&a, &Backend::new("b", &[])).await;

    let request = Request::builder()
        .header("authorization", "Bearer t")
        .header("if-none-match", "\"x\"")
        .uri(format!("http://{}/sovd/v1/components", server.addr))
        .body(Full::new(Bytes::new()))
        .unwrap();
    let response = common::client().request(request).await.unwrap();

    assert_eq!(response.status(), 200);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let ids: Vec<_> = entities(serde_json::from_slice(&body).unwrap())
        .into_iter()
        .map(|e| e.id)
        .collect();
    assert!(ids.contains(&"flxc1000".to_owned()), "{ids:?}");
}

/// The list request of a client may have a body, which the upstreams'
/// discovery requests do not, so they get none of its framing either.
#[rstest]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn a_list_request_with_a_body_lists_the_upstreams_entities() {
    let a = Backend::new("a", &["flxc1000"]);
    let server = start_two(&a, &Backend::new("b", &["hovr4000"])).await;

    let (status, json) = send(&server, Method::GET, "/components", b"{}").await;

    assert_eq!(status, 200);
    let ids: Vec<_> = entities(json).into_iter().map(|e| e.id).collect();
    assert!(ids.contains(&"flxc1000".to_owned()), "{ids:?}");
    assert!(ids.contains(&"hovr4000".to_owned()), "{ids:?}");
}

/// A dot segment could reach another resource of an upstream that removes
/// it, so such a path stays with the server, also with a single upstream.
#[rstest]
#[case("/components/../locks")]
#[case("/components/%2e%2E/locks")]
#[case("/components/ecu2/./data")]
#[case("/data/../locks")]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn a_path_with_a_dot_segment_goes_to_no_upstream(#[case] path: &str) {
    let a = Backend::new("a", &["ecu2"]);
    let server = start(vec![a.upstream()]).await;

    let (status, _) = send(&server, Method::GET, path, b"").await;

    assert_eq!(status, 404, "{path}");
    assert!(a.seen().is_empty(), "{path}: {:?}", a.seen());
}

#[rstest]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn equal_ids_go_to_the_first_upstream() {
    let a = Backend::new("a", &["both"]);
    let b = Backend::new("b", &["both"]);
    let server = start_two(&a, &b).await;

    let answer = get(&server, "/components/both/faults").await;

    assert_eq!(answer, (200, "a".to_owned()));
}

#[rstest]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn the_servers_own_entities_never_go_upstream() {
    let a = Backend::new("a", &["ecu"]);
    let b = Backend::new("b", &["ecu"]);
    let server = start_two(&a, &b).await;

    assert_eq!(get(&server, "/components/ecu").await.0, 200);

    assert_eq!(b.seen(), Vec::<String>::new());
}

#[rstest]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn an_entity_registered_later_is_found() {
    let a = Backend::new("a", &[]);
    let b = Backend::new("b", &[]);
    let server = start_two(&a, &b).await;

    let (status, json) = send(&server, Method::GET, "/components/late", b"").await;
    assert_eq!(status, 404);
    assert_eq!(vendor_code(json).as_deref(), Some("entity-not-found"));

    b.add("late");

    assert_eq!(
        get(&server, "/components/late/x").await,
        (200, "b".to_owned())
    );
}

#[rstest]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn an_entity_that_moved_is_found_by_the_next_request() {
    let a = Backend::new("a", &["moves"]);
    let b = Backend::new("b", &[]);
    let server = start_two(&a, &b).await;
    assert_eq!(
        get(&server, "/components/moves/x").await,
        (200, "a".to_owned())
    );

    a.remove("moves");
    b.add("moves");

    assert_eq!(get(&server, "/components/moves/x").await.0, 404);
    assert_eq!(
        get(&server, "/components/moves/x").await,
        (200, "b".to_owned())
    );
}

#[rstest]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn a_failing_upstream_answers_for_the_entities_after_it() {
    let a = Backend::new("a", &[]);
    let b = Backend::new("b", &["hovr4000"]);
    a.set_status(502);
    let server = start_two(&a, &b).await;

    // `a` may have the entity, so `b` does not get it
    assert_eq!(get(&server, "/components/hovr4000/x").await.0, 502);
    a.clear_status();
    assert_eq!(
        get(&server, "/components/hovr4000/x").await,
        (200, "b".to_owned())
    );
    // a known owner answers whatever the upstreams before it do
    a.set_status(502);
    assert_eq!(
        get(&server, "/components/hovr4000/x").await,
        (200, "b".to_owned())
    );
}

#[rstest]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn without_an_owner_a_failure_is_the_answer() {
    let a = Backend::new("a", &[]);
    let b = Backend::new("b", &[]);
    b.set_status(502);
    let server = start_two(&a, &b).await;

    assert_eq!(get(&server, "/components/gone/x").await.0, 502);
}

#[rstest]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn an_upstream_that_refuses_the_client_answers_for_itself() {
    let a = Backend::new("a", &[]);
    let b = Backend::new("b", &[]);
    b.set_status(401);
    let server = start_two(&a, &b).await;

    assert_eq!(get(&server, "/components/guarded/x").await.0, 401);
    assert_eq!(b.seen().len(), 2);
}

#[rstest]
#[case(Method::POST, "/authorize")]
#[case(Method::GET, "/functions")]
#[case(Method::GET, "/data")]
#[case(Method::GET, "/x-vendor")]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn what_has_one_answer_goes_to_its_upstream(#[case] method: Method, #[case] path: &str) {
    let a = Backend::new("a", &[]);
    let b = Backend::new("b", &[]);
    let server = start_two(&a, &b).await;

    let (status, json) = send(&server, method.clone(), path, b"{}").await;

    assert_eq!(status, 200, "{method} {path}");
    assert_eq!(backend(json), "a", "{method} {path}");
    assert_eq!(b.seen(), Vec::<String>::new(), "{method} {path}");
}

#[rstest]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn the_locks_of_an_entity_go_with_the_entity() {
    let a = Backend::new("a", &[]);
    let b = Backend::new("b", &["hovr4000"]);
    let server = start_two(&a, &b).await;

    let path = "/components/hovr4000/locks";
    let (status, json) = send(&server, Method::POST, path, b"{}").await;

    assert_eq!(status, 200);
    assert_eq!(backend(json), "b");
}

#[test]
fn a_route_can_only_have_one_upstream() {
    let a = Backend::new("a", &[]);
    let b = Backend::new("b", &[]);

    let built = Federation::new(
        vec![
            a.upstream().routes([ForwardRoute::Functions]),
            b.upstream().routes([ForwardRoute::Functions]),
        ],
        Topology::default(),
    );

    assert!(matches!(
        built,
        Err(SharedForwardRoute(ForwardRoute::Functions))
    ));
}

#[test]
fn upstreams_share_entities_and_locks() {
    let a = Backend::new("a", &[]);
    let b = Backend::new("b", &[]);
    let shared = [ForwardRoute::Entities, ForwardRoute::Locks];

    let built = Federation::new(
        vec![a.upstream().routes(shared), b.upstream().routes(shared)],
        Topology::default(),
    );

    assert!(built.is_ok());
}

/// An upstream that never answers, with a short time for finding out what it
/// has.
fn hanging() -> Upstream {
    let service = tower::service_fn(|_: Request<opensovd_server::Body>| {
        pending::<Result<Response<Full<Bytes>>, Infallible>>()
    });
    Upstream::new(service)
        .routes([ForwardRoute::Entities])
        .discovery_timeout(std::time::Duration::from_millis(50))
}

/// An upstream that hangs takes itself out of a list, not the list.
#[rstest]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn a_list_stands_without_an_upstream_that_hangs() {
    let a = Backend::new("a", &["flxc1000"]);
    let server = start(vec![
        a.upstream().routes([ForwardRoute::Entities]),
        hanging(),
    ])
    .await;

    let (status, json) = send(&server, Method::GET, "/components", b"").await;

    assert_eq!(status, 200);
    assert!(entities(json).iter().any(|item| item.id == "flxc1000"));
}

/// An upstream that hangs keeps no owner from answering, even if it comes first.
#[rstest]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn an_owner_answers_next_to_an_upstream_that_hangs() {
    let a = Backend::new("a", &["flxc1000"]);
    let server = start(vec![
        a.upstream().routes([ForwardRoute::Entities]),
        hanging(),
    ])
    .await;

    let (status, _) = send(&server, Method::GET, "/components/flxc1000", b"").await;

    assert_eq!(status, 200);
    assert_eq!(
        a.seen()
            .iter()
            .filter(|seen| *seen == "GET /components/flxc1000")
            .count(),
        2,
        "the look and the request"
    );
}

/// Without an owner, the upstream that hangs may have been it.
/// An upstream that hangs may have the entity, so one after it does not get
/// it.
#[rstest]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn an_upstream_that_hangs_answers_for_the_entities_after_it() {
    let a = Backend::new("a", &["flxc1000"]);
    let server = start(vec![
        hanging(),
        a.upstream().routes([ForwardRoute::Entities]),
    ])
    .await;

    let (status, json) = send(&server, Method::GET, "/components/flxc1000", b"").await;

    assert_eq!(status, 504);
    assert_eq!(vendor_code(json).as_deref(), Some("upstream-timeout"));
    assert_eq!(a.seen(), ["GET /components/flxc1000"]);
}

#[rstest]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn an_upstream_that_hangs_answers_for_an_unknown_entity() {
    let a = Backend::new("a", &[]);
    let server = start(vec![
        a.upstream().routes([ForwardRoute::Entities]),
        hanging(),
    ])
    .await;

    let (status, json) = send(&server, Method::GET, "/components/unknown", b"").await;

    assert_eq!(status, 504);
    assert_eq!(vendor_code(json).as_deref(), Some("upstream-timeout"));
}
