// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Several SOVD servers behind one: a layer that serves what the server does
//! not hold from upstream services, such as classic diagnostic adapters. The
//! server does not know about it. The layer belongs inside auth, so that
//! forwarded requests are authorized like any other: add it after
//! [`Auth::into_layer`](opensovd_server::Auth::into_layer).
//!
//! The server still answers what it serves itself: entities in the topology,
//! on every path below them, including the stub routes. The [`ForwardRoute`]s
//! of an upstream are forwarded, by default all of them; what is not forwarded is
//! answered as without an upstream.
//!
//! Upstreams share [`ForwardRoute::Entities`], where the upstream that has the
//! entity answers, and [`ForwardRoute::Locks`], where a lock holds on all of
//! them. Every other route has one upstream, as there is one
//! answer. Functional locks stay with the upstream of the function.
//!
//! The upstream receives the request with its URI relative to the API version,
//! such as `/components/ecu/faults?status=x`, and with the method, headers and
//! body unchanged, and with the [`ServerUri`] extension. The response is what
//! the upstream service answers, which is not verbatim what a remote server
//! sent: a proxy such as the gateway's replaces the server's references to
//! itself with the [`ServerUri`].
//!
//! The entities of the upstream are part of the server's discovery: `/`
//! advertises `/components` and `/apps`, and these lists hold the upstream's
//! entities next to the server's own. Without a working upstream they hold the
//! server's own only.
//!
//! So are the other resources of the upstream that clients reach through the
//! server: `/` adds the links of the upstream's own `/` for the routes that
//! are forwarded, and keeps what the server advertises itself. What is left of
//! that answer describes the upstream. The combining stops at these links: a
//! collection has one answer, not the items of both.

use std::collections::{BTreeSet, HashMap};
use std::convert::Infallible;
use std::error::Error as StdError;
use std::mem;
use std::sync::{Arc, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::task::{Context, Poll};
use std::time::Duration;

use axum::extract::OriginalUri;
use axum::response::IntoResponse;
use bytes::Bytes;
use futures::future::{BoxFuture, join_all};
use http::header::{AUTHORIZATION, CONTENT_LENGTH, HOST, HeaderMap};
use http::request::Parts;
use http::{Extensions, Method, StatusCode, Uri};
use http_body_util::{BodyExt, Empty, Full, Limited};
use opensovd_models::Collection;
use opensovd_models::discovery::{Entities, EntityReference};
use opensovd_models::error::GenericError;
use opensovd_server::{Body, Topology, encode_path_segment};
use percent_encoding::percent_decode_str;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};
use tower::util::BoxCloneSyncService;
use tower::{Layer, Service as TowerService, ServiceExt};

use self::locks::VehicleLocks;

mod locks;

/// The log target of forwarding.
const TARGET: &str = "forwarding";

/// The API version below the server's mount path.
const API_VERSION: &str = "v1";

/// A request to an upstream.
type Request = http::Request<Body>;

/// The answer of an upstream.
type Response = http::Response<Body>;

/// A request as the server's router sees it.
type AxumRequest = http::Request<axum::body::Body>;

/// An answer as the server's router sees it.
type AxumResponse = http::Response<axum::body::Body>;

/// A request of the API as the server's client made it.
#[derive(Clone, Debug)]
struct ApiRequest {
    /// The URI relative to the API version, with the query, such as
    /// `/components/ecu/faults?status=x`.
    uri: Uri,
    /// The server's API as its client reaches it, such as
    /// `http://localhost:7690/sovd/v1`.
    server_uri: String,
}

/// What an [`Upstream`] sends its requests to, which hides whether that is a
/// proxy to a remote server or a service in this process.
type Service = BoxCloneSyncService<Request, Response, Infallible>;

/// How long an upstream gets to answer a discovery request by default.
const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(5);

/// The largest discovery response accepted from the upstream.
const DISCOVERY_LIMIT: usize = 16 * 1024 * 1024;

/// Prefix of the custom resource collections, ISO 17978-3 5.4.5.
const CUSTOM_PREFIX: &str = "x-";

/// The collection of components.
const COMPONENTS: &str = "components";

/// The collection of apps.
const APPS: &str = "apps";

/// The collection of areas.
const AREAS: &str = "areas";

/// The collection of functions.
const FUNCTIONS: &str = "functions";

/// The collection of vehicle locks.
const LOCKS: &str = "locks";

/// The collection of updates.
const UPDATES: &str = "updates";

