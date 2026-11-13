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
//! The id of a lock of the server names its parts, so the server keeps no
//! state: the upstreams hold the locks and decide who owns one, by the client's
//! token, and they are only asked on a client's request. A lock outlives a
//! restart of the server, and the locks of the upstreams expire by themselves.
//! A client owns one lock per upstream at most, as an upstream renews the lock
//! of its owner, so the parts a client owns on all upstreams are its lock.
//!
//! A lock is acquired from the upstreams in their order, so two clients cannot
//! each hold a part and wait for the other, and only if all of them grant it.
//! An upstream without locks refuses, as the lock would not be global.
//!
//! `GET /locks` lists the locks of all upstreams by what each lists to the
//! client. Every lock that is no part of the client's lock, such as one acquired
//! directly from an upstream or one of another client, is a lock of the server
//! of a single part, so every id the list holds can be asked for. Such locks
//! keep a client of this server from acquiring its lock.
//!
//! `break_lock` reaches the upstreams with the rest of the request, and each
//! decides for itself, as ISO 17978-3 has no way to ask first. A part it broke
//! may stay visible, as a CDA keeps it as a defunct lock until its client
//! deletes it, so a lock that lost a part stays here too: the upstream's
//! `lock-broken` is passed on, with what it tells about who broke it, and the
//! client's `DELETE` reaches all parts. A part that is gone, as of an upstream
//! that removes a broken lock, makes the lock `lock-broken` too, and the parts
//! left stay until the client's `DELETE`: only that ends a lock. If an upstream refuses after others
//! granted `break_lock`, the locks those broke stay broken, as no upstream can
//! restore one, and the refusal tells how many granted it.

use std::convert::Infallible;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use bytes::Bytes;
use futures::future::join_all;
use http::header::{LOCATION, WWW_AUTHENTICATE};
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
    api_uri, empty_body, json_response, read_body, server_uri, vendor_error,
};

/// The key that asks to break the locks of others (ISO 17978-3 Table 240).
const BREAK_LOCK: &str = "break_lock";

/// The parameter of a refused lock that tells how many upstreams granted
/// `break_lock` before one refused, each of which may have broken a lock.
const BREAK_LOCK_GRANTED: &str = "break_lock_granted";

/// The longest id of a lock of the server that is decoded.
const MAX_LOCK_ID_LEN: usize = 4096;

/// What an id of an upstream's lock has to escape in a path: all but the
/// unreserved characters of RFC 3986.
const NOT_UNRESERVED: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');

/// The id of the lock of the server that consists of `parts`, the ids of the
/// locks of the upstreams in their order, without a part where it has none.
fn lock_id(parts: &[Option<&str>]) -> String {
    URL_SAFE_NO_PAD.encode(serde_json::to_vec(parts).unwrap_or_default())
}

/// The upstreams of the lock `id` with the ids of its parts, or nothing if
/// `id` is no lock of the server with these upstreams.
fn lock_parts<'a>(forward: &'a Federation, id: &str) -> Option<Vec<(&'a Upstream, String)>> {
    if id.len() > MAX_LOCK_ID_LEN {
        return None;
    }
    let json = URL_SAFE_NO_PAD.decode(id).ok()?;
    let parts: Vec<Option<String>> = serde_json::from_slice(&json).ok()?;
    if parts.len() != forward.lock_upstreams().count() {
        return None;
    }
    let parts: Vec<_> = forward
        .lock_upstreams()
        .zip(parts)
        .filter_map(|(upstream, part)| Some((upstream, part?)))
        .collect();
    (!parts.is_empty()).then_some(parts)
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
    let breaks = breaks_locks(&body);

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
            let granted = acquired.len();
            release_acquired(&parts, acquired, query.as_deref()).await;
            let (error, challenges) = upstream_generic_error(answer, "the lock").await;
            let refused = refused_lock(status, error, breaks.then_some(granted));
            return with_challenges(refused, challenges);
        }
        let Some((id, lock)) = parse_body::<Lock>(answer)
            .await
            .and_then(|lock| Some((lock.id.clone()?, lock)))
        else {
            let created = status == StatusCode::CREATED;
            if created {
                release_unnamed(upstream, &parts, query.as_deref()).await;
            }
            // one that created a lock also granted `break_lock`
            let granted = acquired.len().saturating_add(usize::from(created));
            release_acquired(&parts, acquired, query.as_deref()).await;
            let (status, error) = invalid_lock();
            return refused_lock(status, error, breaks.then_some(granted));
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

    let parts_ids: Vec<_> = ids.iter().map(|id| Some(id.as_str())).collect();
    let id = lock_id(&parts_ids);
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

/// The answer to a lock that `error` refused with `status`. If the request
/// asked to break locks, `granted` upstreams granted that before, and each may
/// have broken the lock of another client, which nothing restores: the client
/// learns how many there were.
fn refused_lock(status: StatusCode, mut error: GenericError, granted: Option<usize>) -> Response {
    if let Some(granted) = granted.filter(|granted| *granted > 0) {
        error
            .parameters
            .get_or_insert_default()
            .insert(BREAK_LOCK_GRANTED.to_owned(), Value::from(granted));
    }
    json_response(status, &error)
}

/// Whether the request `body` asks to break the locks of others. An upstream
/// judges its value, so only `true` counts here, as it does for a CDA.
fn breaks_locks(body: &Bytes) -> bool {
    let fields: Map<String, Value> = serde_json::from_slice(body).unwrap_or_default();
    fields
        .iter()
        .any(|(name, value)| name.eq_ignore_ascii_case(BREAK_LOCK) && value.as_bool() == Some(true))
}

/// The locks of all upstreams, by what each lists to the client: the parts
/// the client owns on all upstreams once, as its lock, and every other lock of
/// an upstream, such as one acquired from it directly or one of another client,
/// as a lock of a single part. Only the upstreams know who owns a lock.
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

    let mut items = Vec::new();
    let own = own_lock(&lists);
    if let Some(own) = &own {
        let seen = lists
            .iter()
            .zip(own)
            .filter_map(|(list, part)| list.iter().find(|lock| lock.id.as_deref() == Some(part)))
            .cloned()
            .collect();
        let mut item = merge_parts(seen);
        let own: Vec<_> = own.iter().map(|part| Some(part.as_str())).collect();
        item.id = Some(lock_id(&own));
        items.push(item);
    }
    let count = lists.len();
    for (index, list) in lists.into_iter().enumerate() {
        let part_of_own = |lock: &Lock| {
            own.as_ref()
                .and_then(|own| own.get(index))
                .is_some_and(|part| lock.id.as_ref() == Some(part))
        };
        for mut lock in list.into_iter().filter(|lock| !part_of_own(lock)) {
            // A lock without an id cannot be asked for, and stays as it is.
            if let Some(part) = lock.id.take() {
                let single: Vec<_> = (0..count)
                    .map(|at| (at == index).then_some(part.as_str()))
                    .collect();
                lock.id = Some(lock_id(&single));
            }
            items.push(lock);
        }
    }
    let list = Collection {
        items,
        schema: None,
    };
    json_response(StatusCode::OK, &list)
}

