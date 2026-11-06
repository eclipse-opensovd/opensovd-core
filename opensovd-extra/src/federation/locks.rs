// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! The vehicle lock of a server with upstreams.
//!
//! A lock at the root applies to everything the server manages, ISO 17978-3
//! 7.17.2, so it has to hold on all upstreams, while each of them gives its own
//! lock its own id. A lock of the server is therefore its own resource, with its
//! own id, also with a single upstream, and consists of parts. Today all parts
//! are locks of upstreams; when the server implements locking, its own lock of
//! the entities it holds is one more.
//!
//! The server remembers which parts make up a lock, and nothing else: the
//! upstreams hold them and decide who owns one, by the client's token. They are
//! only asked on a client's request, with its token, and their locks expire by
//! themselves, so a restart of the server leaves nothing locked for good.
//!
//! A lock is acquired from the upstreams in their order, so two clients cannot
//! each hold a part and wait for the other, and only if all of them grant it.
//! An upstream without locks refuses, as the lock would not be global.
//!
//! Locks acquired directly from an upstream are not listed here, but they keep
//! a client of this server from acquiring its lock.
//!
//! `break_lock` reaches the upstreams with the rest of the request, and each
//! decides for itself. A part it broke may stay visible, as a CDA keeps it as
//! a defunct lock until its client deletes it, so a lock that lost a part stays
//! here too: the upstream's `lock-broken` is passed on, with what it tells about
//! who broke it, and the client's `DELETE` reaches all parts. If an upstream
//! refuses after another broke a part, nothing restores that part.

use std::collections::BTreeMap;
use std::convert::Infallible;
use std::sync::{PoisonError, RwLock};
use std::time::{Duration, Instant};

use bytes::Bytes;
use futures::future::join_all;
use http::header::{CONTENT_LENGTH, LOCATION};
use http::request::Parts;
use http::{HeaderValue, Method, StatusCode, Uri, response};
use http_body_util::Full;
use opensovd_models::Collection;
use opensovd_models::error::{ErrorCode, GenericError};
use opensovd_models::locking::Lock;
use opensovd_server::Body;
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
use serde_json::{Map, Value};
use tower::Service as TowerService;

use super::{
    AxumRequest, AxumResponse, Federation, LOCKS, Request, Response, Routes, TARGET, Upstream,
    api_uri, empty_body, json_response, read_body, vendor_error,
};

/// The key of the seconds a lock lasts in a request (ISO 17978-3 Table 240).
const LOCK_EXPIRATION: &str = "lock_expiration";

/// What an id of an upstream's lock has to escape in a path: all but the
/// unreserved characters of RFC 3986.
const NOT_UNRESERVED: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');

/// A lock of the server: the ids the upstreams gave it, in their order, and
/// when it expires, after which the upstreams have released it by themselves.
#[derive(Clone)]
struct Held {
    ids: Vec<String>,
    until: Instant,
}

/// The locks of the server by their id.
#[derive(Default)]
pub(super) struct VehicleLocks(RwLock<BTreeMap<String, Held>>);

impl VehicleLocks {
    /// The locks that have not expired.
    fn current(&self) -> BTreeMap<String, Held> {
        let now = Instant::now();
        let mut locks = self.0.write().unwrap_or_else(PoisonError::into_inner);
        locks.retain(|_, held| held.until > now);
        locks.clone()
    }

    fn get(&self, id: &str) -> Option<Held> {
        self.current().remove(id)
    }

    /// The id of the lock that stands for `ids`. An upstream that renews a
    /// lock its client already owns gives the same id, which is then the same
    /// lock, and a lock that shares a part with it is over.
    fn hold(&self, ids: Vec<String>, until: Instant) -> String {
        let mut locks = self.0.write().unwrap_or_else(PoisonError::into_inner);
        if let Some((id, held)) = locks.iter_mut().find(|(_, held)| held.ids == ids) {
            held.until = until;
            return id.clone();
        }
        locks.retain(|_, held| !held.ids.iter().any(|id| ids.contains(id)));
        let id = uuid::Uuid::new_v4().to_string();
        locks.insert(id.clone(), Held { ids, until });
        id
    }

    fn extend(&self, id: &str, until: Instant) {
        let mut locks = self.0.write().unwrap_or_else(PoisonError::into_inner);
        if let Some(held) = locks.get_mut(id) {
            held.until = until;
        }
    }

    fn forget(&self, id: &str) {
        let mut locks = self.0.write().unwrap_or_else(PoisonError::into_inner);
        locks.remove(id);
    }
}