/// The resources of the `SOVDServer` that are not plain
/// [`COLLECTIONS`], and how each is handled. This is the one place that names
/// them: [`classify`] and what `/` advertises follow from it.
const RESOURCES: [(&str, Rule); 6] = [
    (COMPONENTS, Rule::Entities),
    (APPS, Rule::Entities),
    (AREAS, Rule::Areas),
    (FUNCTIONS, Rule::Route(ForwardRoute::Functions)),
    (LOCKS, Rule::Route(ForwardRoute::Locks)),
    (UPDATES, Rule::Updates),
];

/// Resource collections of the `SOVDServer` (ISO 17978-3 Table 8 and Table 53)
/// that go upstream as [`ForwardRoute::Collections`]. The resources that are
/// handled differently are in [`RESOURCES`].
///
/// The list tells these names apart from other keys:
/// - In the upstream's `/`, only links with these names are copied into the
///   server's `/`. Keys that describe the upstream itself, such as `id` or
///   `schema`, are dropped.
/// - A request starting with one of these names, such as `GET /faults`, goes
///   upstream.
///
/// A name appears at `/` only if the upstream advertises it.
const COLLECTIONS: [&str; 14] = [
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
];

/// Resources of the `SOVDServer` in ISO 17978-3 Table 10 that Table 53 has no
/// attribute for: [`ForwardRoute::Collections`], but not advertised at `/`.
const DATA_RESOURCES: [&str; 2] = ["data-categories", "data-groups"];

/// The server's API as its client reaches it, such as
/// `http://gateway:7690/sovd/v1`: an extension of forwarded requests, for an
/// upstream proxy to point links in responses at the server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerUri(pub String);

/// Requests the server can forward instead of answering them itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ForwardRoute {
    // Only `Entities` and `Locks` can have several upstreams; see `Federation::new`.
    /// Requests below `/components/{id}` and `/apps/{id}` with an `id` the
    /// topology does not hold, and below `/areas` if it holds no area. The
    /// upstream's entities are also listed by `/components` and `/apps`.
    Entities,
    /// Requests below `/functions`, as the topology holds no functions.
    /// Without an upstream these requests answer as the stubs do.
    Functions,
    /// Requests below the resource collections of the server, such as
    /// `/data` and `/data-categories`, as the server holds none but
    /// `/updates`, which goes upstream without an update provider in the
    /// topology. Without an upstream the routes answer.
    Collections,
    /// Requests below the custom resource collections of the server, whose
    /// names start with `x-` (ISO 17978-3 5.4.5), such as `/x-acme-reports`.
    /// Those of an entity go with the entity.
    CustomCollections,
    /// The server-level `/locks` and `/locks/{lock_id}`.
    ///
    /// A lock here applies to all entities, so upstreams share this route and
    /// a lock holds on all of them; see [`Upstream::routes`]. Without an upstream
    /// the stubs answer. The locks of entities go with their entity.
    Locks,
    /// The server-level `/authorize`. Without an upstream the stub answers.
    Authorize,
}

impl ForwardRoute {
    /// Every route, what the server forwards by default.
    pub const ALL: [Self; 6] = [
        Self::Entities,
        Self::Functions,
        Self::Collections,
        Self::CustomCollections,
        Self::Locks,
        Self::Authorize,
    ];
}

/// A server that takes requests the server does not serve itself, by default
/// all [`ForwardRoute`]s. Given to [`Federation::new`], which takes several.
#[derive(Clone)]
pub struct Upstream {
    service: Service,
    routes: Arc<[ForwardRoute]>,
    /// How long this upstream gets to answer a discovery request.
    discovery_timeout: Duration,
}

impl Upstream {
    /// Forwards to `svc`, such as a proxy to a classic diagnostic adapter.
    pub fn new<S, ResBody>(svc: S) -> Self
    where
        S: TowerService<Request, Response = http::Response<ResBody>, Error = Infallible>
            + Clone
            + Send
            + Sync
            + 'static,
        S::Future: Send + 'static,
        ResBody: http_body::Body<Data = Bytes> + Send + 'static,
        ResBody::Error: Into<Box<dyn StdError + Send + Sync>>,
    {
        let svc = svc.map_response(|response: http::Response<ResBody>| response.map(Body::new));
        Self {
            service: BoxCloneSyncService::new(svc),
            routes: ForwardRoute::ALL.into(),
            discovery_timeout: DISCOVERY_TIMEOUT,
        }
    }

    /// Forwards only `routes` to this upstream. The server answers the others
    /// as it does without an upstream, unless another upstream takes them.
    #[must_use]
    pub fn routes(mut self, routes: impl IntoIterator<Item = ForwardRoute>) -> Self {
        self.routes = routes.into_iter().collect();
        self
    }

