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

mod bulkdata;
mod data;
mod entities;
mod error;
mod log;
mod version;

use axum::{
    Extension, Router,
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

impl Default for BaseUri {
    fn default() -> Self {
        Self {
            scheme: "http".to_string(),
            path: String::new(),
        }
    }
}

/// Mirrors the default gateway deployment for route tests.
#[cfg(test)]
pub(crate) fn test_base_uri() -> Extension<BaseUri> {
    Extension(BaseUri {
        scheme: "http".to_string(),
        path: "/sovd".to_string(),
    })
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

pub fn router<V>(vendor_info: Option<V>, topology: Topology, base_uri: BaseUri) -> Router
where
    V: Serialize + Clone + Send + Sync + 'static,
    VersionInfo<V>: JsonSchema,
{
    let state = AppState {
        vendor_info,
        topology,
    };

    let v1_routes = Router::new()
        .merge(entities::routes::<V>())
        .merge(bulkdata::routes::<V>())
        .merge(data::routes::<V>())
        .merge(log::routes::<V>());

    let router = Router::new()
        .nest(&format!("/{API_VERSION}"), v1_routes)
        .merge(version::routes::<V>());

    router.with_state(state).layer(Extension(base_uri))
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

    use super::{BaseUri, router};

    #[tokio::test]
    async fn malformed_path_parameters_are_rejected_as_json() {
        let base_uri = BaseUri {
            scheme: "http".to_owned(),
            path: String::new(),
        };
        let app = router::<()>(None, create_mock_topology().await, base_uri);

        for (method, path) in [
            ("GET", "/v1/areas/%FF"),
            ("GET", "/v1/areas/%FF/contains"),
            ("GET", "/v1/apps/%FF"),
            ("GET", "/v1/components/%FF"),
            ("GET", "/v1/components/%FF/hosts"),
            ("GET", "/v1/components/%FF/data-categories"),
            ("GET", "/v1/components/%FF/data-groups"),
            ("GET", "/v1/components/%FF/data"),
            ("GET", "/v1/components/%FF/data/x"),
            ("PUT", "/v1/components/%FF/data/x"),
            ("GET", "/v1/apps/%FF/data-categories"),
            ("GET", "/v1/apps/%FF/data-groups"),
            ("GET", "/v1/apps/%FF/data"),
            ("GET", "/v1/apps/%FF/data/x"),
            ("PUT", "/v1/apps/%FF/data/x"),
            ("GET", "/v1/components/%FF/bulk-data"),
            ("GET", "/v1/components/%FF/bulk-data/c"),
            ("POST", "/v1/components/%FF/bulk-data/c"),
            ("DELETE", "/v1/components/%FF/bulk-data/c"),
            ("GET", "/v1/components/%FF/bulk-data/c/x"),
            ("DELETE", "/v1/components/%FF/bulk-data/c/x"),
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
        let base_uri = BaseUri {
            scheme: "http".to_owned(),
            path: String::new(),
        };
        let app = router::<()>(None, create_mock_topology().await, base_uri);

        for (path, param) in [
            ("/v1/components/%FF", "component_id"),
            ("/v1/components/ecu/data/%FF", "data_id"),
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
