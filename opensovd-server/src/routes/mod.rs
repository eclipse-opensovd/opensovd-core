// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! HTTP route handlers for the SOVD API.
//!
//! This module provides all SOVD-compliant REST endpoints:
//!
//! ## Discovery
//! - GET / - Query capabilities of the root entity
//! - GET /components - List all components
//! - GET /components/{component_id} - Query capabilities of a component
//!
//! ## Bulkdata
//! - GET    /{entity-collection}/{entity-id}/bulk-data - List bulk-data categories
//! - GET    /{entity-collection}/{entity-id}/bulk-data/{category} - List of BulkDataDescriptors for a specific category
//! - POST   /{entity-collection}/{entity-id}/bulk-data/{category} - Upload bulk data to the SOVD server and create a resource for the bulk data
//! - DELETE /{entity-collection}/{entity-id}/bulk-data/{category} - Delete all bulk data resources for a specific category
//! - GET    /{entity-collection}/{entity-id}/bulk-data/{category}/{bulk-data-id} - Download a specific bulk data resource
//! - DELETE /{entity-collection}/{entity-id}/bulk-data/{category}/{bulk-data-id} - Delete a specific bulk data resource
//!
//! ## Data
//! - GET /components/{component_id}/data-categories - List data categories
//! - GET /components/{component_id}/data-groups - List data groups
//! - GET /components/{component_id}/data - List data resources
//! - GET /components/{component_id}/data/{data_id} - Read a data value
//! - PUT /components/{component_id}/data/{data_id} - Write a data value
//!
//! ## Version
//! - GET /version-info - Get SOVD server version information

mod bulk_data;
mod data;
mod discovery;
mod error;
mod log;
mod version;

use axum::{
    Router,
    extract::FromRef,
    http::{Uri, request::Parts},
};
use http::header::HOST;
use opensovd_core::Topology;
pub use opensovd_models::version::{VendorInfo, VersionInfo};
use serde::Serialize;

use crate::schema::JsonSchema;

#[derive(Clone)]
pub struct AppState<V> {
    pub vendor_info: Option<V>,
    pub topology: Topology,
}

impl<V> FromRef<AppState<V>> for Topology {
    fn from_ref(state: &AppState<V>) -> Topology {
        state.topology.clone()
    }
}

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

pub(crate) fn base_uri(parts: &Parts) -> String {
    let (scheme, path) = parts
        .extensions
        .get::<BaseUri>()
        .map_or(("http", ""), |b| (b.scheme.as_str(), b.path.as_str()));
    let host = parts
        .headers
        .get(HOST)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("localhost");
    format!("{scheme}://{host}{path}")
}

pub(crate) fn versioned_uri(parts: &Parts) -> String {
    format!("{}/{API_VERSION}", base_uri(parts))
}

/// The API routes. The caller layers the [`BaseUri`] extension.
pub fn router<V>(vendor_info: Option<V>, topology: Topology) -> Router
where
    V: Serialize + Clone + Send + Sync + 'static,
    VersionInfo<V>: JsonSchema,
{
    let state = AppState {
        vendor_info,
        topology,
    };

    let v1_routes = Router::new()
        .merge(discovery::routes::<V>())
        .merge(bulk_data::routes::<V>())
        .merge(data::routes::<V>())
        .merge(log::routes::<V>());

    let router = Router::new()
        .nest(&format!("/{API_VERSION}"), v1_routes)
        .merge(version::routes::<V>());

    router.with_state(state)
}

/// Answers a path no route serves with a `GenericError` body.
pub(crate) async fn not_found(uri: Uri) -> error::Error {
    error::Error::ResourceNotFound(uri.path().to_string())
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