    /// Gives this upstream `timeout` to answer what the server asks to find
    /// out what it has, by default 5 seconds: a request for a list or the root,
    /// and the look for the owner of an entity. A list or the root without its
    /// answer holds the rest, and an owner that answers in time still answers,
    /// so an upstream that hangs does not hang the others. Forwarded requests
    /// have no timeout, as flashing and operations may run long.
    #[must_use]
    pub fn discovery_timeout(mut self, timeout: Duration) -> Self {
        self.discovery_timeout = timeout;
        self
    }

    /// Whether the requests of `route` go to this upstream.
    fn includes(&self, route: ForwardRoute) -> bool {
        self.routes.contains(&route)
    }

    /// Sends `request` upstream with its URI relative to the API version, and
    /// returns what the upstream service answers.
    async fn forward(&self, request: Request) -> Response {
        let uri = api_uri(request.extensions());
        let (parts, body) = request.into_parts();
        self.send_to(parts, body, uri).await
    }

    /// Sends a request of `parts` and `body` to `uri`, relative to the API
    /// version of the upstream, and returns what the upstream service answers.
    async fn send_to(&self, mut parts: Parts, body: Body, uri: Uri) -> Response {
        let server = parts.extensions.get::<ApiRequest>().cloned();
        if let Some(api) = server {
            parts.extensions.insert(ServerUri(api.server_uri));
        }
        parts.uri = uri;
        let request = Request::from_parts(parts, body);
        match self.service.clone().oneshot(request).await {
            Ok(response) => response,
            Err(never) => match never {},
        }
    }

    /// Sends the server's own request of `method` to `uri`, for a request of
    /// the client of `parts`. It has no body, and of the client's headers only
    /// the token, so that the upstream decides what the client may do.
    async fn request_with_client_token(&self, parts: &Parts, method: Method, uri: Uri) -> Response {
        let mut own = parts.clone();
        own.method = method;
        own.headers = HeaderMap::new();
        if let Some(token) = parts.headers.get(AUTHORIZATION) {
            own.headers.insert(AUTHORIZATION, token.clone());
        }
        self.send_to(own, empty_body(), uri).await
    }

    /// The upstream's answer to a look at the entity `uri` for the client of
    /// `parts`, or a 504 if it does not answer in time.
    async fn look_at(&self, parts: &Parts, uri: Uri) -> Response {
        let look = self.request_with_client_token(parts, Method::GET, uri);
        tokio::time::timeout(self.discovery_timeout, look)
            .await
            .unwrap_or_else(|_| {
                tracing::warn!(target: TARGET, "Upstream did not answer a look in time");
                let message = "The upstream did not answer in time";
                vendor_error(StatusCode::GATEWAY_TIMEOUT, "upstream-timeout", message)
            })
    }

    /// The upstream's answer to the discovery request of `parts`, or nothing
    /// if the upstream has none to give: the server's own answer stands on its
    /// own, whatever state the upstream is in.
    async fn discovery<T: DeserializeOwned>(&self, parts: &Parts) -> Option<T> {
        let answer = tokio::time::timeout(self.discovery_timeout, self.discover(parts)).await;
        answer.unwrap_or_else(|_| {
            tracing::warn!(target: TARGET, "Upstream did not answer a discovery request in time");
            None
        })
    }

    /// The upstream's answer to the discovery request of `parts`, however long
    /// it takes.
    async fn discover<T: DeserializeOwned>(&self, parts: &Parts) -> Option<T> {
        // The client's request, without the body: the upstream answers it for this
        // client, with its token, its `Accept` and its query.
        let request = Request::from_parts(parts.clone(), empty_body());
        let response = self.forward(request).await;
        let status = response.status();
        if !status.is_success() {
            tracing::warn!(target: TARGET, %status, "Upstream did not answer a discovery request");
            return None;
        }
        let answer = read_body(response.into_body())
            .await
            .map_err(|error| error.to_string())
            .and_then(|body| serde_json::from_slice(&body).map_err(|error| error.to_string()));
        match answer {
            Ok(answer) => Some(answer),
            Err(error) => {
                tracing::warn!(target: TARGET, %error, "Upstream sent an invalid discovery answer");
                None
            }
        }
    }

    /// Whether a client that follows the upstream's link to the resource
    /// collection `name` of its `/` reaches the upstream.
    fn forwards_capability(&self, name: &str) -> bool {
        let rule = RESOURCES.iter().find(|(resource, _)| *resource == name);
        match rule {
            Some((_, rule)) => rule.route().is_some_and(|route| self.includes(route)),
            None if COLLECTIONS.contains(&name) => self.includes(ForwardRoute::Collections),
            None => {
                name.starts_with(CUSTOM_PREFIX) && self.includes(ForwardRoute::CustomCollections)
            }
        }
    }

