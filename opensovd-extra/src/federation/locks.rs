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
//! `GET /locks` lists what the upstreams list, so locks acquired directly from
//! an upstream, or before a restart of the server, show too, each as its
//! upstream lists it and with its id, which the server does not know. They keep
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
use std::sync::{RwLock, RwLockReadGuard};
use std::time::{Duration, Instant};

use bytes::Bytes;
use futures::future::join_all;
use http::header::LOCATION;
use http::request::Parts;
use http::{HeaderValue, Method, StatusCode, Uri};
use http_body_util::Full;
use opensovd_models::Collection;
use opensovd_models::error::{ErrorCode, GenericError};
use opensovd_models::locking::Lock;
use opensovd_server::Body;
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};
use tower::Service as TowerService;

use super::{
    AxumRequest, AxumResponse, Federation, LOCKS, Request, Response, Routes, TARGET, Upstream,
    api_uri, empty_body, json_response, read_body, read_guard, server_uri, vendor_error,
    write_guard,
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

impl Held {
    /// Whether the lock has not expired at `now`.
    fn is_live(&self, now: Instant) -> bool {
        self.until > now
    }
}

/// What [`VehicleLocks`] holds, for the log.
const VEHICLE_LOCKS: &str = "vehicle locks";

/// The locks of the server by their id.
#[derive(Default)]
pub(super) struct VehicleLocks(RwLock<BTreeMap<String, Held>>);

impl VehicleLocks {
    /// The table for reading, expired locks included: see [`Held::is_live`].
    fn table(&self) -> RwLockReadGuard<'_, BTreeMap<String, Held>> {
        read_guard(&self.0, VEHICLE_LOCKS)
    }

    /// The lock `id`, unless it expired.
    fn get(&self, id: &str) -> Option<Held> {
        let now = Instant::now();
        self.table()
            .get(id)
            .filter(|held| held.is_live(now))
            .cloned()
    }

    /// The id of the lock that stands for `ids`. An upstream that renews a
    /// lock its client already owns gives the same id, which is then the same
    /// lock, and a lock that shares a part with it is over.
    fn insert_or_renew(&self, ids: Vec<String>, until: Instant) -> String {
        let mut locks = write_guard(&self.0, VEHICLE_LOCKS);
        // Every new lock drops the expired ones, so the table holds no more
        // than the locks acquired since the oldest that is still running.
        let now = Instant::now();
        locks.retain(|_, held| held.is_live(now));
        if let Some((id, held)) = locks.iter_mut().find(|(_, held)| held.ids == ids) {
            held.until = until;
            return id.clone();
        }
        locks.retain(|_, held| !held.ids.iter().any(|id| ids.contains(id)));
        let id = uuid::Uuid::new_v4().to_string();
        locks.insert(id.clone(), Held { ids, until });
        id
    }

    fn renew(&self, id: &str, until: Instant) {
        let mut locks = write_guard(&self.0, VEHICLE_LOCKS);
        if let Some(held) = locks.get_mut(id) {
            held.until = until;
        }
    }

    fn remove(&self, id: &str) {
        let mut locks = write_guard(&self.0, VEHICLE_LOCKS);
        locks.remove(id);
    }
}

/// Answers a request below `/locks`.
pub(super) async fn handle<S>(forward: &Federation, request: Request, routes: Routes<S>) -> Response
where
    S: TowerService<AxumRequest, Response = AxumResponse, Error = Infallible>,
{
    let path = api_uri(request.extensions()).path().to_owned();
    // [`Federation::destination`] sends `/locks` and `/locks/{id}` here.
    let id = path.split('/').nth(2);
    match (request.method().clone(), id) {
        (Method::POST, None) => acquire_vehicle_lock(forward, request).await,
        (Method::GET, None) => list_locks(forward, request).await,
        (Method::GET, Some(id)) => lock_details(forward, request, id).await,
        (Method::PUT, Some(id)) => modify_expiration(forward, request, id).await,
        (Method::DELETE, Some(id)) => release_lock(forward, request, id).await,
        _ => routes.run(request).await,
    }
}

