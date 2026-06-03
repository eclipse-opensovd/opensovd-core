// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

use crate::client::{Client, encode};
use crate::data::{DataRequest, ListDataRequest};
use crate::error::Result;
use crate::models::{DefaultModels, Models};

/// A reference to a specific component.
pub struct Component<'a, M = DefaultModels> {
    pub(crate) client: &'a Client<M>,
    pub(crate) id: String,
}

impl<'a, M: Models> Component<'a, M> {
    /// Returns a request builder for listing data items on this entity.
    #[must_use]
    pub fn list_data(&self) -> ListDataRequest<'a, M> {
        ListDataRequest {
            client: self.client,
            path: format!("/components/{}/data", self.id),
            schema: false,
        }
    }

    /// Returns a reference to a specific data item on this entity.
    #[must_use]
    pub fn data(&self, data_id: &str) -> DataRequest<'a, M> {
        DataRequest {
            client: self.client,
            path: format!("/components/{}/data/{}", self.id, encode(data_id)),
        }
    }

    /// Fetch data categories for this entity.
    pub async fn data_categories(&self) -> Result<M::DataCategoriesResponse> {
        self.client
            .get(&format!("/components/{}/data-categories", self.id), &[])
            .await
    }

    /// Fetch data groups for this entity.
    pub async fn data_groups(&self) -> Result<M::DataGroupsResponse> {
        self.client
            .get(&format!("/components/{}/data-groups", self.id), &[])
            .await
    }

    /// Fetch apps hosted on this component.
    pub async fn hosts(&self) -> Result<M::EntitiesResponse> {
        self.client
            .get(&format!("/components/{}/hosts", self.id), &[])
            .await
    }

    /// List areas this component belongs to.
    pub async fn belongs_to(&self) -> Result<M::EntitiesResponse> {
        self.client
            .get(&format!("/components/{}/belongs-to", self.id), &[])
            .await
    }
}
