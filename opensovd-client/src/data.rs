// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

use opensovd_models::data::WriteRequest;
use serde::Serialize;

use crate::client::{Client, schema_query};
use crate::error::Result;
use crate::models::{DefaultModels, Models};

/// Request builder for listing data items on an entity.
pub struct ListDataRequest<'a, M = DefaultModels> {
    pub(crate) client: &'a Client<M>,
    pub(crate) path: String,
    pub(crate) schema: bool,
}

impl<M: Models> ListDataRequest<'_, M> {
    /// Append `include-schema=true` to the request.
    #[must_use]
    pub fn schema(mut self, include: bool) -> Self {
        self.schema = include;
        self
    }

    /// Send the request.
    pub async fn send(&self) -> Result<M::DataListResponse> {
        self.client.get(&self.path, schema_query(self.schema)).await
    }
}

/// A reference to a specific data item, used to build read/write requests.
pub struct DataRequest<'a, M = DefaultModels> {
    pub(crate) client: &'a Client<M>,
    pub(crate) path: String,
}

impl<'a, M: Models> DataRequest<'a, M> {
    /// Returns a request builder for reading this data item.
    #[must_use]
    pub fn read(&self) -> ReadDataRequest<'a, M> {
        ReadDataRequest {
            client: self.client,
            path: self.path.clone(),
            schema: false,
        }
    }

    /// Returns a request builder for writing this data item.
    ///
    /// # Errors
    ///
    /// Returns an error if `value` cannot be serialized to JSON.
    #[allow(clippy::result_large_err)]
    pub fn write(&self, value: &impl Serialize) -> Result<WriteDataRequest<'a, M>> {
        Ok(WriteDataRequest {
            client: self.client,
            path: self.path.clone(),
            value: serde_json::to_value(value)?,
        })
    }
}

/// Request builder for reading a single data value.
pub struct ReadDataRequest<'a, M = DefaultModels> {
    client: &'a Client<M>,
    path: String,
    schema: bool,
}

impl<M: Models> ReadDataRequest<'_, M> {
    /// Append `include-schema=true` to the request.
    #[must_use]
    pub fn schema(mut self, include: bool) -> Self {
        self.schema = include;
        self
    }

    /// Send the request.
    pub async fn send(&self) -> Result<M::ReadResponse> {
        self.client.get(&self.path, schema_query(self.schema)).await
    }
}

/// Request builder for writing a single data value.
pub struct WriteDataRequest<'a, M = DefaultModels> {
    client: &'a Client<M>,
    path: String,
    value: serde_json::Value,
}

impl<M> WriteDataRequest<'_, M> {
    /// Send the request.
    pub async fn send(&self) -> Result<()> {
        let body = WriteRequest {
            data: self.value.clone(),
            signature: None,
        };
        self.client.put(&self.path, &[], &body).await
    }
}
