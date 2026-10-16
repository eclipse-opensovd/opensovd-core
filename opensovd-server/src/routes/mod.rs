// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! HTTP route handlers for the SOVD API.
//!
//! The route table has two parts: one router per entity kind says what the
//! kind serves, relative to the entity, and [`versioned`] nests each kind at
//! every level it is served on, such as `/components/{component_id}` and
//! `/components/{component_id}/subcomponents/{subcomponent_id}`, where the
//! sub-entity listing is left out. Handlers take the entity from the route,
//! so one handler serves all levels. Routes that are not served yet answer
//! `resource-not-found` for an existing entity.
//!
//! A request is checked in this order:
//! - the route first: an unknown path answers 404 `resource-not-found`, a
//!   method the route does not serve 405 `method-not-allowed`;
//! - then the request's form: path params, query, headers and body (400, 415);
//! - then the entity (404 `entity-not-found`);
//! - then the resource (404 `resource-not-found`);
//! - then the operation.
//!
//! - `entity`: the `Entity` extractor and the stub answer
//! - `discovery`: capabilities, listings and relations
//! - `data`, `bulk_data`, `log`: the served resource collections
//! - `stubs`: the routers of collections that are not served yet
//! - `version`: `GET /version-info`, outside the versioned API

mod bulk_data;
mod data;
mod discovery;
mod entity;
mod error;
mod log;
mod stubs;
mod version;

use axum::{
    Router,
    extract::OriginalUri,
    http::{Uri, request::Parts},
    routing::get,
};
use discovery::{app, area, component};
use entity::stub;
use http::header::HOST;
use opensovd_core::Topology;
pub use opensovd_models::version::{VendorInfo, VersionInfo};
use serde::Serialize;

use crate::schema::JsonSchema;

const API_VERSION: &str = "v1";

/// SOVD standard version.
pub const SOVD_VERSION: &str = "1.1.0";

/// Scheme and mount path the server advertises, resolved from its configuration
/// and attached to every request as an extension.
#[derive(Clone, Debug)]
pub(crate) struct BaseUri {
    pub scheme: String,
    /// Normalized mount path: leading `/`, or empty when mounted at the root.
    pub path: String,
}

/// The API over `topology`, mounted and advertised at `/sovd` like the
/// default gateway deployment, for route tests.
#[cfg(test)]
pub(crate) fn test_router(topology: Topology) -> Router {
    let base_uri = BaseUri {
        scheme: "http".to_string(),
        path: "/sovd".to_string(),
    };
    Router::new()
        .nest("/sovd", router::<()>(None, topology))
        .fallback(not_found)
        .layer(axum::Extension(base_uri))
}

/// The advertised scheme and mount path: `http` and empty without a
/// [`BaseUri`].
fn advertised(parts: &Parts) -> (&str, &str) {
    parts
        .extensions
        .get::<BaseUri>()
        .map_or(("http", ""), |b| (b.scheme.as_str(), b.path.as_str()))
}

pub(crate) fn versioned_uri(parts: &Parts) -> String {
    let (scheme, path) = advertised(parts);
    let host = parts
        .headers
        .get(HOST)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("localhost");
    format!("{scheme}://{host}{path}/{API_VERSION}")
}

/// The request URI before nesting stripped a prefix.
pub(crate) fn original_uri(parts: &Parts) -> &Uri {
    parts
        .extensions
        .get::<OriginalUri>()
        .map_or(&parts.uri, |OriginalUri(uri)| uri)
}

/// The request path below the API version, such as `components/ecu/faults`,
/// for error messages. A path outside the API stays whole.
pub(crate) fn api_path(parts: &Parts) -> &str {
    let path = original_uri(parts).path();
    let (_, base) = advertised(parts);
    path.strip_prefix(base)
        .and_then(|path| path.strip_prefix('/'))
        .and_then(|path| path.strip_prefix(API_VERSION))
        .and_then(|path| path.strip_prefix('/'))
        .filter(|path| !path.is_empty())
        .unwrap_or(path)
}

