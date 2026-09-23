// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

use opensovd_models::data::{DataCategories, DataGroups};
use opensovd_models::discovery::{Entities, EntityCapabilities};

use crate::capabilities::{CapabilitiesRequest, link, merge_link};
use crate::client::{Client, encode};
use crate::data::{DataRequest, ListDataRequest};
use crate::error::Result;

/// A reference to a specific component.
///
/// Its links start out derived from the id; [`Component::capabilities`]
/// replaces them with the ones the server advertises.
pub struct Component<'a> {
    pub(crate) client: &'a Client,
    pub(crate) path: String,
    pub(crate) capabilities: EntityCapabilities,
}

impl<'a> Component<'a> {
    pub(crate) fn new(client: &'a Client, id: &str) -> Self {
        let path = client.url(&format!("/components/{}", encode(id)));
        let capabilities = EntityCapabilities {
            id: id.to_owned(),
            data: Some(format!("{path}/data").into()),
            hosts: Some(format!("{path}/hosts").into()),
            ..EntityCapabilities::default()
        };
        Self {
            client,
            path,
            capabilities,
        }
    }

    /// The links this handle navigates by.
    #[must_use]
    pub fn links(&self) -> &EntityCapabilities {
        &self.capabilities
    }

    /// Returns a request builder for the capabilities the server advertises
    /// for this component (`GET /components/{id}`).
    #[must_use]
    pub fn query_capabilities(&self) -> CapabilitiesRequest<'_> {
        CapabilitiesRequest {
            client: self.client,
            path: self.path.clone(),
            schema: false,
        }
    }

    /// Fetch the capabilities of this component and navigate by the links
    /// the server advertises, keeping the defaults where it has none.
    pub async fn capabilities(self) -> Result<Self> {
        let advertised = self.query_capabilities().send().await?.data;
        let capabilities = EntityCapabilities {
            data: merge_link(
                self.client,
                advertised.data.as_ref(),
                self.capabilities.data,
            ),
            hosts: merge_link(
                self.client,
                advertised.hosts.as_ref(),
                self.capabilities.hosts,
            ),
            ..advertised
        };
        Ok(Self {
            capabilities,
            ..self
        })
    }

    /// Returns a request builder for listing data items on this entity.
    #[must_use]
    pub fn list_data(&self) -> ListDataRequest<'_> {
        ListDataRequest {
            client: self.client,
            path: link(self.capabilities.data.as_ref()).to_owned(),
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
            path: format!(
                "{}/{}",
                link(self.capabilities.data.as_ref()),
                encode(data_id)
            ),
        }
    }

    /// Fetch data categories for this entity.
    pub async fn data_categories(&self) -> Result<DataCategories> {
        self.client
            .get(&format!("{}/data-categories", self.path), &[])
            .await
    }

    /// Fetch data groups for this entity.
    pub async fn data_groups(&self) -> Result<DataGroups> {
        self.client
            .get(&format!("{}/data-groups", self.path), &[])
            .await
    }

    /// Fetch apps hosted on this component.
    pub async fn hosts(&self) -> Result<Entities> {
        self.client
            .get(link(self.capabilities.hosts.as_ref()), &[])
            .await
    }

    /// List areas this component belongs to.
    ///
    /// The advertised `belongs-to` link names a single area rather than a
    /// collection, so this keeps to the path derived from the id.
    pub async fn belongs_to(&self) -> Result<Entities> {
        self.client
            .get(&format!("{}/belongs-to", self.path), &[])
            .await
    }
}