    /// The names of what the upstream advertises to the request of `parts`
    /// for the capabilities of the server, and a client reaches through the
    /// server.
    async fn capabilities(&self, parts: &Parts) -> Vec<String> {
        // `/authorize` is no capability, so nothing else is worth the request
        if self.routes.iter().all(|r| *r == ForwardRoute::Authorize) {
            return Vec::new();
        }
        self.discovery::<Map<String, Value>>(parts)
            .await
            .unwrap_or_default()
            .into_iter()
            .filter(|(name, link)| link.is_string() && self.forwards_capability(name))
            .map(|(name, _)| name)
            .collect()
    }
}

/// Serves what the server does not hold from upstreams, in the order they are
/// given, which decides between equal entity ids.
pub struct Federation {
    upstreams: Vec<Upstream>,
    /// What the server holds, which stays with the server.
    topology: Topology,
    /// The scheme the server is reached by.
    scheme: String,
    /// The path the server is mounted at, empty at the root.
    base: String,
    /// The upstream that answered for an entity, by its path, such as
    /// `components/ecu`. It saves asking again, and is no source of truth: an
    /// entry that turns out wrong is dropped.
    owners: RwLock<HashMap<String, Upstream>>,
    /// The locks of the server.
    vehicle_locks: VehicleLocks,
}

/// Where a request goes.
enum Destination {
    /// To the routes of the server.
    Routes,
    /// To an upstream.
    Upstream(Upstream),
    /// To the upstream remembered as the owner of the entity with this key.
    Owner(Upstream, String),
    /// Nowhere, this is the answer.
    Answer(Response),
    /// To the vehicle locks.
    Locks,
}

/// More than one upstream for a route that has one answer.
#[derive(Debug, thiserror::Error)]
#[error("{0:?} can have one upstream only")]
pub struct SharedForwardRoute(pub ForwardRoute);

impl Federation {
    /// A federation of `upstreams` next to a server that holds `topology`.
    ///
    /// Every route but [`ForwardRoute::Entities`] and [`ForwardRoute::Locks`],
    /// which upstreams share, has one upstream, as there is one answer.
    ///
    /// # Errors
    ///
    /// Returns the route that has more.
    pub fn new(upstreams: Vec<Upstream>, topology: Topology) -> Result<Self, SharedForwardRoute> {
        let contested = ForwardRoute::ALL.into_iter().find(|route| {
            !matches!(route, ForwardRoute::Entities | ForwardRoute::Locks)
                && upstreams.iter().filter(|u| u.includes(*route)).count() > 1
        });
        match contested {
            Some(route) => Err(SharedForwardRoute(route)),
            None => Ok(Self {
                upstreams,
                topology,
                scheme: "http".to_owned(),
                base: String::new(),
                owners: RwLock::default(),
                vehicle_locks: VehicleLocks::default(),
            }),
        }
    }

    /// The server is reached by `scheme`, by default `http`.
    #[must_use]
    pub fn scheme(mut self, scheme: &str) -> Self {
        scheme.clone_into(&mut self.scheme);
        self
    }

    /// The server is mounted at `base`, by default at the root.
    #[must_use]
    pub fn base(mut self, base: &str) -> Self {
        base.trim_end_matches('/').clone_into(&mut self.base);
        self
    }

    /// The federation as a layer around the routes of the server.
    #[must_use]
    pub fn into_layer(self) -> FederationLayer {
        FederationLayer(Arc::new(self))
    }

    /// Whether the requests of `route` go upstream.
    fn includes(&self, route: ForwardRoute) -> bool {
        self.owner(route).is_some()
    }

    /// The first upstream of `route`, the only one but for entities.
    fn owner(&self, route: ForwardRoute) -> Option<&Upstream> {
        self.upstreams.iter().find(|u| u.includes(route))
    }

    /// All upstreams, in the order that decides between equal entity ids.
    fn upstreams(&self) -> impl Iterator<Item = &Upstream> {
        self.upstreams.iter()
    }

    fn entity_upstreams(&self) -> impl Iterator<Item = &Upstream> {
        self.upstreams()
            .filter(|upstream| upstream.includes(ForwardRoute::Entities))
    }

    fn lock_upstreams(&self) -> impl Iterator<Item = &Upstream> {
        self.upstreams()
            .filter(|upstream| upstream.includes(ForwardRoute::Locks))
    }

