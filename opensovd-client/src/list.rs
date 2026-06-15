// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
//
// See the NOTICE file(s) distributed with this work for additional
// information regarding copyright ownership.
//
// This program and the accompanying materials are made available under the
// terms of the Apache License Version 2.0 which is available at
// https://www.apache.org/licenses/LICENSE-2.0
//
// SPDX-License-Identifier: Apache-2.0

use opensovd_models::Response;
use opensovd_models::discovery::Entities;

use crate::client::{Client, schema_query};
use crate::error::Result;

/// Request builder for listing entities in a collection.
pub struct ListEntitiesRequest<'a> {
    pub(crate) client: &'a Client,
    pub(crate) path: String,
    pub(crate) schema: bool,
}

impl ListEntitiesRequest<'_> {
    /// Append `include-schema=true` to the request.
    #[must_use]
    pub fn schema(mut self, include: bool) -> Self {
        self.schema = include;
        self
    }

    /// Send the request.
    pub async fn send(&self) -> Result<Response<Entities>> {
        self.client.get(&self.path, schema_query(self.schema)).await
    }
}
