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
//! ## Data
//! - GET /components/{component_id}/data-categories - List data categories
//! - GET /components/{component_id}/data-groups - List data groups
//! - GET /components/{component_id}/data - List data resources
//! - GET /components/{component_id}/data/{data_id} - Read a data value
//! - PUT /components/{component_id}/data/{data_id} - Write a data value
//!
//! ## Version
//! - GET /version-info - Get SOVD server version information

mod data;
mod entities;
mod error;
mod version;

use axum::{Extension, Router, extract::FromRef, http::request::Parts};
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
pub const SOVD_VERSION: &str = "1.1";

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
        .merge(data::routes::<V>());

    let router = Router::new()
        .nest(&format!("/{API_VERSION}"), v1_routes)
        .merge(version::routes::<V>());

    router.with_state(state).layer(Extension(base_uri))
}
