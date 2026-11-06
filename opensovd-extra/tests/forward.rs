// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Forwarding of what the server does not serve to an upstream service.

#![expect(clippy::unwrap_used)]

use std::convert::Infallible;
use std::sync::{Arc, Mutex};

use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::{Method, Request, Response};
use opensovd_extra::{Federation, ForwardRoute, Upstream as ServerUpstream};
use opensovd_models::discovery::{Entities, EntityCapabilities};
use opensovd_models::{GenericError, UriReference};
use opensovd_server::{
    Access, Auth, AuthError, Authenticator, Authorizer, Parts, Principal, Server, Topology,
};
use rstest::rstest;
use serde_json::{Map, Value};
use tokio::net::TcpListener;
use tokio::sync::oneshot;

mod common;

/// A request as the upstream saw it.
#[derive(Clone, Debug)]
struct Seen {
    method: Method,
    uri: String,
    authorization: Option<String>,
    body: Bytes,
}

const UPSTREAM_BODY: &[u8] = b"\x00\x01not json";

/// The entity list of the upstream for `path`, unless the client is refused.
fn upstream_list(path: &str, authorization: Option<&str>) -> Option<Response<Full<Bytes>>> {
    let body: &[u8] = match path {
        // any upstream, not the CDA, which has no root
        "/" => {
            br#"{"id":"upstream","name":"An upstream","translation_id":"upstream",
                "areas":"http://upstream:8080/api/areas",
                "components":"http://upstream:8080/api/components",
                "apps":"http://upstream:8080/api/apps",
                "functions":"http://upstream:8080/api/functions",
                "locks":"http://upstream:8080/api/locks",
                "belongs-to":"http://upstream:8080/api/belongs-to",
                "bulk-data":"http://upstream:8080/api/bulk-data",
                "communication-logs":"http://upstream:8080/api/communication-logs",
                "configurations":"http://upstream:8080/api/configurations",
                "cyclic-subscriptions":"http://upstream:8080/api/cyclic-subscriptions",
                "data":"http://upstream:8080/api/data",
                "data-categories":"http://upstream:8080/api/data-categories",
                "data-groups":"http://upstream:8080/api/data-groups",
                "data-lists":"http://upstream:8080/api/data-lists",
                "depends-on":"http://upstream:8080/api/depends-on",
                "faults":"http://upstream:8080/api/faults",
                "logs":"http://upstream:8080/api/logs",
                "modes":"http://upstream:8080/api/modes",
                "operations":"http://upstream:8080/api/operations",
                "scripts":"http://upstream:8080/api/scripts",
                "triggers":"http://upstream:8080/api/triggers",
                "updates":"http://upstream:8080/api/updates",
                "hosts":"http://upstream:8080/api/hosts",
                "x-acme-reports":"http://upstream:8080/api/x-acme-reports",
                "x-acme-count":3,
                "schema":{}}"#
        }
        "/components" => {
            br#"{"items":[
                {"id":"flxc1000","name":"FLXC","href":"http://cda:20002/Vehicle/v15/components/flxc1000"},
                {"id":"ecu","name":"The CDA's ecu","href":"http://cda:20002/Vehicle/v15/components/ecu"}
            ],"x-sovd2uds-can-ecus":[]}"#
        }
        // a resource list, not an entity
        "/apps" => br#"{"items":[{"name":"sovd2uds","href":"http://cda:20002/vehicle/v15/apps/sovd2uds"}]}"#,
        _ => return None,
    };
    let (status, body) = if authorization == Some("Bearer refused") {
        (401, &b"{}"[..])
    } else {
        (200, body)
    };
    Some(
        Response::builder()
            .status(status)
            .header("content-type", "application/json")
            .body(Full::new(Bytes::copy_from_slice(body)))
            .unwrap(),
    )
}

/// An upstream that records each request and answers 202 with a binary body
/// and headers the server must not touch.
#[derive(Clone, Default)]
struct Upstream {
    seen: Arc<Mutex<Vec<Seen>>>,
}