/// The API routes. The caller layers the [`BaseUri`] extension.
pub fn router<V>(vendor_info: Option<V>, topology: Topology) -> Router
where
    V: Serialize + Clone + Send + Sync + 'static,
    VersionInfo<V>: JsonSchema,
{
    Router::new()
        .nest(&format!("/{API_VERSION}"), versioned().with_state(topology))
        .merge(version::routes::<V>())
        .method_not_allowed_fallback(method_not_allowed)
        .with_state(vendor_info)
}

/// The routes below `/{API_VERSION}`: each entity kind at every level it is
/// served on, one level below the top-level areas and components.
fn versioned() -> Router<Topology> {
    Router::new()
        .merge(server())
        .nest("/areas/{area_id}", areas())
        .nest("/areas/{area_id}/subareas/{subarea_id}", subareas())
        .nest("/components/{component_id}", components())
        .nest(
            "/components/{component_id}/subcomponents/{subcomponent_id}",
            subcomponents(),
        )
        .nest("/apps/{app_id}", apps())
        .nest("/functions/{function_id}", functions())
}

/// Resource collections of the server, components and apps.
fn collections() -> Router<Topology> {
    Router::new()
        .merge(data::routes())
        .merge(bulk_data::routes())
        .merge(log::routes())
        .merge(stubs::clear_data())
        .merge(stubs::communication_logs())
        .merge(stubs::configurations())
        .merge(stubs::cyclic_subscriptions())
        .merge(stubs::data_lists())
        .merge(stubs::faults())
        .merge(stubs::locks())
        .merge(stubs::modes())
        .merge(stubs::operations())
        .merge(stubs::scripts())
        .merge(stubs::triggers())
        .merge(stubs::updates())
}

fn server() -> Router<Topology> {
    Router::new()
        .route("/", get(discovery::server_capabilities))
        .route("/areas", get(area::area_list))
        .route("/components", get(component::component_list))
        .route("/apps", get(app::app_list))
        .route("/functions", get(stub))
        .route("/depends-on", get(stub))
        .merge(stubs::auth())
        .merge(collections())
}

fn areas() -> Router<Topology> {
    subareas().route("/subareas", get(stub))
}

/// What a subarea serves: what an area serves but subareas, as entities
/// nest one level only.
fn subareas() -> Router<Topology> {
    Router::new()
        .route("/", get(area::area_capabilities))
        .route("/contains", get(area::area_contains))
}

fn components() -> Router<Topology> {
    subcomponents().route("/subcomponents", get(stub))
}

/// What a subcomponent serves: what a component serves but subcomponents.
fn subcomponents() -> Router<Topology> {
    Router::new()
        .route("/", get(component::component_capabilities))
        .route("/hosts", get(component::component_hosts))
        .route("/depends-on", get(stub))
        .merge(collections())
        .merge(stubs::status())
}

fn apps() -> Router<Topology> {
    Router::new()
        .route("/", get(app::app_capabilities))
        .route("/depends-on", get(stub))
        .merge(collections())
        .merge(stubs::status())
}

/// What a function serves: capabilities and depends-on. The topology holds
/// no functions, so both answer `resource-not-found` for any id.
fn functions() -> Router<Topology> {
    Router::new()
        .route("/", get(stub))
        .route("/depends-on", get(stub))
}

/// Answers a method the route does not serve with a `GenericError` body;
/// axum adds the `Allow` header with the methods of the route.
async fn method_not_allowed(parts: Parts) -> error::Error {
    error::Error::MethodNotAllowed(api_path(&parts).to_owned())
}

