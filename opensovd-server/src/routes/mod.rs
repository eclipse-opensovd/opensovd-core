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

use axum::{Router, extract::FromRef, http::request::Parts};
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

pub(crate) fn base_uri(parts: &Parts) -> String {
    let host = parts
        .headers
        .get(HOST)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("localhost");
    format!("http://{host}/sovd")
}

pub(crate) fn versioned_uri(parts: &Parts) -> String {
    format!("{}/{API_VERSION}", base_uri(parts))
}

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
        .merge(entities::routes::<V>())
        .merge(data::routes::<V>());

    let router = Router::new()
        .nest(&format!("/{API_VERSION}"), v1_routes)
        .merge(version::routes::<V>());

    router.with_state(state)
}