impl Upstream {
    fn service(
        &self,
    ) -> tower::util::BoxCloneSyncService<
        Request<opensovd_server::Body>,
        Response<Full<Bytes>>,
        Infallible,
    > {
        let seen = Arc::clone(&self.seen);
        tower::util::BoxCloneSyncService::new(tower::service_fn(
            move |request: Request<opensovd_server::Body>| {
                let seen = Arc::clone(&seen);
                async move {
                    let (parts, body) = request.into_parts();
                    let body = body.collect().await.unwrap().to_bytes();
                    let authorization = parts
                        .headers
                        .get("authorization")
                        .map(|v| v.to_str().unwrap().to_owned());
                    let list = upstream_list(parts.uri.path(), authorization.as_deref());
                    seen.lock().unwrap().push(Seen {
                        method: parts.method,
                        uri: parts.uri.to_string(),
                        authorization,
                        body,
                    });
                    if let Some(list) = list {
                        return Ok(list);
                    }
                    Ok::<_, Infallible>(
                        Response::builder()
                            .status(202)
                            .header("content-type", "application/octet-stream")
                            .header("location", "http://cda:20002/vehicle/v15/x/1")
                            .header("x-upstream", "cda")
                            .body(Full::new(Bytes::from_static(UPSTREAM_BODY)))
                            .unwrap(),
                    )
                }
            },
        ))
    }

    fn seen(&self) -> Vec<Seen> {
        self.seen.lock().unwrap().clone()
    }
}

#[derive(Clone)]
struct Bearer;

impl Authenticator for Bearer {
    fn authenticate(&self, parts: &Parts) -> Result<Option<Principal>, AuthError> {
        Ok(parts
            .headers
            .get("authorization")
            .filter(|h| *h == "Bearer ok")
            .map(|_| Principal {
                subject: "client".to_owned(),
                roles: Vec::new(),
                scopes: Vec::new(),
                claims: Map::new(),
            }))
    }
}

#[derive(Clone)]
struct Authenticated;

impl Authorizer for Authenticated {
    fn authorize(&self, access: &Access<'_>) -> bool {
        access.principal.is_some()
    }
}

struct TestServer {
    addr: std::net::SocketAddr,
    upstream: Upstream,
    shutdown: Option<oneshot::Sender<()>>,
}

impl Drop for TestServer {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
    }
}

async fn start(auth: Option<Auth>) -> TestServer {
    start_at("/sovd", auth).await
}

async fn start_at(base: &str, auth: Option<Auth>) -> TestServer {
    start_with(base, auth, None).await
}

/// Starts a server that forwards `routes`, or its default without them.
async fn start_with(base: &str, auth: Option<Auth>, routes: Option<&[ForwardRoute]>) -> TestServer {
    let topology = opensovd_mocks::create_mock_topology().await;
    start_topology(topology, base, auth, routes).await
}

async fn start_topology(
    topology: Topology,
    base: &str,
    auth: Option<Auth>,
    routes: Option<&[ForwardRoute]>,
) -> TestServer {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let upstream = Upstream::default();
    let service = match routes {
        Some(routes) => ServerUpstream::new(upstream.service()).routes(routes.iter().copied()),
        None => ServerUpstream::new(upstream.service()),
    };

    let mounted = Some(base).filter(|base| *base != "/");
    let federation = Federation::new(vec![service], topology.clone())
        .unwrap()
        .base(base);
    let builder = Server::builder()
        .listener(listener)
        .base_uri(base)
        .unwrap()
        .topology(topology)
        .layer(tower::util::option_layer(
            auth.map(|auth| auth.into_layer(mounted)),
        ))
        .layer(federation.into_layer())
        .shutdown(async {
            shutdown_rx.await.ok();
        });
    let server = builder.build().unwrap();
    tokio::spawn(async move { server.serve().await.unwrap() });

    TestServer {
        addr,
        upstream,
        shutdown: Some(shutdown_tx),
    }
}

struct Reply {
    status: u16,
    headers: http::HeaderMap,
    body: Bytes,
}