    fn owners(&self) -> RwLockReadGuard<'_, HashMap<String, Upstream>> {
        self.owners.read().unwrap_or_else(PoisonError::into_inner)
    }

    fn owners_mut(&self) -> RwLockWriteGuard<'_, HashMap<String, Upstream>> {
        self.owners.write().unwrap_or_else(PoisonError::into_inner)
    }

    /// Where a request goes that names an entity the server does not hold,
    /// `id` as it stands in the path, for the client of `parts`.
    ///
    /// A single upstream takes it without asking. Of several, entities come
    /// and go at runtime, and only a look tells whose it is, one that is safe
    /// for any request. The first upstream in order that has the entity owns
    /// it. An upstream that does not let the client look may have it, and
    /// answers for itself. Without an owner, one that failed may have been
    /// the owner, so its failure is the answer rather than the server's 404.
    async fn find_owner(&self, parts: &Parts, collection: &str, id: &str) -> Destination {
        let candidates: Vec<_> = self.entity_upstreams().cloned().collect();
        if let [only] = candidates.as_slice() {
            return Destination::Upstream(only.clone());
        }
        let key = format!("{collection}/{id}");
        if let Some(owner) = self.owners().get(&key).cloned() {
            return Destination::Owner(owner, key);
        }
        let Ok(uri) = format!("/{key}").parse::<Uri>() else {
            return Destination::Routes;
        };
        let looks = candidates.iter().map(|c| c.look_at(parts, uri.clone()));
        let answers = join_all(looks).await;

        let mut guarded = None;
        let mut failed = None;
        for (upstream, answer) in candidates.into_iter().zip(answers) {
            let status = answer.status();
            if status.is_success() {
                self.owners_mut().insert(key, upstream.clone());
                return Destination::Upstream(upstream);
            }
            if status.is_server_error() {
                failed.get_or_insert(answer);
            } else if status != StatusCode::NOT_FOUND {
                guarded.get_or_insert(upstream);
            }
        }
        match (guarded, failed) {
            (Some(upstream), _) => Destination::Upstream(upstream),
            (None, Some(failure)) => Destination::Answer(failure),
            (None, None) => Destination::Routes,
        }
    }

    /// Where the request of `parts` for `path`, a path relative to the API
    /// version, goes. Paths [`classify`] does not know, and ids that do not decode,
    /// are left to the routes.
    async fn destination(&self, parts: &Parts, path: &str) -> Destination {
        let Some((rule, raw_id)) = classify(path) else {
            return Destination::Routes;
        };
        let topology = &self.topology;
        let entities = self.entity_upstreams().next().is_some();
        let to_owner = |route| {
            self.owner(route)
                .map_or(Destination::Routes, |u| Destination::Upstream(u.clone()))
        };
        match rule {
            Rule::Route(ForwardRoute::Locks) => {
                if self.lock_upstreams().next().is_some() {
                    Destination::Locks
                } else {
                    Destination::Routes
                }
            }
            Rule::Route(route) => to_owner(route),
            Rule::Areas if entities && topology.read().await.areas().len() == 0 => {
                to_owner(ForwardRoute::Entities)
            }
            Rule::Updates if topology.read().await.get_updates().is_none() => {
                to_owner(ForwardRoute::Collections)
            }
            Rule::Entity(collection) if entities => {
                let raw_id = raw_id.unwrap_or_default();
                let Ok(id) = percent_decode_str(raw_id).decode_utf8() else {
                    return Destination::Routes;
                };
                let held = {
                    let topology = topology.read().await;
                    if collection == COMPONENTS {
                        topology.get_component(&id).is_ok()
                    } else {
                        topology.get_app(&id).is_ok()
                    }
                };
                if held {
                    return Destination::Routes;
                }
                self.find_owner(parts, collection, raw_id).await
            }
            Rule::Root | Rule::Areas | Rule::Updates | Rule::Entities | Rule::Entity(_) => {
                Destination::Routes
            }
        }
    }

    /// The answer to `request`, which is outside the API or goes through
    /// [`Self::route`].
    async fn answer<S>(&self, mut request: Request, routes: Routes<S>) -> Response
    where
        S: TowerService<AxumRequest, Response = AxumResponse, Error = Infallible>,
    {
        let Some(api) = self.api_request(&request) else {
            return routes.run(request).await;
        };
        request.extensions_mut().insert(api);
        self.route(request, routes).await
    }

    /// The API request of `request`, with the mount path and the API version
    /// stripped from its path, so `/sovd/v1/components` becomes `/components` and
    /// `/sovd/v1` becomes `/`. `None` for a path outside the API.
    fn api_request(&self, request: &Request) -> Option<ApiRequest> {
        let original = request
            .extensions()
            .get::<OriginalUri>()
            .map_or(request.uri(), |OriginalUri(uri)| uri);
        let path = original
            .path()
            .strip_prefix(self.base.as_str())?
            .strip_prefix('/')?
            .strip_prefix(API_VERSION)
            .filter(|path| path.is_empty() || path.starts_with('/'))?;
        let path = if path.is_empty() { "/" } else { path };
        let uri = match original.query() {
            Some(query) => format!("{path}?{query}"),
            None => path.to_owned(),
        };
        let host = request
            .headers()
            .get(HOST)
            .and_then(|host| host.to_str().ok())
            .unwrap_or("localhost");
        Some(ApiRequest {
            uri: uri.parse().ok()?,
            server_uri: format!("{}://{host}{}/{API_VERSION}", self.scheme, self.base),
        })
    }

    /// The answer to a request the routes may serve.
    async fn route<S>(&self, request: Request, routes: Routes<S>) -> Response
    where
        S: TowerService<AxumRequest, Response = AxumResponse, Error = Infallible>,
    {
        let uri = api_uri(request.extensions());
        let path = uri.path();
        let (parts, body) = request.into_parts();
        let destination = self.destination(&parts, path).await;
        let request = Request::from_parts(parts, body);
        match destination {
            Destination::Routes => self.combined(request, routes, path).await,
            Destination::Upstream(upstream) => upstream.forward(request).await,
            Destination::Owner(owner, key) => {
                let response = owner.forward(request).await;
                if response.status() == StatusCode::NOT_FOUND {
                    self.owners_mut().remove(&key);
                }
                response
            }
            Destination::Answer(response) => response,
            Destination::Locks => locks::answer(self, request, routes).await,
        }
    }

    /// The answer of the routes to `request`, with what the upstreams add to
    /// the lists of entities and to the API root.
    async fn combined<S>(&self, request: Request, routes: Routes<S>, path: &str) -> Response
    where
        S: TowerService<AxumRequest, Response = AxumResponse, Error = Infallible>,
    {
        if request.method() != Method::GET {
            return routes.run(request).await;
        }
        let (parts, body) = request.into_parts();
        let answer = routes.run(Request::from_parts(parts.clone(), body)).await;
        if answer.status() != StatusCode::OK {
            return answer;
        }
        match classify(path) {
            Some((Rule::Root, _)) => self.with_capabilities(answer, &parts).await,
            Some((Rule::Entities, _)) => {
                let collection = path.trim_start_matches('/');
                self.with_entities(answer, &parts, collection).await
            }
            _ => answer,
        }
    }

    /// `answer`, a list of `collection`, with the entities the upstreams add to
    /// it. The server's own entity wins when both have an id, as requests for it
    /// are answered by the server.
    async fn with_entities(&self, answer: Response, parts: &Parts, collection: &str) -> Response {
        let entities = self.upstream_entities(parts).await;
        if entities.is_empty() {
            return answer;
        }
        let tags = tags(&api_uri(&parts.extensions));
        let base = server_uri(&parts.extensions);
        with_json(answer, |list: &mut Entities| {
            let mut held: BTreeSet<_> = list.items.iter().map(|item| item.id.clone()).collect();
            for mut entity in entities {
                let matches = tags.is_empty()
                    || entity
                        .tags
                        .as_ref()
                        .is_some_and(|own| tags.iter().any(|tag| own.contains(tag)));
                if !matches || !held.insert(entity.id.clone()) {
                    continue;
                }
                let segment = encode_path_segment(&entity.id);
                entity.href = format!("{base}/{collection}/{segment}").into();
                list.items.push(entity);
            }
        })
        .await
    }

    /// `answer`, the API root, with links to what the upstreams offer next to
    /// the server's own capabilities, which win where both have one. They lead
    /// to the server, as forwarded requests go through its auth.
    async fn with_capabilities(&self, answer: Response, parts: &Parts) -> Response {
        let names = self.upstream_capabilities(parts).await;
        if names.is_empty() {
            return answer;
        }
        let base = server_uri(&parts.extensions);
        with_json(answer, |root: &mut Map<String, Value>| {
            for name in names {
                let segment = encode_path_segment(&name);
                let link = format!("{base}/{segment}");
                root.entry(name).or_insert(link.into());
            }
        })
        .await
    }

    /// The entities the upstreams add to the list request of `parts`, in their
    /// order, which decides between equal ids.
    async fn upstream_entities(&self, parts: &Parts) -> Vec<EntityReference> {
        let lists = self
            .entity_upstreams()
            .map(|upstream| upstream.discovery::<Collection<Value>>(parts));
        join_all(lists)
            .await
            .into_iter()
            .flatten()
            .flat_map(|list| list.items)
            // Not all items are entities: the `sovd2uds` resource list in the
            // `/apps` of a CDA has no id.
            .filter_map(|item| serde_json::from_value(item).ok())
            .collect()
    }

    /// The names of what the upstreams advertise to the request of `parts` for
    /// the capabilities of the server, and a client reaches through the server.
    /// The entities of upstreams are only known by asking, so `/components`
    /// and `/apps` are always among them, and `/locks` is the federation's own.
    async fn upstream_capabilities(&self, parts: &Parts) -> Vec<String> {
        let capabilities = self
            .upstreams()
            .map(|upstream| upstream.capabilities(parts));
        let mut names: Vec<_> = join_all(capabilities).await.into_iter().flatten().collect();
        for (name, rule) in RESOURCES {
            let own = matches!(rule, Rule::Entities | Rule::Route(ForwardRoute::Locks));
            if own && rule.route().is_some_and(|route| self.includes(route)) {
                names.push(name.to_owned());
            }
        }
        names
    }
}

