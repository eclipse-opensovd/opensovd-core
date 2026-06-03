// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Pluggable model set.
//!
//! [`Models`] splits the wire contract into two halves:
//! - **PODs** — the leaf item types (`EntityRef`, `DataInfo`, ...), which is
//!   where servers usually differ.
//! - **`*Response` envelopes** — the full response each endpoint returns,
//!   typically `Response<Items<Pod>>`.
//!
//! A consumer overrides only the PODs (reusing the default envelopes via
//! `Self::EntityRef` etc.) or swaps a whole envelope when its shape differs.
//! Implement it with your own deserializable types to target a server whose
//! wire shapes differ from [`DefaultModels`] (e.g. a classic-diagnostic-adapter).

use opensovd_models::{Items, Response, data, discovery};
use serde::de::DeserializeOwned;

/// The POD and response types a SOVD server speaks.
pub trait Models {
    // ---- PODs (leaf items) ------------------------------------------------
    /// Item in entity collections.
    type EntityRef: DeserializeOwned;
    /// Item in `/data`.
    type DataInfo: DeserializeOwned;
    /// Item in `/data-categories`.
    type DataCategory: DeserializeOwned;
    /// Item in `/data-groups`.
    type DataGroup: DeserializeOwned;

    // ---- Response envelopes -----------------------------------------------
    /// `/components`, `/apps`, `/areas`, and the `hosts` / `belongs-to` /
    /// `is-located-on` / `contains` relationships.
    type EntitiesResponse: DeserializeOwned;
    /// `GET /.../data`.
    type DataListResponse: DeserializeOwned;
    /// `GET /.../data-categories`.
    type DataCategoriesResponse: DeserializeOwned;
    /// `GET /.../data-groups`.
    type DataGroupsResponse: DeserializeOwned;
    /// `GET /.../data/{id}`.
    type ReadResponse: DeserializeOwned;
}

/// Default model set backed by `opensovd-models`.
pub struct DefaultModels;

impl Models for DefaultModels {
    type EntityRef = discovery::EntityReference;
    type DataInfo = data::Metadata;
    type DataCategory = data::DataCategoryInformation;
    type DataGroup = data::Group;

    type EntitiesResponse = Response<Items<Self::EntityRef>>;
    type DataListResponse = Response<Items<Self::DataInfo>>;
    type DataCategoriesResponse = Response<Items<Self::DataCategory>>;
    type DataGroupsResponse = Response<Items<Self::DataGroup>>;
    type ReadResponse = data::ReadResponse;
}