async fn send(
    server: &TestServer,
    method: Method,
    path: &str,
    authorization: Option<&str>,
    body: &'static [u8],
) -> Reply {
    let mut request = Request::builder()
        .method(method)
        .uri(format!("http://{}{path}", server.addr));
    if let Some(authorization) = authorization {
        request = request.header("authorization", authorization);
    }
    let request = request.body(Full::new(Bytes::from_static(body))).unwrap();
    let response = common::client().request(request).await.unwrap();
    let status = response.status().as_u16();
    let headers = response.headers().clone();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    Reply {
        status,
        headers,
        body,
    }
}

fn assert_upstream_reply(reply: &Reply) {
    assert_eq!(reply.status, 202);
    assert_eq!(
        reply.headers.get("location").unwrap(),
        "http://cda:20002/vehicle/v15/x/1"
    );
    assert_eq!(reply.headers.get("x-upstream").unwrap(), "cda");
    assert_eq!(
        reply.headers.get("content-type").unwrap(),
        "application/octet-stream"
    );
    assert_eq!(&reply.body[..], UPSTREAM_BODY);
}

#[rstest]
// entities the topology does not hold
#[case(
    Method::GET,
    "/sovd/v1/components/flxc1000/faults",
    "/components/flxc1000/faults"
)]
#[case(
    Method::DELETE,
    "/sovd/v1/components/flxc1000/faults/P0001",
    "/components/flxc1000/faults/P0001"
)]
#[case(
    Method::GET,
    "/sovd/v1/components/flxc1000/data/vin?include-schema=true",
    "/components/flxc1000/data/vin?include-schema=true"
)]
#[case(
    Method::PUT,
    "/sovd/v1/components/flxc1000/genericservice",
    "/components/flxc1000/genericservice"
)]
#[case(
    Method::PUT,
    "/sovd/v1/components/flxc1000/x-sovd2uds-download/requestdownload",
    "/components/flxc1000/x-sovd2uds-download/requestdownload"
)]
#[case(
    Method::POST,
    "/sovd/v1/components/flxc1000/locks",
    "/components/flxc1000/locks"
)]
#[case(
    Method::PUT,
    "/sovd/v1/components/flxc1000/modes/session",
    "/components/flxc1000/modes/session"
)]
#[case(Method::GET, "/sovd/v1/components/flxc1000", "/components/flxc1000")]
#[case(
    Method::GET,
    "/sovd/v1/components/flxc1000/subcomponents/x/data",
    "/components/flxc1000/subcomponents/x/data"
)]
#[case(
    Method::POST,
    "/sovd/v1/apps/sovd2uds/bulk-data/runtimefiles-nextupdate",
    "/apps/sovd2uds/bulk-data/runtimefiles-nextupdate"
)]
// functions
#[case(Method::GET, "/sovd/v1/functions", "/functions")]
#[case(
    Method::PUT,
    "/sovd/v1/functions/functionalgroups/fg/modes/session",
    "/functions/functionalgroups/fg/modes/session"
)]
// server-level authorization
#[case(Method::POST, "/sovd/v1/authorize", "/authorize")]
// routes the API does not have
#[case(Method::GET, "/sovd/v1/x-vendor", "/x-vendor")]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn forwards_what_the_server_does_not_serve(
    #[case] method: Method,
    #[case] path: &str,
    #[case] upstream_uri: &str,
) {
    let server = start(None).await;
    let reply = send(
        &server,
        method.clone(),
        path,
        Some("Bearer cda"),
        b"\x01\x02body",
    )
    .await;

    assert_upstream_reply(&reply);
    let seen = server.upstream.seen();
    assert_eq!(seen.len(), 1, "{method} {path}");
    let seen = seen.first().unwrap();
    assert_eq!(seen.method, method);
    assert_eq!(seen.uri, upstream_uri);
    assert_eq!(seen.authorization.as_deref(), Some("Bearer cda"));
    assert_eq!(&seen.body[..], b"\x01\x02body");
}