/// What a path relative to the API version is, for [`Federation::destination`].
#[derive(Clone, Copy)]
enum Rule {
    /// The API root, which the routes answer and the upstreams add links to.
    Root,
    /// All of it goes to the upstream of the route.
    Route(ForwardRoute),
    /// [`ForwardRoute::Entities`], unless the server holds areas.
    Areas,
    /// [`ForwardRoute::Collections`], unless the server holds updates.
    Updates,
    /// A list of entities, which the routes answer and the upstreams add to.
    Entities,
    /// [`ForwardRoute::Entities`]: below an entity of a collection, such as
    /// `components`, unless the server holds it.
    Entity(&'static str),
}

impl Rule {
    /// The route a client of this rule reaches an upstream by, none for the
    /// root, which stays with the server.
    fn route(self) -> Option<ForwardRoute> {
        match self {
            Self::Root => None,
            Self::Route(route) => Some(route),
            Self::Areas | Self::Entities | Self::Entity(_) => Some(ForwardRoute::Entities),
            Self::Updates => Some(ForwardRoute::Collections),
        }
    }
}

/// The [`Rule`] that decides where `path` goes, by its first segment. `path` is
/// relative to the API version, such as `/components/ecu/data`.
///
/// Below an entity this is [`Rule::Entity`], with the id still percent-encoded
/// as in the path. `None` for a path no rule covers, such as `/unknown`, which
/// the routes answer.
fn classify(path: &str) -> Option<(Rule, Option<&str>)> {
    let path = path.strip_prefix('/')?;
    if path.is_empty() {
        return Some((Rule::Root, None));
    }
    let (name, below) = match path.split_once('/') {
        Some((name, below)) => (name, Some(below)),
        None => (path, None),
    };
    if let Some(&(name, rule)) = RESOURCES.iter().find(|(known, _)| *known == name) {
        return match (rule, below) {
            (Rule::Entities, Some(below)) => {
                let id = below.split('/').next().filter(|id| !id.is_empty())?;
                Some((Rule::Entity(name), Some(id)))
            }
            (Rule::Route(ForwardRoute::Locks), Some(id)) if id.is_empty() || id.contains('/') => {
                None
            }
            _ => Some((rule, None)),
        };
    }
    if COLLECTIONS.contains(&name) || DATA_RESOURCES.contains(&name) {
        return Some((Rule::Route(ForwardRoute::Collections), None));
    }
    if name.starts_with(CUSTOM_PREFIX) {
        return Some((Rule::Route(ForwardRoute::CustomCollections), None));
    }
    (name == "authorize" && below.is_none()).then_some((Rule::Route(ForwardRoute::Authorize), None))
}

/// [`Federation`] as a layer.
#[derive(Clone)]
pub struct FederationLayer(Arc<Federation>);

impl<S> Layer<S> for FederationLayer {
    type Service = FederationService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        FederationService {
            federation: Arc::clone(&self.0),
            inner,
        }
    }
}