async fn acquire_vehicle_lock(forward: &Federation, request: Request) -> Response {
    let query = request_query(&request);
    let uri = api_uri(request.extensions());
    let (parts, body) = request.into_parts();
    let Ok(body) = read_body(body).await else {
        return too_large_error();
    };

    // The locks acquired so far, without those an upstream merely renewed: the
    // client had these before and keeps them if this fails.
    let mut acquired = Vec::new();
    let mut ids = Vec::new();
    let mut answers = Vec::new();
    for upstream in forward.lock_upstreams() {
        let request = parts.clone();
        let answer = upstream
            .send_to(request, Body::new(Full::new(body.clone())), uri.clone())
            .await;
        let status = answer.status();
        if !status.is_success() {
            release_acquired(&parts, acquired, query.as_deref()).await;
            return upstream_error(answer, "the lock").await;
        }
        let Some((id, lock)) = parse_body::<Lock>(answer)
            .await
            .and_then(|lock| Some((lock.id.clone()?, lock)))
        else {
            release_acquired(&parts, acquired, query.as_deref()).await;
            return invalid_lock_error();
        };
        if status == StatusCode::CREATED {
            acquired.push((upstream.clone(), id.clone()));
        }
        ids.push(id);
        answers.push(lock);
    }
    if answers.is_empty() {
        return invalid_lock_error();
    }

    let id = forward
        .vehicle_locks
        .insert_or_renew(ids, requested_expiry(&body));
    let mut lock = merge_parts(answers);
    lock.id = Some(id.clone());
    // A lock that only got renewed was the client's already, as its answer
    // is: 200 without a `Location`.
    if acquired.is_empty() {
        return json_response(StatusCode::OK, &lock);
    }
    let mut answer = json_response(StatusCode::CREATED, &lock);
    let location = format!("{}/{LOCKS}/{id}", server_uri(&parts.extensions));
    if let Ok(location) = HeaderValue::try_from(location) {
        answer.headers_mut().insert(LOCATION, location);
    }
    answer
}

/// The locks of all upstreams, by what each lists to the client: a lock of
/// the server once, with its id, as all its parts show it, and every other lock
/// of an upstream, such as one acquired from it directly or before a restart,
/// as the upstream lists it. Only the upstreams know who owns a lock.
async fn list_locks(forward: &Federation, request: Request) -> Response {
    let (parts, _) = request.into_parts();
    let uri = api_uri(&parts.extensions);
    let upstreams: Vec<_> = forward.lock_upstreams().collect();
    let requests = upstreams
        .iter()
        .map(|upstream| upstream.request_with_client_token(&parts, Method::GET, uri.clone()));
    let mut lists = Vec::new();
    for answer in join_all(requests).await {
        // An upstream decides who may see its locks.
        if !answer.status().is_success() {
            return upstream_error(answer, "the list of locks").await;
        }
        let Some(list) = parse_body::<Collection<Lock>>(answer).await else {
            return invalid_lock_error();
        };
        lists.push(list.items);
    }
    if lists.is_empty() {
        return invalid_lock_error();
    }

    let table = forward.vehicle_locks.table();
    let now = Instant::now();
    let known: Vec<_> = table.iter().filter(|(_, held)| held.is_live(now)).collect();
    let mut items = Vec::new();
    for &(id, held) in &known {
        let seen: Option<Vec<Lock>> = lists
            .iter()
            .zip(&held.ids)
            .map(|(list, part)| {
                list.iter()
                    .find(|lock| lock.id.as_ref() == Some(part))
                    .cloned()
            })
            .collect();
        // A part one upstream does not list leaves the lock out, but the
        // lock stays known: what a client sees may not be all there is, and
        // details and the parts' expiry tell when it is over.
        let Some(seen) = seen else {
            continue;
        };
        let mut item = merge_parts(seen);
        item.id = Some(id.clone());
        items.push(item);
    }
    for (index, list) in lists.into_iter().enumerate() {
        let of_the_server = |lock: &Lock| {
            known.iter().any(|(_, held)| {
                held.ids
                    .get(index)
                    .is_some_and(|part| lock.id.as_ref() == Some(part))
            })
        };
        items.extend(list.into_iter().filter(|lock| !of_the_server(lock)));
    }
    let list = Collection {
        items,
        schema: None,
    };
    json_response(StatusCode::OK, &list)
}