#[rstest]
#[case(Method::GET, "/sovd/v1/components/ecu", 200)]
#[case(Method::GET, "/sovd/v1/components/ecu/data", 200)]
#[case(Method::GET, "/sovd/v1/apps/engine_control", 200)]
#[case(Method::GET, "/sovd/v1/areas/powertrain", 200)]
#[case(Method::GET, "/sovd/version-info", 200)]
#[case(Method::POST, "/sovd/v1/token", 404)]
// stub routes of entities the server holds
#[case(Method::GET, "/sovd/v1/components/ecu/faults", 404)]
#[case(Method::GET, "/sovd/v1/components/ecu/locks", 404)]
#[case(Method::GET, "/sovd/v1/apps/engine_control/operations", 404)]
// paths no route serves, as nothing but custom collections is forwarded
#[case(Method::GET, "/sovd/v1/unknown", 404)]
// paths no route serves below entities the server holds
#[case(Method::GET, "/sovd/v1/components/ecu/x-single-ecu-jobs", 404)]
#[case(Method::GET, "/sovd/v1/components/ecu/data/x/docs", 404)]
#[case(Method::GET, "/sovd/v1/apps/engine_control/x-vendor", 404)]
#[case(Method::GET, "/sovd/v1/areas/powertrain/x-vendor", 404)]
// outside the versioned API
#[case(Method::GET, "/sovd/other", 404)]
#[case(Method::GET, "/other", 404)]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn serves_what_it_holds(#[case] method: Method, #[case] path: &str, #[case] status: u16) {
    let server = start(None).await;
    let reply = send(&server, method.clone(), path, None, b"").await;

    assert_eq!(reply.status, status, "{method} {path}");
    assert!(reply.headers.get("x-upstream").is_none(), "{method} {path}");
    assert!(server.upstream.seen().is_empty(), "{method} {path}");
}

fn vendor_code(json: Value) -> Option<String> {
    let error: GenericError = serde_json::from_value(json).unwrap();
    error.vendor_code
}

/// The ids and `href`s of an entity list.
fn entities(reply: &Reply) -> Vec<(String, String)> {
    let list: Entities = serde_json::from_slice(&reply.body).unwrap();
    list.items
        .into_iter()
        .map(|item| (item.id, item.href.0))
        .collect()
}

#[rstest]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn lists_hold_the_upstreams_entities_next_to_the_own() {
    let server = start(None).await;
    let host = server.addr;

    let path = "/sovd/v1/components";
    let reply = send(&server, Method::GET, path, Some("Bearer t"), b"").await;

    assert_eq!(reply.status, 200);
    assert_eq!(
        entities(&reply),
        [
            ("ecu".to_owned(), format!("http://{host}{path}/ecu")),
            ("gateway".to_owned(), format!("http://{host}{path}/gateway")),
            (
                "flxc1000".to_owned(),
                format!("http://{host}{path}/flxc1000")
            ),
        ]
    );
    let list: Entities = serde_json::from_slice(&reply.body).unwrap();
    let names: Vec<_> = list.items.iter().map(|item| item.name.as_str()).collect();
    assert_eq!(names, ["Engine Control Unit", "Vehicle Gateway", "FLXC"]);
    let seen = server.upstream.seen();
    assert_eq!(seen.len(), 1);
    let seen = seen.first().unwrap();
    assert_eq!(seen.uri, "/components");
    assert_eq!(seen.authorization.as_deref(), Some("Bearer t"));

    let reply = send(&server, Method::GET, "/sovd/v1/apps", None, b"").await;
    let ids: Vec<_> = entities(&reply).into_iter().map(|(id, _)| id).collect();
    assert!(!ids.contains(&"sovd2uds".to_owned()), "{ids:?}");
}

#[rstest]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn a_tag_filter_covers_the_upstreams_entities() {
    let server = start(None).await;
    let path = "/sovd/v1/components?tags=network";

    let reply = send(&server, Method::GET, path, None, b"").await;

    let ids: Vec<_> = entities(&reply).into_iter().map(|(id, _)| id).collect();
    assert_eq!(ids, ["gateway"]);
}

#[rstest]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn lists_stand_without_an_answer_from_the_upstream() {
    let server = start(None).await;
    let path = "/sovd/v1/components";

    let reply = send(&server, Method::GET, path, Some("Bearer refused"), b"").await;

    assert_eq!(reply.status, 200);
    let ids: Vec<_> = entities(&reply).into_iter().map(|(id, _)| id).collect();
    assert_eq!(ids, ["ecu", "gateway"]);
}