/// The service of a [`FederationLayer`].
#[derive(Clone)]
pub struct FederationService<S> {
    federation: Arc<Federation>,
    inner: S,
}

impl<S> TowerService<AxumRequest> for FederationService<S>
where
    S: TowerService<AxumRequest, Response = AxumResponse, Error = Infallible>
        + Clone
        + Send
        + 'static,
    S::Future: Send + 'static,
{
    type Response = AxumResponse;
    type Error = Infallible;
    type Future = BoxFuture<'static, Result<AxumResponse, Infallible>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Infallible>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: AxumRequest) -> Self::Future {
        let federation = Arc::clone(&self.federation);
        // the clone may not be ready, the one that was polled is
        let inner = self.inner.clone();
        let routes = Routes(mem::replace(&mut self.inner, inner));
        Box::pin(async move {
            let request = request.map(Body::new);
            Ok(federation
                .answer(request, routes)
                .await
                .map(axum::body::Body::new))
        })
    }
}

/// The routes of the server, inside the layer.
struct Routes<S>(S);

impl<S> Routes<S>
where
    S: TowerService<AxumRequest, Response = AxumResponse, Error = Infallible>,
{
    /// The server's answer to `request`.
    async fn run(mut self, request: Request) -> Response {
        match self.0.call(request.map(axum::body::Body::new)).await {
            Ok(response) => response.map(Body::new),
            Err(never) => match never {},
        }
    }
}