/// Answers a request below `/locks`.
pub(super) async fn answer<S>(forward: &Federation, request: Request, routes: Routes<S>) -> Response
where
    S: TowerService<AxumRequest, Response = AxumResponse, Error = Infallible>,
{
    let path = api_uri(request.extensions()).path().to_owned();
    // [`Federation::destination`] sends `/locks` and `/locks/{id}` here.
    let id = path.split('/').nth(2);
    match (request.method().clone(), id) {
        (Method::POST, None) => acquire(forward, request).await,
        (Method::GET, None) => list(forward, request).await,
        (Method::GET, Some(id)) => details(forward, request, id).await,
        (Method::PUT, Some(id)) => modify(forward, request, id).await,
        (Method::DELETE, Some(id)) => release(forward, request, id).await,
        _ => routes.run(request).await,
    }
}

async fn acquire(forward: &Federation, request: Request) -> Response {
    let query = query(&request);
    let uri = api_uri(request.extensions());
    let (parts, body) = request.into_parts();
    let Ok(body) = read_body(body).await else {
        return too_large();
    };

    // The locks acquired so far, without those an upstream merely renewed: the
    // client had these before and keeps them if this fails.
    let mut acquired = Vec::new();
    let mut ids = Vec::new();
    let mut answers = Vec::new();
    let mut heads = Vec::new();
    for upstream in forward.lock_upstreams() {
        let request = parts.clone();
        let answer = upstream
            .send_to(request, Body::new(Full::new(body.clone())), uri.clone())
            .await;
        let status = answer.status();
        if !status.is_success() {
            roll_back(&parts, acquired, query.as_deref()).await;
            return answer;
        }
        let part = read(answer).await.and_then(|(head, lock)| {
            let id = lock.id.clone()?;
            Some((head, lock, id))
        });
        let Some((head, lock, id)) = part else {
            roll_back(&parts, acquired, query.as_deref()).await;
            return invalid_lock();
        };
        if status == StatusCode::CREATED {
            acquired.push((upstream.clone(), id.clone()));
        }
        ids.push(id);
        answers.push(lock);
        heads.push(head);
    }

    let first_id = ids.first().cloned().unwrap_or_default();
    let id = forward.vehicle_locks.hold(ids, until(&body));
    let created = !acquired.is_empty();
    let mut lock = merged(answers);
    lock.id = Some(id.clone());
    let Some(mut head) = heads.into_iter().next() else {
        return invalid_lock();
    };
    if created {
        head.status = StatusCode::CREATED;
    }
    let location = head.headers.get(LOCATION).and_then(|l| l.to_str().ok());
    let location = location.map(|location| location.replace(&first_id, &id));
    if let Some(location) = location.and_then(|l| HeaderValue::try_from(l).ok()) {
        head.headers.insert(LOCATION, location);
    }
    answer_with(head, &lock)
}

/// One entry per lock, as all upstreams see it, as only they know who owns it
/// and whether a part was broken. A lock whose part is gone is over.
async fn list(forward: &Federation, request: Request) -> Response {
    let query = query(&request);
    let (parts, _) = request.into_parts();
    let known = forward.vehicle_locks.current();
    // Without locks nothing below asks an upstream, and it decides who may ask.
    if known.is_empty() {
        let Some(first) = forward.lock_upstreams().next() else {
            return invalid_lock();
        };
        let uri = api_uri(&parts.extensions);
        let answer = first
            .request_with_client_token(&parts, Method::GET, uri)
            .await;
        if !answer.status().is_success() {
            return answer;
        }
    }
    let requests = known.values().map(|held| {
        let targets = parts_of(forward, held, query.as_deref());
        let parts = &parts;
        async move {
            join_all(targets.iter().map(|(upstream, uri)| {
                upstream.request_with_client_token(parts, Method::GET, uri.clone())
            }))
            .await
        }
    });
    let replies = join_all(requests).await;

    let mut items = Vec::new();
    for (id, reply) in known.keys().zip(replies) {
        if reply.iter().any(|a| a.status() == StatusCode::NOT_FOUND) {
            forward.vehicle_locks.forget(id);
            continue;
        }
        let mut item = match read_parts(reply).await {
            Ok((_, item)) => item,
            Err(answer) => return *answer,
        };
        item.id = Some(id.clone());
        items.push(item);
    }
    let list = Collection {
        items,
        schema: None,
    };
    json_response(StatusCode::OK, &list)
}