#[rstest]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn lists_hold_the_upstreams_entities_only_when_forwarding_them() {
    let server = start_with("/sovd", None, Some(&[ForwardRoute::Locks])).await;

    let reply = send(&server, Method::GET, "/sovd/v1/components", None, b"").await;

    let ids: Vec<_> = entities(&reply).into_iter().map(|(id, _)| id).collect();
    assert_eq!(ids, ["ecu", "gateway"]);
    assert!(server.upstream.seen().is_empty());
}

#[rstest]
#[case(Some(&[ForwardRoute::Entities][..]), true)]
#[case(Some(&[ForwardRoute::Locks][..]), false)]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn an_empty_server_advertises_the_entities_of_the_upstream(
    #[case] routes: Option<&[ForwardRoute]>,
    #[case] advertised: bool,
) {
    let server = start_topology(Topology::default(), "/sovd", None, routes).await;

    let reply = send(&server, Method::GET, "/sovd/v1", None, b"").await;

    let root: EntityCapabilities = serde_json::from_slice(&reply.body).unwrap();
    assert_eq!(root.components.is_some(), advertised);
    assert_eq!(root.apps.is_some(), advertised);
    assert_eq!(root.areas.is_some(), advertised);
}

/// What the topology holds is the server's, what it does not goes upstream.
#[rstest]
#[case("/sovd/v1/updates", "/updates")]
#[case("/sovd/v1/areas", "/areas")]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn the_server_wins_over_the_upstream_with_what_it_holds(
    #[case] path: &str,
    #[case] upstream_path: &str,
) {
    let holding = start(None).await;
    let reply = send(&holding, Method::GET, path, None, b"").await;
    assert_eq!(reply.status, 200);
    assert!(holding.upstream.seen().is_empty());

    let empty = start_topology(Topology::default(), "/sovd", None, None).await;
    send(&empty, Method::GET, path, None, b"").await;
    let seen = empty.upstream.seen();
    assert_eq!(seen.last().unwrap().uri, upstream_path);
}

/// The server-level collections of ISO 17978-3 Table 8 and Table 53 besides
/// the entity collections and `locks`.
const COLLECTIONS: [&str; 15] = [
    "belongs-to",
    "bulk-data",
    "communication-logs",
    "configurations",
    "cyclic-subscriptions",
    "data",
    "data-lists",
    "depends-on",
    "faults",
    "logs",
    "modes",
    "operations",
    "scripts",
    "triggers",
    "updates",
];

/// The capabilities of the server, as with the [`ForwardRoute`]s of `routes` and
/// the client's `authorization`.
async fn root(routes: Option<&[ForwardRoute]>, authorization: Option<&str>) -> Reply {
    let server = start_with("/sovd", None, routes).await;
    let reply = send(&server, Method::GET, "/sovd/v1", authorization, b"").await;
    assert_eq!(reply.status, 200);
    reply
}

/// The root of `reply` by the capabilities the model knows.
fn capabilities(reply: &Reply) -> EntityCapabilities {
    serde_json::from_slice(&reply.body).unwrap()
}

/// The root of `reply` by name, for the collections that have no field.
fn links(reply: &Reply) -> Map<String, Value> {
    serde_json::from_slice(&reply.body).unwrap()
}

#[rstest]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn the_root_adds_what_the_upstream_offers_through_the_server() {
    let server = start(None).await;
    let base = format!("http://{}/sovd/v1", server.addr);

    let path = "/sovd/v1";
    let reply = send(&server, Method::GET, path, Some("Bearer t"), b"").await;

    assert_eq!(reply.status, 200);
    let root = capabilities(&reply);
    let link = |name| Some(UriReference(format!("{base}/{name}")));
    assert_eq!(root.functions, link("functions"), "{path}");
    assert_eq!(root.locks, link("locks"), "{path}");
    assert_eq!(root.components, link("components"), "{path}");
    let links = links(&reply);
    for name in COLLECTIONS {
        assert_eq!(links[name], format!("{base}/{name}"), "{path}");
    }
    assert_eq!(
        links["x-acme-reports"],
        format!("{base}/x-acme-reports"),
        "{path}"
    );
    let seen = server.upstream.seen();
    assert_eq!(seen.len(), 1);
    let seen = seen.first().unwrap();
    assert_eq!((seen.method.as_str(), seen.uri.as_str()), ("GET", "/"));
    assert_eq!(seen.authorization.as_deref(), Some("Bearer t"));
}

