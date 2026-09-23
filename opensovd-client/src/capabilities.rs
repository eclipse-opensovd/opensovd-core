// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

use opensovd_models::discovery::EntityCapabilities;
use opensovd_models::{Response, UriReference};

use crate::client::{Client, schema_query};
use crate::error::Result;

/// Request builder for the capabilities of the vehicle or of an entity.
pub struct CapabilitiesRequest<'a> {
    pub(crate) client: &'a Client,
    pub(crate) path: String,
    pub(crate) schema: bool,
}

impl CapabilitiesRequest<'_> {
    /// Append `include-schema=true` to the request.
    #[must_use]
    pub fn schema(mut self, include: bool) -> Self {
        self.schema = include;
        self
    }

    /// Send the request.
    pub async fn send(self) -> Result<Response<EntityCapabilities>> {
        self.client.get(&self.path, schema_query(self.schema)).await
    }
}

/// Prefer the link the server advertised, keeping the default where it has none.
pub(crate) fn merge_link(
    client: &Client,
    advertised: Option<&UriReference>,
    default: Option<UriReference>,
) -> Option<UriReference> {
    advertised
        .map(|link| UriReference(client.resolve(&link.0)))
        .or(default)
}

pub(crate) fn link(link: Option<&UriReference>) -> &str {
    link.map_or("", |l| l.0.as_str())
}
