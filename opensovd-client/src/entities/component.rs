// SPDX-FileCopyrightText: 2026 Copyright (c) Contributors to the Eclipse Foundation
//
// See the NOTICE file(s) distributed with this work for additional
// information regarding copyright ownership.
//
// This program and the accompanying materials are made available under the
// terms of the Apache License Version 2.0 which is available at
// https://www.apache.org/licenses/LICENSE-2.0
//
// SPDX-License-Identifier: Apache-2.0

use opensovd_models::data::{DataCategories, DataGroups};
use opensovd_models::discovery::{Entities, EntityCapabilities};

use crate::client::{Client, encode};
use crate::data::{DataRequest, ListDataRequest};
use crate::error::Result;

/// A reference to a specific component.
pub struct Component<'a> {
    pub(crate) client: &'a Client,
    pub(crate) id: String,
}

impl Component<'_> {
    /// Returns a request builder for listing data items on this entity.
    #[must_use]
    pub fn list_data(&self) -> ListDataRequest<'_> {
        ListDataRequest {
            client: self.client,
            path: format!("/components/{}/data", self.id),
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
            path: format!("/components/{}/data/{}", self.id, encode(data_id)),
        }
    }

    /// Fetch data categories for this entity.
    pub async fn data_categories(&self) -> Result<DataCategories> {
        self.client
            .get(&format!("/components/{}/data-categories", self.id), &[])
            .await
    }

    /// Fetch data groups for this entity.
    pub async fn data_groups(&self) -> Result<DataGroups> {
        self.client
            .get(&format!("/components/{}/data-groups", self.id), &[])
            .await
    }

    /// Fetch apps hosted on this component.
    pub async fn hosts(&self) -> Result<Entities> {
        self.client
            .get(&format!("/components/{}/hosts", self.id), &[])
            .await
    }

    /// Get the area this component belongs to by following its advertised
    /// `belongs-to` link, or `None` when it advertises none.
    pub async fn belongs_to(&self) -> Result<Option<EntityCapabilities>> {
        let capabilities: EntityCapabilities = self
            .client
            .get(&format!("/components/{}", self.id), &[])
            .await?;
        let Some(href) = capabilities.belongs_to else {
            return Ok(None);
        };
        self.client.follow(&href).await.map(Some)
    }
}