/// Answers a path no route serves with a `GenericError` body.
pub(crate) async fn not_found(parts: Parts) -> error::Error {
    error::Error::ResourceNotFound(api_path(&parts).to_owned())
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use opensovd_mocks::create_mock_topology;
    use tower::ServiceExt;

    use super::test_router;

    #[tokio::test]
    async fn malformed_path_parameters_are_rejected_as_json() {
        let app = test_router(create_mock_topology().await);

        for (method, path) in [
            ("GET", "/sovd/v1/areas/%FF"),
            ("GET", "/sovd/v1/areas/%FF/contains"),
            ("GET", "/sovd/v1/apps/%FF"),
            ("GET", "/sovd/v1/components/%FF"),
            ("GET", "/sovd/v1/components/%FF/hosts"),
            ("GET", "/sovd/v1/components/%FF/data-categories"),
            ("GET", "/sovd/v1/components/%FF/data-groups"),
            ("GET", "/sovd/v1/components/%FF/data"),
            ("GET", "/sovd/v1/components/%FF/data/x"),
            ("PUT", "/sovd/v1/components/%FF/data/x"),
            ("GET", "/sovd/v1/apps/%FF/data-categories"),
            ("GET", "/sovd/v1/apps/%FF/data-groups"),
            ("GET", "/sovd/v1/apps/%FF/data"),
            ("GET", "/sovd/v1/apps/%FF/data/x"),
            ("PUT", "/sovd/v1/apps/%FF/data/x"),
            ("GET", "/sovd/v1/components/%FF/bulk-data"),
            ("GET", "/sovd/v1/components/%FF/bulk-data/c"),
            ("POST", "/sovd/v1/components/%FF/bulk-data/c"),
            ("DELETE", "/sovd/v1/components/%FF/bulk-data/c"),
            ("GET", "/sovd/v1/components/%FF/bulk-data/c/x"),
            ("DELETE", "/sovd/v1/components/%FF/bulk-data/c/x"),
            ("GET", "/sovd/v1/apps/%FF/logs"),
            ("GET", "/sovd/v1/apps/%FF/logs/entries"),
            ("GET", "/sovd/v1/apps/%FF/logs/x-opensovd-log-streams"),
            ("GET", "/sovd/v1/apps/%FF/logs/config"),
            ("PUT", "/sovd/v1/apps/%FF/logs/config"),
            ("DELETE", "/sovd/v1/apps/%FF/logs/config"),
            ("GET", "/sovd/v1/components/ecu/data/%FF"),
            ("PUT", "/sovd/v1/apps/engine_control/data/%FF"),
            ("GET", "/sovd/v1/components/ecu/bulk-data/%FF"),
            ("GET", "/sovd/v1/apps/ota_manager/bulk-data/c/%FF"),
            ("GET", "/sovd/v1/components/%FF/faults"),
            ("PUT", "/sovd/v1/apps/%FF/status/restart"),
            ("GET", "/sovd/v1/areas/%FF/subareas"),
            ("GET", "/sovd/v1/components/ecu/faults/%FF"),
            ("GET", "/sovd/v1/components/ecu/subcomponents/%FF"),
            ("GET", "/sovd/v1/components/ecu/subcomponents/%FF/data"),
            ("GET", "/sovd/v1/areas/powertrain/subareas/%FF/contains"),
            ("GET", "/sovd/v1/functions/%FF"),
        ] {
            let request = Request::builder()
                .method(method)
                .uri(path)
                .body(Body::empty())
                .unwrap();
            let response = app.clone().oneshot(request).await.unwrap();
            assert_eq!(response.status(), 400, "{method} {path}");

            let body = response.into_body().collect().await.unwrap().to_bytes();
            let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(json["error_code"], "incomplete-request", "{method} {path}");
        }
    }

    #[tokio::test]
    async fn params_that_are_not_utf8_are_named() {
        let app = test_router(create_mock_topology().await);

        for (path, param) in [
            ("/sovd/v1/components/%FF", "component_id"),
            ("/sovd/v1/components/ecu/data/%FF", "data_id"),
        ] {
            let request = Request::builder().uri(path).body(Body::empty()).unwrap();
            let response = app.clone().oneshot(request).await.unwrap();
            assert_eq!(response.status(), 400, "{path}");

            let body = response.into_body().collect().await.unwrap().to_bytes();
            let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(
                json["message"],
                format!("Invalid UTF-8 in `{param}`"),
                "{path}"
            );
        }
    }
}