/// The server's API as its client reaches it.
fn server_uri(extensions: &Extensions) -> String {
    extensions
        .get::<ApiRequest>()
        .map(|api| api.server_uri.clone())
        .unwrap_or_default()
}

/// The values of the `tags` query parameter of `uri`.
fn tags(uri: &Uri) -> Vec<String> {
    uri.query()
        .unwrap_or_default()
        .split('&')
        .filter_map(|pair| pair.strip_prefix("tags="))
        .map(|tag| {
            percent_decode_str(&tag.replace('+', " "))
                .decode_utf8_lossy()
                .into_owned()
        })
        .collect()
}

/// `answer` with its JSON body, a `T`, changed by `change`, or as it is if its
/// body is no `T`.
async fn with_json<T: DeserializeOwned + Serialize>(
    answer: Response,
    change: impl FnOnce(&mut T),
) -> Response {
    let (mut head, body) = answer.into_parts();
    let Ok(body) = read_body(body).await else {
        return vendor_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "response-too-large",
            "The response is too large",
        );
    };
    let Ok(mut json) = serde_json::from_slice::<T>(&body) else {
        return Response::from_parts(head, Body::new(Full::new(body)));
    };
    change(&mut json);
    let Ok(changed) = serde_json::to_vec(&json) else {
        return Response::from_parts(head, Body::new(Full::new(body)));
    };
    head.headers.remove(CONTENT_LENGTH);
    Response::from_parts(head, Body::new(Full::new(Bytes::from(changed))))
}

/// The URI of a request relative to the API version.
fn api_uri(extensions: &Extensions) -> Uri {
    extensions
        .get::<ApiRequest>()
        .map_or_else(|| Uri::from_static("/"), |api| api.uri.clone())
}

fn empty_body() -> Body {
    Body::new(Empty::<Bytes>::new())
}

async fn read_body(body: Body) -> Result<Bytes, Box<dyn StdError + Send + Sync>> {
    Ok(Limited::new(body, DISCOVERY_LIMIT)
        .collect()
        .await?
        .to_bytes())
}

/// A JSON answer of the federation itself.
fn json_response(status: StatusCode, body: &impl Serialize) -> Response {
    (status, axum::Json(body)).into_response().map(Body::new)
}

/// A SOVD `GenericError` of the federation itself, with a vendor code.
fn vendor_error(status: StatusCode, code: &str, message: &str) -> Response {
    json_response(status, &GenericError::with_vendor_code(code, message))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_the_paths_it_decides_on() {
        use ForwardRoute::{Authorize, Collections, CustomCollections, Locks};

        let rule = |path| classify(path).and_then(|(rule, id)| Some((rule.route()?, id)));
        let entity = |path| matches!(classify(path), Some((Rule::Entity(COMPONENTS), _)));

        assert_eq!(
            rule("/components/ecu"),
            Some((ForwardRoute::Entities, Some("ecu")))
        );
        assert_eq!(
            rule("/components/ecu/"),
            Some((ForwardRoute::Entities, Some("ecu")))
        );
        assert_eq!(
            rule("/components/ecu/data/x"),
            Some((ForwardRoute::Entities, Some("ecu")))
        );
        assert!(entity("/components/ecu/data/x"));
        assert!(!entity("/components"));
        assert_eq!(rule("/components/"), None);
        assert_eq!(rule("/components//data"), None);

        assert_eq!(rule("/locks"), Some((Locks, None)));
        assert_eq!(rule("/locks/id"), Some((Locks, None)));
        assert_eq!(rule("/locks/"), None);
        assert_eq!(rule("/locks/id/"), None);
        assert_eq!(rule("/locks/id/x"), None);

        assert_eq!(rule("/data"), Some((Collections, None)));
        assert_eq!(rule("/data/"), Some((Collections, None)));
        assert_eq!(rule("/data-groups/x/y"), Some((Collections, None)));
        assert_eq!(rule("/authorize"), Some((Authorize, None)));
        assert_eq!(rule("/authorize/"), None);

        assert!(matches!(classify("/"), Some((Rule::Root, None))));
        assert!(classify("//").is_none());
        assert_eq!(rule("/x-vendor"), Some((CustomCollections, None)));
        assert_eq!(rule("/x-vendor/a/b"), Some((CustomCollections, None)));
        assert_eq!(
            rule("/components/ecu/x-vendor"),
            Some((ForwardRoute::Entities, Some("ecu")))
        );
        assert_eq!(rule("/unknown"), None);
    }
}