/// The lock as all upstreams see it. If one has lost its part, the lock does
/// not hold anymore, whatever the others say.
async fn lock_details(forward: &Federation, request: Request, id: &str) -> Response {
    let Some(held) = forward.vehicle_locks.get(id) else {
        return not_found_error(api_uri(request.extensions()).path());
    };
    let query = request_query(&request);
    let (parts, _) = request.into_parts();
    let targets = part_uris(forward, &held, query.as_deref());
    let answers = join_all(targets.iter().map(|(upstream, uri)| {
        upstream.request_with_client_token(&parts, Method::GET, uri.clone())
    }))
    .await;

    if answers.iter().any(|a| a.status() == StatusCode::NOT_FOUND) {
        return end_broken_lock(forward, &parts, id, answers, targets).await;
    }
    match read_merged_lock(answers, id).await {
        // ISO 17978-3 Table 248 has no id: the path names the lock.
        Ok(lock) => json_response(StatusCode::OK, &Lock { id: None, ..lock }),
        Err(answer) => *answer,
    }
}

async fn modify_expiration(forward: &Federation, request: Request, id: &str) -> Response {
    let Some(held) = forward.vehicle_locks.get(id) else {
        return not_found_error(api_uri(request.extensions()).path());
    };
    let path = api_uri(request.extensions()).path().to_owned();
    let query = request_query(&request);
    let (parts, body) = request.into_parts();
    let Ok(body) = read_body(body).await else {
        return too_large_error();
    };
    let targets = part_uris(forward, &held, query.as_deref());
    let answers = join_all(targets.iter().map(|(upstream, uri)| {
        upstream.send_to(
            parts.clone(),
            Body::new(Full::new(body.clone())),
            uri.clone(),
        )
    }))
    .await;

    if answers.iter().any(|a| a.status() == StatusCode::NOT_FOUND) {
        return end_broken_lock(forward, &parts, id, answers, targets).await;
    }
    if answers.iter().all(|a| a.status().is_success()) {
        forward.vehicle_locks.renew(id, requested_expiry(&body));
    }
    match first_failure_or_first(answers) {
        Some(answer) => gateway_answer(answer, id).await,
        None => not_found_error(&path),
    }
}

/// Releases all parts. One that is gone already is released, and one that the
/// upstream refuses to release keeps the lock, so that it stays listed.
async fn release_lock(forward: &Federation, request: Request, id: &str) -> Response {
    let Some(held) = forward.vehicle_locks.get(id) else {
        return not_found_error(api_uri(request.extensions()).path());
    };
    let path = api_uri(request.extensions()).path().to_owned();
    let query = request_query(&request);
    let (parts, _) = request.into_parts();
    let targets = part_uris(forward, &held, query.as_deref());
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
        forward.vehicle_locks.remove(id);
    }
    let released: Vec<_> = answers
        .into_iter()
        .filter(|answer| answer.status() != StatusCode::NOT_FOUND)
        .collect();
    // All parts gone is a lock that is over already.
    match first_failure_or_first(released) {
        Some(answer) => gateway_answer(answer, id).await,
        None => not_found_error(&path),
    }
}

/// The first answer that is no success, or else the first answer.
fn first_failure_or_first(answers: Vec<Response>) -> Option<Response> {
    let at = answers
        .iter()
        .position(|answer| !answer.status().is_success())
        .unwrap_or_default();
    answers.into_iter().nth(at)
}

