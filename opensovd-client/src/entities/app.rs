// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

use opensovd_models::data::{DataCategories, DataGroups};
use opensovd_models::discovery::Entities;

use crate::client::{Client, encode};
use crate::data::{DataRequest, ListDataRequest};
use crate::error::Result;

/// A reference to a specific app.
pub struct App<'a> {
    pub(crate) client: &'a Client,
    pub(crate) id: String,
}

impl App<'_> {
    /// Returns a request builder for listing data items on this entity.
    #[must_use]
    pub fn list_data(&self) -> ListDataRequest<'_> {
        ListDataRequest {
            client: self.client,
            path: format!("/apps/{}/data", self.id),
            schema: false,
            groups: Vec::new(),
            categories: Vec::new(),
            tags: Vec::new(),
        }
    }

    /// Returns a reference to a specific data item on this entity.
    #[must_use]
    pub fn data(&self, data_id: &str) -> DataRequest<'_> {
        DataRequest {
            client: self.client,
            path: format!("/apps/{}/data/{}", self.id, encode(data_id)),
        }
    }

    /// Fetch data categories for this entity.
    pub async fn data_categories(&self) -> Result<DataCategories> {
        self.client
            .get(&format!("/apps/{}/data-categories", self.id), &[])
            .await
    }

    /// Fetch data groups for this entity.
    pub async fn data_groups(&self) -> Result<DataGroups> {
        self.client
            .get(&format!("/apps/{}/data-groups", self.id), &[])
            .await
    }

    /// Get the component this app is located on.
    pub async fn is_located_on(&self) -> Result<Entities> {
        self.client
            .get(&format!("/apps/{}/is-located-on", self.id), &[])
            .await
    }

    /// List areas this app belongs to.
    pub async fn belongs_to(&self) -> Result<Entities> {
        self.client
            .get(&format!("/apps/{}/belongs-to", self.id), &[])
            .await
    }
}