/// The ids of the parts of the client's lock in `lists`, one owned lock on
/// each upstream, or nothing if an upstream lists none or more than one.
fn own_lock(lists: &[Vec<Lock>]) -> Option<Vec<String>> {
    lists
        .iter()
        .map(|list| {
            let mut owned = list.iter().filter(|lock| lock.owned == Some(true));
            match (owned.next(), owned.next()) {
                (Some(lock), None) => lock.id.clone(),
                _ => None,
            }
        })
        .collect()
}

/// The lock as all upstreams see it. If one has lost its part, the lock does
/// not hold anymore, whatever the others say, and if all have, it is over.
/// Asking ends nothing: the parts left stay until the client deletes the lock.
async fn lock_details(forward: &Federation, request: Request, id: &str) -> Response {
    let path = api_uri(request.extensions()).path().to_owned();
    let query = request_query(&request);
    let Some(targets) = part_uris(forward, id, query.as_deref()) else {
        return not_found_error(&path);
    };
    let (parts, _) = request.into_parts();
    let answers = join_all(targets.iter().map(|(upstream, uri)| {
        upstream.request_with_client_token(&parts, Method::GET, uri.clone())
    }))
    .await;

    if all_gone(&answers) {
        return not_found_error(&path);
    }
    if answers.iter().any(|a| a.status() == StatusCode::NOT_FOUND) {
        return lost_part_error(id);
    }
    match read_merged_lock(answers, id).await {
        // ISO 17978-3 Table 248 has no id: the path names the lock.
        Ok(lock) => json_response(StatusCode::OK, &Lock { id: None, ..lock }),
        Err(answer) => *answer,
    }
}

async fn modify_expiration(forward: &Federation, request: Request, id: &str) -> Response {
    let path = api_uri(request.extensions()).path().to_owned();
    let query = request_query(&request);
    let Some(targets) = part_uris(forward, id, query.as_deref()) else {
        return not_found_error(&path);
    };
    let (parts, body) = request.into_parts();
    let Ok(body) = read_body(body).await else {
        return too_large_error();
    };
    let answers = join_all(targets.iter().map(|(upstream, uri)| {
        upstream.send_to(
            parts.clone(),
            Body::new(Full::new(body.clone())),
            uri.clone(),
        )
    }))
    .await;

    if all_gone(&answers) {
        return not_found_error(&path);
    }
    if answers.iter().any(|a| a.status() == StatusCode::NOT_FOUND) {
        return lost_part_error(id);
    }
    match first_failure_or_first(answers) {
        Some(answer) => gateway_answer(answer, id).await,
        None => not_found_error(&path),
    }
}