/// With `updates` on both sides the answer has the one of the server, once.
#[rstest]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn the_root_holds_a_capability_of_both_once() {
    let server = start(None).await;

    let reply = send(&server, Method::GET, "/sovd/v1", None, b"").await;

    let body = std::str::from_utf8(&reply.body).unwrap();
    assert_eq!(body.matches("\"updates\"").count(), 1, "{body}");
}

#[rstest]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn the_root_keeps_the_upstreams_description_of_itself_out() {
    let reply = root(None, None).await;

    let root = capabilities(&reply);
    assert_eq!(root.id, "");
    assert_eq!(root.name, "");
    assert!(!root.areas.unwrap().0.contains("upstream:"));
    let names = links(&reply);
    for field in [
        "translation_id",
        "hosts",
        "data-categories",
        "data-groups",
        "x-acme-count",
        "schema",
    ] {
        assert!(!names.contains_key(field), "{field}");
    }
}

#[rstest]
#[case(ForwardRoute::Entities, &["apps", "areas", "components"])]
#[case(ForwardRoute::Functions, &["functions"])]
#[case(ForwardRoute::Collections, &COLLECTIONS)]
#[case(ForwardRoute::Locks, &["locks"])]
#[case(ForwardRoute::CustomCollections, &["x-acme-reports"])]
#[case(ForwardRoute::Authorize, &[])]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn the_root_adds_only_links_that_are_forwarded(
    #[case] route: ForwardRoute,
    #[case] expected: &[&str],
) {
    let server = start_topology(Topology::default(), "/sovd", None, Some(&[route])).await;

    let reply = send(&server, Method::GET, "/sovd/v1", None, b"").await;

    let links = links(&reply);
    let mut added: Vec<_> = links
        .keys()
        .filter(|field| !["id", "name"].contains(&field.as_str()))
        .map(String::as_str)
        .collect();
    let mut expected = expected.to_vec();
    expected.sort_unstable();
    added.sort_unstable();
    assert_eq!(added, expected);
}

#[rstest]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn the_root_stands_without_an_answer_from_the_upstream() {
    let reply = root(None, Some("Bearer refused")).await;

    let names = links(&reply);
    for field in ["functions", "data", "x-acme-reports"] {
        assert!(!names.contains_key(field), "{field}");
    }
    assert!(capabilities(&reply).components.is_some());
}

/// A request of each [`ForwardRoute`], and how the server answers it itself.
const ROUTE_REQUESTS: [(ForwardRoute, Method, &str, u16); 17] = [
    (
        ForwardRoute::Entities,
        Method::GET,
        "/sovd/v1/components/flxc1000/faults",
        404,
    ),
    (
        ForwardRoute::Entities,
        Method::POST,
        "/sovd/v1/apps/cda_app/locks",
        404,
    ),
    (
        ForwardRoute::Functions,
        Method::GET,
        "/sovd/v1/functions",
        404,
    ),
    (
        ForwardRoute::Functions,
        Method::GET,
        "/sovd/v1/functions/fg",
        404,
    ),
    (ForwardRoute::Collections, Method::GET, "/sovd/v1/data", 404),
    (
        ForwardRoute::Collections,
        Method::PUT,
        "/sovd/v1/data/version",
        415,
    ),
    (
        ForwardRoute::Collections,
        Method::GET,
        "/sovd/v1/faults",
        404,
    ),
    (
        ForwardRoute::Collections,
        Method::DELETE,
        "/sovd/v1/faults/P1",
        404,
    ),
    (
        ForwardRoute::Collections,
        Method::GET,
        "/sovd/v1/modes",
        404,
    ),
    (
        ForwardRoute::Collections,
        Method::GET,
        "/sovd/v1/data-categories",
        404,
    ),
    (
        ForwardRoute::Collections,
        Method::GET,
        "/sovd/v1/data-groups",
        404,
    ),
    (
        ForwardRoute::CustomCollections,
        Method::GET,
        "/sovd/v1/x-vendor",
        404,
    ),
    (ForwardRoute::Locks, Method::GET, "/sovd/v1/locks", 404),
    (ForwardRoute::Locks, Method::POST, "/sovd/v1/locks", 404),
    (ForwardRoute::Locks, Method::GET, "/sovd/v1/locks/l1", 404),
    (
        ForwardRoute::Locks,
        Method::DELETE,
        "/sovd/v1/locks/l1",
        404,
    ),
    (
        ForwardRoute::Authorize,
        Method::POST,
        "/sovd/v1/authorize",
        404,
    ),
];

