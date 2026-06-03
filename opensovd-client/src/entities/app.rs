// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

use crate::client::{Client, encode};
use crate::data::{DataRequest, ListDataRequest};
use crate::error::Result;
use crate::models::{DefaultModels, Models};

/// A reference to a specific app.
pub struct App<'a, M = DefaultModels> {
    pub(crate) client: &'a Client<M>,
    pub(crate) id: String,
}

impl<'a, M: Models> App<'a, M> {
    /// Returns a request builder for listing data items on this entity.
    #[must_use]
    pub fn list_data(&self) -> ListDataRequest<'a, M> {
        ListDataRequest {
            client: self.client,
            path: format!("/apps/{}/data", self.id),
            schema: false,
        }
    }

    /// Returns a reference to a specific data item on this entity.
    #[must_use]
    pub fn data(&self, data_id: &str) -> DataRequest<'a, M> {
        DataRequest {
            client: self.client,
            path: format!("/apps/{}/data/{}", self.id, encode(data_id)),
        }
    }

    /// Fetch data categories for this entity.
    pub async fn data_categories(&self) -> Result<M::DataCategoriesResponse> {
        self.client
            .get(&format!("/apps/{}/data-categories", self.id), &[])
            .await
    }

    /// Fetch data groups for this entity.
    pub async fn data_groups(&self) -> Result<M::DataGroupsResponse> {
        self.client
            .get(&format!("/apps/{}/data-groups", self.id), &[])
            .await
    }

    /// Get the component this app is located on.
    pub async fn is_located_on(&self) -> Result<M::EntitiesResponse> {
        self.client
            .get(&format!("/apps/{}/is-located-on", self.id), &[])
            .await
    }

    /// List areas this app belongs to.
    pub async fn belongs_to(&self) -> Result<M::EntitiesResponse> {
        self.client
            .get(&format!("/apps/{}/belongs-to", self.id), &[])
            .await
    }
}