/// The lock as all upstreams see it. If one has lost its part, the lock does
/// not hold anymore, whatever the others say.
async fn details(forward: &Federation, request: Request, id: &str) -> Response {
    let Some(held) = forward.vehicle_locks.get(id) else {
        return not_found(api_uri(request.extensions()).path());
    };
    let query = query(&request);
    let (parts, _) = request.into_parts();
    let targets = parts_of(forward, &held, query.as_deref());
    let answers = join_all(targets.iter().map(|(upstream, uri)| {
        upstream.request_with_client_token(&parts, Method::GET, uri.clone())
    }))
    .await;

    if answers.iter().any(|a| a.status() == StatusCode::NOT_FOUND) {
        return broken(forward, &parts, id, answers, targets).await;
    }
    match read_parts(answers).await {
        Ok((meta, part)) => answer_with(meta, &part),
        Err(answer) => *answer,
    }
}

async fn modify(forward: &Federation, request: Request, id: &str) -> Response {
    let Some(held) = forward.vehicle_locks.get(id) else {
        return not_found(api_uri(request.extensions()).path());
    };
    let path = api_uri(request.extensions()).path().to_owned();
    let query = query(&request);
    let (parts, body) = request.into_parts();
    let Ok(body) = read_body(body).await else {
        return too_large();
    };
    let targets = parts_of(forward, &held, query.as_deref());
    let answers = join_all(targets.iter().map(|(upstream, uri)| {
        upstream.send_to(
            parts.clone(),
            Body::new(Full::new(body.clone())),
            uri.clone(),
        )
    }))
    .await;

    if answers.iter().any(|a| a.status() == StatusCode::NOT_FOUND) {
        return broken(forward, &parts, id, answers, targets).await;
    }
    if answers.iter().all(|a| a.status().is_success()) {
        forward.vehicle_locks.extend(id, until(&body));
    }
    pick(answers).unwrap_or_else(|| not_found(&path))
}

/// Releases all parts. One that is gone already is released, and one that the
/// upstream refuses to release keeps the lock, so that it stays listed.
async fn release(forward: &Federation, request: Request, id: &str) -> Response {
    let Some(held) = forward.vehicle_locks.get(id) else {
        return not_found(api_uri(request.extensions()).path());
    };
    let path = api_uri(request.extensions()).path().to_owned();
    let query = query(&request);
    let (parts, _) = request.into_parts();
    let targets = parts_of(forward, &held, query.as_deref());
    let answers = join_all(
        targets
            .iter()
            .map(|(upstream, uri)| upstream.send_to(parts.clone(), empty_body(), uri.clone())),
    )
    .await;

    let held_on = |answer: &Response| {
        !answer.status().is_success() && answer.status() != StatusCode::NOT_FOUND
    };
    if !answers.iter().any(held_on) {
        forward.vehicle_locks.forget(id);
    }
    let (released, gone): (Vec<_>, Vec<_>) = answers
        .into_iter()
        .partition(|answer| answer.status() != StatusCode::NOT_FOUND);
    pick(released)
        .or_else(|| pick(gone))
        .unwrap_or_else(|| not_found(&path))
}

/// The first answer that is no success, or else the first answer.
fn pick(answers: Vec<Response>) -> Option<Response> {
    let at = answers
        .iter()
        .position(|answer| !answer.status().is_success())
        .unwrap_or_default();
    answers.into_iter().nth(at)
}

/// Answers that the lock `id` is over, as `answers` show: `targets` that still
/// hold a part release it.
async fn broken(
    forward: &Federation,
    parts: &Parts,
    id: &str,
    answers: Vec<Response>,
    targets: Vec<(Upstream, Uri)>,
) -> Response {
    forward.vehicle_locks.forget(id);
    let left = targets
        .into_iter()
        .zip(answers)
        .filter(|(_, answer)| answer.status() != StatusCode::NOT_FOUND)
        .map(|(target, _)| target);
    join_all(left.map(|(upstream, uri)| async move {
        let status = upstream
            .request_with_client_token(parts, Method::DELETE, uri)
            .await
            .status();
        if !status.is_success() {
            tracing::warn!(target: TARGET, %status, "Upstream kept the part of a broken lock");
        }
    }))
    .await;
    json_response(
        StatusCode::CONFLICT,
        &GenericError::new(
            ErrorCode::LockBroken,
            format!("The lock {id} does not hold on all upstreams anymore"),
        ),
    )
}

