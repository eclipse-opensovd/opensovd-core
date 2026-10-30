// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Routes that are not served yet, relative to the entity. Each answers
//! `resource-not-found` for an existing entity, like a path no route
//! serves. Replace a router here with a real one as it lands.

use axum::Router;
use axum::routing::{get, post, put};
use opensovd_core::Topology;

use super::entity::stub;

pub(super) fn auth() -> Router<Topology> {
    Router::new()
        .route("/authorize", post(stub))
        .route("/token", post(stub))
}

pub(super) fn clear_data() -> Router<Topology> {
    Router::new()
        .route("/clear-data", get(stub))
        .route("/clear-data/cached-data", put(stub))
        .route("/clear-data/learned-data", put(stub))
        .route("/clear-data/client-defined-resources", put(stub))
        .route("/clear-data/status", get(stub))
}

pub(super) fn communication_logs() -> Router<Topology> {
    Router::new()
        .route("/communication-logs", get(stub).post(stub))
        .route(
            "/communication-logs/{communication_log_id}",
            get(stub).put(stub).delete(stub),
        )
}

pub(super) fn configurations() -> Router<Topology> {
    Router::new()
        .route("/configurations", get(stub).delete(stub))
        .route(
            "/configurations/{configuration_id}",
            get(stub).put(stub).delete(stub),
        )
}

pub(super) fn cyclic_subscriptions() -> Router<Topology> {
    Router::new()
        .route("/cyclic-subscriptions", get(stub).post(stub))
        .route(
            "/cyclic-subscriptions/{subscription_id}",
            get(stub).put(stub).delete(stub),
        )
}

pub(super) fn data_lists() -> Router<Topology> {
    Router::new()
        .route("/data-lists", get(stub).post(stub))
        .route("/data-lists/{data_list_id}", get(stub).delete(stub))
}

pub(super) fn faults() -> Router<Topology> {
    Router::new()
        .route("/faults", get(stub).delete(stub))
        .route("/faults/{fault_code}", get(stub).delete(stub))
}

pub(super) fn locks() -> Router<Topology> {
    Router::new()
        .route("/locks", get(stub).post(stub))
        .route("/locks/{lock_id}", get(stub).put(stub).delete(stub))
}

pub(super) fn modes() -> Router<Topology> {
    Router::new()
        .route("/modes", get(stub))
        .route("/modes/{mode_id}", get(stub).put(stub))
}

pub(super) fn operations() -> Router<Topology> {
    Router::new()
        .route("/operations", get(stub))
        .route("/operations/{operation_id}", get(stub))
        .route(
            "/operations/{operation_id}/executions",
            get(stub).post(stub),
        )
        .route(
            "/operations/{operation_id}/executions/{execution_id}",
            get(stub).put(stub).delete(stub),
        )
}

pub(super) fn scripts() -> Router<Topology> {
    Router::new()
        .route("/scripts", get(stub).post(stub))
        .route("/scripts/{script_id}", get(stub).delete(stub))
        .route("/scripts/{script_id}/executions", get(stub).post(stub))
        .route(
            "/scripts/{script_id}/executions/{execution_id}",
            get(stub).put(stub).delete(stub),
        )
}

/// Starting, restarting and shutting down an entity.
pub(super) fn status() -> Router<Topology> {
    Router::new()
        .route("/status", get(stub))
        .route("/status/start", put(stub))
        .route("/status/restart", put(stub))
        .route("/status/force-restart", put(stub))
        .route("/status/shutdown", put(stub))
        .route("/status/force-shutdown", put(stub))
}

pub(super) fn triggers() -> Router<Topology> {
    Router::new()
        .route("/triggers", get(stub).post(stub))
        .route("/triggers/{trigger_id}", get(stub).put(stub).delete(stub))
}