/// Answers that the lock `id` is over, as `answers` show: `targets` that still
/// hold a part release it.
async fn end_broken_lock(
    forward: &Federation,
    parts: &Parts,
    id: &str,
    answers: Vec<Response>,
    targets: Vec<(Upstream, Uri)>,
) -> Response {
    forward.vehicle_locks.remove(id);
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
async fn release_acquired(parts: &Parts, acquired: Vec<(Upstream, String)>, query: Option<&str>) {
    let locks = acquired
        .iter()
        .filter_map(|(upstream, id)| Some((upstream, upstream_lock_uri(id, query)?)));
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
fn part_uris(forward: &Federation, held: &Held, query: Option<&str>) -> Vec<(Upstream, Uri)> {
    forward
        .lock_upstreams()
        .zip(&held.ids)
        .filter_map(|(upstream, id)| Some((upstream.clone(), upstream_lock_uri(id, query)?)))
        .collect()
}

fn request_query(request: &Request) -> Option<String> {
    api_uri(request.extensions()).query().map(str::to_owned)
}

/// The URI of the lock `id` of an upstream.
fn upstream_lock_uri(id: &str, query: Option<&str>) -> Option<Uri> {
    let id = utf8_percent_encode(id, NOT_UNRESERVED);
    let uri = match query {
        Some(query) => format!("/{LOCKS}/{id}?{query}"),
        None => format!("/{LOCKS}/{id}"),
    };
    uri.parse().ok()
}

fn not_found_error(path: &str) -> Response {
    vendor_error(
        StatusCode::NOT_FOUND,
        "resource-not-found",
        &format!("Resource not found: {path}"),
    )
}

fn invalid_lock_error() -> Response {
    vendor_error(
        StatusCode::BAD_GATEWAY,
        "upstream-invalid-lock",
        "invalid lock of an upstream",
    )
}

fn too_large_error() -> Response {
    vendor_error(
        StatusCode::PAYLOAD_TOO_LARGE,
        "request-too-large",
        "The request is too large",
    )
}

/// When the lock of the request `body` expires. Only `lock_expiration` counts
/// here and the upstreams judge the rest, which may treat keys and values more
/// freely than a model: a CDA ignores the case of keys.
fn requested_expiry(body: &Bytes) -> Instant {
    let fields: Map<String, Value> = serde_json::from_slice(body).unwrap_or_default();
    let seconds = fields
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(LOCK_EXPIRATION))
        .and_then(|(_, seconds)| seconds.as_u64())
        .unwrap_or_default();
    let now = Instant::now();
    now.checked_add(Duration::from_secs(seconds)).unwrap_or(now)
}

/// The merged lock `id` of the `answers` of all its parts, or the server's
/// answer to the first that is no success.
async fn read_merged_lock(answers: Vec<Response>, id: &str) -> Result<Lock, Box<Response>> {
    let mut seen = Vec::new();
    for answer in answers {
        if !answer.status().is_success() {
            let about = format!("the lock {id}");
            return Err(Box::new(upstream_error(answer, &about).await));
        }
        let part = parse_body(answer)
            .await
            .ok_or_else(|| Box::new(invalid_lock_error()))?;
        seen.push(part);
    }
    if seen.is_empty() {
        return Err(Box::new(invalid_lock_error()));
    }
    Ok(merge_parts(seen))
}

async fn parse_body<T: DeserializeOwned>(answer: Response) -> Option<T> {
    let body: Bytes = read_body(answer.into_body()).await.ok()?;
    serde_json::from_slice(&body).ok()
}

/// The server's answer to the upstream's `answer` about the lock `id`: its
/// status alone for a success, and its error otherwise. Every answer below
/// `/locks` is the server's, so an upstream's lock never shows.
async fn gateway_answer(answer: Response, id: &str) -> Response {
    let status = answer.status();
    if status.is_success() {
        let mut answer = Response::new(empty_body());
        *answer.status_mut() = status;
        return answer;
    }
    upstream_error(answer, &format!("the lock {id}")).await
}

/// The server's answer to an upstream that refused `about`, such as a lock:
/// its status and error, such as `lock-broken` with who broke the lock, with a
/// message of the server by the error code, as the upstream's names its own
/// lock.
async fn upstream_error(answer: Response, about: &str) -> Response {
    let status = answer.status();
    let error = parse_body::<GenericError>(answer).await.map_or_else(
        || GenericError::with_vendor_code("upstream-error", format!("An upstream refused {about}")),
        |error| GenericError {
            message: match error.error_code {
                ErrorCode::LockBroken => format!("An upstream reports {about} as broken"),
                _ => format!("An upstream refused {about}"),
            },
            translation_id: None,
            ..error
        },
    );
    json_response(status, &error)
}

/// What the upstreams say about the parts of one lock: it expires with the
/// first of them, is owned if all are, and the first upstream speaks for the
/// rest.
fn merge_parts(locks: Vec<Lock>) -> Lock {
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