/// Releases what was `acquired` for a lock that failed. What an upstream does
/// not release expires by itself.
async fn roll_back(parts: &Parts, acquired: Vec<(Upstream, String)>, query: Option<&str>) {
    let locks = acquired
        .iter()
        .filter_map(|(upstream, id)| Some((upstream, lock_uri(id, query)?)));
    join_all(locks.map(|(upstream, uri)| async move {
        let status = upstream
            .request_with_client_token(parts, Method::DELETE, uri)
            .await
            .status();
        if !status.is_success() {
            tracing::warn!(target: TARGET, %status, "Upstream kept the part of a failed lock");
        }
    }))
    .await;
}

/// The upstreams of `held` with the URIs of their parts.
fn parts_of(forward: &Federation, held: &Held, query: Option<&str>) -> Vec<(Upstream, Uri)> {
    forward
        .lock_upstreams()
        .zip(&held.ids)
        .filter_map(|(upstream, id)| Some((upstream.clone(), lock_uri(id, query)?)))
        .collect()
}

fn query(request: &Request) -> Option<String> {
    api_uri(request.extensions()).query().map(str::to_owned)
}

/// The URI of the lock `id` of an upstream.
fn lock_uri(id: &str, query: Option<&str>) -> Option<Uri> {
    let id = utf8_percent_encode(id, NOT_UNRESERVED);
    let uri = match query {
        Some(query) => format!("/{LOCKS}/{id}?{query}"),
        None => format!("/{LOCKS}/{id}"),
    };
    uri.parse().ok()
}

fn not_found(path: &str) -> Response {
    vendor_error(
        StatusCode::NOT_FOUND,
        "resource-not-found",
        &format!("Resource not found: {path}"),
    )
}

fn invalid_lock() -> Response {
    vendor_error(
        StatusCode::BAD_GATEWAY,
        "upstream-invalid-lock",
        "invalid lock of an upstream",
    )
}

fn too_large() -> Response {
    vendor_error(
        StatusCode::PAYLOAD_TOO_LARGE,
        "request-too-large",
        "The request is too large",
    )
}

/// When the lock of the request `body` expires. Only `lock_expiration` counts
/// here and the upstreams judge the rest, which may treat keys and values more
/// freely than a model: a CDA ignores the case of keys.
fn until(body: &Bytes) -> Instant {
    let fields: Map<String, Value> = serde_json::from_slice(body).unwrap_or_default();
    let seconds = fields
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(LOCK_EXPIRATION))
        .and_then(|(_, seconds)| seconds.as_u64())
        .unwrap_or_default();
    let now = Instant::now();
    now.checked_add(Duration::from_secs(seconds)).unwrap_or(now)
}

/// The first head and the merged lock of the `answers` of all parts, or the
/// first answer that is no success.
async fn read_parts(answers: Vec<Response>) -> Result<(response::Parts, Lock), Box<Response>> {
    let mut first = None;
    let mut seen = Vec::new();
    for answer in answers {
        if !answer.status().is_success() {
            return Err(Box::new(answer));
        }
        let invalid = || Box::new(invalid_lock());
        let (head, part) = read(answer).await.ok_or_else(invalid)?;
        first.get_or_insert(head);
        seen.push(part);
    }
    let head = first.ok_or_else(|| Box::new(invalid_lock()))?;
    Ok((head, merged(seen)))
}

async fn read(answer: Response) -> Option<(response::Parts, Lock)> {
    let (head, body) = answer.into_parts();
    let body: Bytes = read_body(body).await.ok()?;
    Some((head, serde_json::from_slice(&body).ok()?))
}

fn answer_with(mut head: response::Parts, lock: &Lock) -> Response {
    head.headers.remove(CONTENT_LENGTH);
    let body = serde_json::to_vec(lock).unwrap_or_default();
    Response::from_parts(head, Body::new(Full::new(Bytes::from(body))))
}

/// What the upstreams say about the parts of one lock: it expires with the
/// first of them, is owned if all are, and the first upstream speaks for the
/// rest.
fn merged(locks: Vec<Lock>) -> Lock {
    let lock_expiration = locks.iter().filter_map(|lock| lock.lock_expiration).min();
    let reports_owner = locks.iter().any(|lock| lock.owned.is_some());
    let owned = locks.iter().all(|lock| lock.owned == Some(true));

    let mut merged = Lock {
        lock_expiration,
        owned: reports_owner.then_some(owned),
        ..Lock::default()
    };
    for lock in locks {
        merged.id = merged.id.or(lock.id);
        merged.scopes = merged.scopes.or(lock.scopes);
        for (name, value) in lock.extensions {
            merged.extensions.entry(name).or_insert(value);
        }
    }
    merged
}