#[rstest]
#[case(ForwardRoute::Entities)]
#[case(ForwardRoute::Functions)]
#[case(ForwardRoute::Collections)]
#[case(ForwardRoute::CustomCollections)]
#[case(ForwardRoute::Authorize)]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn forwards_only_the_given_routes(#[case] selected: ForwardRoute) {
    let server = start_with("/sovd", None, Some(&[selected])).await;

    for (route, method, path, status) in ROUTE_REQUESTS {
        let reply = send(&server, method.clone(), path, None, b"").await;
        if route == selected {
            assert_upstream_reply(&reply);
        } else {
            assert_eq!(reply.status, status, "{method} {path}");
            assert!(reply.headers.get("x-upstream").is_none(), "{method} {path}");
        }
    }
}

/// A custom collection of an entity goes with the entity.
#[rstest]
#[case(ForwardRoute::Entities, true)]
#[case(ForwardRoute::CustomCollections, false)]
#[case(ForwardRoute::Locks, false)]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn a_custom_collection_of_an_unknown_entity(
    #[case] selected: ForwardRoute,
    #[case] forwarded: bool,
) {
    let server = start_with("/sovd", None, Some(&[selected])).await;
    let path = "/sovd/v1/components/flxc1000/x-single-ecu-jobs";

    let reply = send(&server, Method::GET, path, None, b"").await;

    assert_eq!(!server.upstream.seen().is_empty(), forwarded);
    assert_eq!(reply.status, if forwarded { 202 } else { 404 });
}

/// What is below an entity the server holds is the server's, also for an
/// upstream that takes custom collections.
#[rstest]
#[case("/sovd/v1/components/ecu/x-single-ecu-jobs")]
#[case("/sovd/v1/apps/engine_control/x-vendor")]
#[case("/sovd/v1/areas/powertrain/x-vendor")]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn a_custom_collection_of_a_held_entity_is_not_forwarded(#[case] path: &str) {
    let server = start_with("/sovd", None, Some(&[ForwardRoute::CustomCollections])).await;

    let reply = send(&server, Method::GET, path, None, b"").await;

    assert_eq!(reply.status, 404, "{path}");
    assert!(server.upstream.seen().is_empty(), "{path}");
}

#[rstest]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn forwards_nothing_without_routes() {
    let server = start_with("/sovd", None, Some(&[])).await;

    for (_, method, path, status) in ROUTE_REQUESTS {
        let reply = send(&server, method.clone(), path, None, b"").await;
        assert_eq!(reply.status, status, "{method} {path}");
    }
    assert!(server.upstream.seen().is_empty());
}

#[rstest]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn auth_runs_before_forwarding() {
    let server = start(Some(Auth::new(Bearer, Authenticated))).await;

    let reply = send(
        &server,
        Method::GET,
        "/sovd/v1/components/flxc1000/faults",
        None,
        b"",
    )
    .await;
    assert_eq!(reply.status, 401);
    let reply = send(&server, Method::POST, "/sovd/v1/locks", None, b"").await;
    assert_eq!(reply.status, 401);
    let reply = send(&server, Method::GET, "/sovd/v1/x-vendor", None, b"").await;
    assert_eq!(reply.status, 401);
    assert!(server.upstream.seen().is_empty());

    let reply = send(
        &server,
        Method::GET,
        "/sovd/v1/components/flxc1000/faults",
        Some("Bearer ok"),
        b"",
    )
    .await;
    assert_upstream_reply(&reply);
    assert_eq!(
        server
            .upstream
            .seen()
            .first()
            .unwrap()
            .authorization
            .as_deref(),
        Some("Bearer ok")
    );
}

