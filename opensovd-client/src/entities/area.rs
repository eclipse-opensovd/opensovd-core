// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

use opensovd_models::discovery::{Entities, EntityCapabilities};

use crate::capabilities::{CapabilitiesRequest, link, merge_link};
use crate::client::{Client, encode};
use crate::error::Result;

/// A reference to a specific area.
///
/// Its links start out derived from the id; [`Area::capabilities`] replaces
/// them with the ones the server advertises.
pub struct Area<'a> {
    pub(crate) client: &'a Client,
    pub(crate) path: String,
    pub(crate) capabilities: EntityCapabilities,
}

impl<'a> Area<'a> {
    pub(crate) fn new(client: &'a Client, id: &str) -> Self {
        let path = client.url(&format!("/areas/{}", encode(id)));
        let capabilities = EntityCapabilities {
            id: id.to_owned(),
            contains: Some(format!("{path}/contains").into()),
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
    /// for this area (`GET /areas/{id}`).
    #[must_use]
    pub fn query_capabilities(&self) -> CapabilitiesRequest<'_> {
        CapabilitiesRequest {
            client: self.client,
            path: self.path.clone(),
            schema: false,
        }
    }

    /// Fetch the capabilities of this area and navigate by the links the
    /// server advertises, keeping the defaults where it has none.
    pub async fn capabilities(self) -> Result<Self> {
        let advertised = self.query_capabilities().send().await?.data;
        let capabilities = EntityCapabilities {
            contains: merge_link(
                self.client,
                advertised.contains.as_ref(),
                self.capabilities.contains,
            ),
            ..advertised
        };
        Ok(Self {
            capabilities,
            ..self
        })
    }

    /// List entities contained in this area.
    pub async fn contains(&self) -> Result<Entities> {
        self.client
            .get(link(self.capabilities.contains.as_ref()), &[])
            .await
    }
}