/// Releases all parts. One that is gone already is released, and the first
/// refusal of an upstream is the answer.
async fn release_lock(forward: &Federation, request: Request, id: &str) -> Response {
    let path = api_uri(request.extensions()).path().to_owned();
    let query = request_query(&request);
    let Some(targets) = part_uris(forward, id, query.as_deref()) else {
        return not_found_error(&path);
    };
    let (parts, _) = request.into_parts();
    // A DELETE has no body here, so none of the client's framing may come with it.
    let answers = join_all(targets.iter().map(|(upstream, uri)| {
        upstream.request_with_client_token(&parts, Method::DELETE, uri.clone())
    }))
    .await;

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

/// Whether all parts of a lock are gone, by the `answers` about them: the lock
/// is over, as after it expired.
fn all_gone(answers: &[Response]) -> bool {
    answers
        .iter()
        .all(|answer| answer.status() == StatusCode::NOT_FOUND)
}

/// The first answer that is no success, or else the first answer.
fn first_failure_or_first(answers: Vec<Response>) -> Option<Response> {
    let at = answers
        .iter()
        .position(|answer| !answer.status().is_success())
        .unwrap_or_default();
    answers.into_iter().nth(at)
}

/// Answers that the lock `id` does not hold anymore, as a part of it is gone.
/// The parts left stay until the client's `DELETE` releases them.
fn lost_part_error(id: &str) -> Response {
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

/// Releases the lock that `upstream` granted the client of `parts` with an
/// answer that does not name it. A client owns one lock per upstream at most,
/// so it is the one the upstream lists as the client's.
async fn release_unnamed(upstream: &Upstream, parts: &Parts, query: Option<&str>) {
    let list = Uri::from_static("/locks");
    let answer = upstream
        .request_with_client_token(parts, Method::GET, list)
        .await;
    let owned = if answer.status().is_success() {
        parse_body::<Collection<Lock>>(answer)
            .await
            .and_then(|list| own_lock(&[list.items]))
            .and_then(|mut own| own.pop())
    } else {
        None
    };
    if let Some(id) = owned {
        release_acquired(parts, vec![(upstream.clone(), id)], query).await;
    } else {
        tracing::warn!(target: TARGET, "Upstream kept a lock it granted without naming it");
    }
}

/// The upstreams of the lock `id` with the URIs of their parts, or nothing if
/// `id` is no lock of the server.
fn part_uris<'a>(
    forward: &'a Federation,
    id: &str,
    query: Option<&str>,
) -> Option<Vec<(&'a Upstream, Uri)>> {
    lock_parts(forward, id)?
        .into_iter()
        .map(|(upstream, part)| Some((upstream, upstream_lock_uri(&part, query)?)))
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
    let (status, error) = invalid_lock();
    json_response(status, &error)
}

fn invalid_lock() -> (StatusCode, GenericError) {
    (
        StatusCode::BAD_GATEWAY,
        GenericError::with_vendor_code("upstream-invalid-lock", "invalid lock of an upstream"),
    )
}

fn too_large_error() -> Response {
    vendor_error(
        StatusCode::PAYLOAD_TOO_LARGE,
        "request-too-large",
        "The request is too large",
    )
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
    let (error, challenges) = upstream_generic_error(answer, about).await;
    with_challenges(json_response(status, &error), challenges)
}

/// `answer` with the authentication `challenges` of an upstream, which tell a
/// client refused for its token how to authenticate (RFC 9110 11.6.1).
fn with_challenges(mut answer: Response, challenges: Vec<HeaderValue>) -> Response {
    for challenge in challenges {
        answer.headers_mut().append(WWW_AUTHENTICATE, challenge);
    }
    answer
}

/// The server's error for an upstream's `answer` that refused `about`, see
/// [`upstream_error`], with the answer's authentication challenges.
async fn upstream_generic_error(answer: Response, about: &str) -> (GenericError, Vec<HeaderValue>) {
    let challenges = answer
        .headers()
        .get_all(WWW_AUTHENTICATE)
        .iter()
        .cloned()
        .collect();
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
    (error, challenges)
}

/// What the upstreams say about the parts of one lock: it expires with the
/// first of them, is owned if the upstreams that tell say so, and the first
/// upstream speaks for the rest. `owned` is optional (ISO 17978-3 Table
/// 245), so an upstream without it says nothing about the owner.
fn merge_parts(locks: Vec<Lock>) -> Lock {
    let lock_expiration = locks.iter().filter_map(|lock| lock.lock_expiration).min();
    let reports_owner = locks.iter().any(|lock| lock.owned.is_some());
    let owned = locks.iter().all(|lock| lock.owned != Some(false));

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

#[cfg(test)]
mod tests {
    use super::*;

    fn lock(owned: Option<bool>) -> Lock {
        Lock {
            owned,
            ..Lock::default()
        }
    }

    #[test]
    fn a_part_that_does_not_tell_its_owner_leaves_it_to_the_others() {
        let owned =
            |parts: &[Option<bool>]| merge_parts(parts.iter().copied().map(lock).collect()).owned;

        assert_eq!(owned(&[Some(true), None]), Some(true));
        assert_eq!(owned(&[None, Some(true)]), Some(true));
        assert_eq!(owned(&[Some(true), Some(false)]), Some(false));
        assert_eq!(owned(&[None, Some(false)]), Some(false));
        assert_eq!(owned(&[None, None]), None);
    }
}