#[rstest]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn without_forwarding_the_server_answers_itself() {
    let server = common::mock_server().await;

    let (status, json) =
        common::send(&server, Method::GET, "/sovd/v1/components/flxc1000/faults").await;
    assert_eq!(status, 404);
    assert_eq!(vendor_code(json).as_deref(), Some("entity-not-found"));

    for (method, path) in [
        (Method::GET, "/sovd/v1/locks"),
        (Method::POST, "/sovd/v1/locks"),
        (Method::GET, "/sovd/v1/locks/l1"),
        (Method::PUT, "/sovd/v1/locks/l1"),
        (Method::DELETE, "/sovd/v1/locks/l1"),
        (Method::POST, "/sovd/v1/authorize"),
        (Method::POST, "/sovd/v1/token"),
    ] {
        let (status, json) = common::send(&server, method.clone(), path).await;
        assert_eq!(status, 404, "{method} {path}");
        let code = vendor_code(json);
        assert_eq!(
            code.as_deref(),
            Some("resource-not-found"),
            "{method} {path}"
        );
    }

    let (status, _) = common::send(&server, Method::GET, "/sovd/v1").await;
    assert_eq!(status, 200);

    for (method, path) in [
        (Method::PUT, "/sovd/v1/locks"),
        (Method::GET, "/sovd/v1/authorize"),
    ] {
        let (status, _) = common::send(&server, method.clone(), path).await;
        assert_eq!(status, 405, "{method} {path}");
    }
}

#[rstest]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn forwards_with_the_api_at_the_root() {
    let server = start_at("/", None).await;

    let reply = send(&server, Method::GET, "/v1/x-vendor?a=b", None, b"").await;
    assert_upstream_reply(&reply);
    let reply = send(
        &server,
        Method::GET,
        "/v1/components/flxc1000/faults",
        None,
        b"",
    )
    .await;
    assert_upstream_reply(&reply);
    let uris: Vec<_> = server.upstream.seen().into_iter().map(|s| s.uri).collect();
    assert_eq!(uris, ["/x-vendor?a=b", "/components/flxc1000/faults"]);

    let reply = send(&server, Method::GET, "/other", None, b"").await;
    assert_eq!(reply.status, 404);
    let reply = send(&server, Method::GET, "/v1/components/ecu", None, b"").await;
    assert_eq!(reply.status, 200);
    assert_eq!(server.upstream.seen().len(), 2);
}

/// The routes serve no path with a trailing slash, with or without an upstream.
#[rstest]
#[case("/sovd/v1/")]
#[case("/sovd/v1/components/")]
#[case("/sovd/v1/components/ecu/")]
#[case("/sovd/v1/components/ecu/data/")]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn a_trailing_slash_is_not_forwarded(#[case] path: &str) {
    let server = start(None).await;

    let reply = send(&server, Method::GET, path, None, b"").await;

    assert_eq!(reply.status, 404, "{path}");
    assert!(server.upstream.seen().is_empty(), "{path}");
}

#[rstest]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn an_entity_of_the_upstream_is_reachable_with_a_trailing_slash() {
    let server = start(None).await;

    let path = "/sovd/v1/components/flxc1000/faults/";
    let reply = send(&server, Method::GET, path, None, b"").await;

    assert_upstream_reply(&reply);
}

/// What is on a path the server routes is the server's, whatever the method.
#[rstest]
#[timeout(std::time::Duration::from_secs(10))]
#[tokio::test]
async fn a_method_the_server_does_not_serve_is_not_forwarded() {
    let server = start(None).await;

    let reply = send(
        &server,
        Method::PUT,
        "/sovd/v1/components/ecu",
        None,
        b"body",
    )
    .await;

    assert_eq!(reply.status, 405);
    assert!(server.upstream.seen().is_empty());
}
