// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

use crate::client::{Client, schema_query};
use crate::error::Result;
use crate::models::{DefaultModels, Models};

/// Request builder for listing entities in a collection.
pub struct ListEntitiesRequest<'a, M = DefaultModels> {
    pub(crate) client: &'a Client<M>,
    pub(crate) path: String,
    pub(crate) schema: bool,
}

impl<M: Models> ListEntitiesRequest<'_, M> {
    /// Append `include-schema=true` to the request.
    #[must_use]
    pub fn schema(mut self, include: bool) -> Self {
        self.schema = include;
        self
    }

    /// Send the request.
    pub async fn send(&self) -> Result<M::Entities> {
        self.client.get(&self.path, schema_query(self.schema)).await
    }
}
