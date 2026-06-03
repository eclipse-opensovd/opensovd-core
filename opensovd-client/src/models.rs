// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Pluggable response model set.
//!
//! [`Models`] names the deserializable POD type returned by each endpoint.
//! Implement it with your own types to target a server whose wire shapes
//! differ from [`DefaultModels`] (e.g. a classic-diagnostic-adapter).

use opensovd_models::{Response, data, discovery};
use serde::de::DeserializeOwned;

/// Response POD types a SOVD server speaks.
pub trait Models {
    /// Entity collections: `/components`, `/apps`, `/areas`, and the
    /// `hosts` / `belongs-to` / `is-located-on` / `contains` relationships.
    type Entities: DeserializeOwned;
    /// `GET /.../data`.
    type DataList: DeserializeOwned;
    /// `GET /.../data/{id}`.
    type ReadResponse: DeserializeOwned;
    /// `GET /.../data-categories`.
    type DataCategories: DeserializeOwned;
    /// `GET /.../data-groups`.
    type DataGroups: DeserializeOwned;
}

/// Default model set backed by `opensovd-models`.
pub struct DefaultModels;

impl Models for DefaultModels {
    type Entities = Response<discovery::Entities>;
    type DataList = Response<data::DataList>;
    type ReadResponse = data::ReadResponse;
    type DataCategories = data::DataCategories;
    type DataGroups = data::DataGroups;
}
