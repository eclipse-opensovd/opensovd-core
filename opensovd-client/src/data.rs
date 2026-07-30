// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

use std::borrow::Borrow;

use opensovd_models::Response;
use opensovd_models::data::{DataCategory, DataList, ReadResponse, WriteRequest};
use serde::Serialize;

use crate::client::{Client, encode, schema_query};
use crate::error::Result;

/// Request builder for listing data items on an entity.
pub struct ListDataRequest<'a> {
    pub(crate) client: &'a Client,
    pub(crate) path: String,
    pub(crate) schema: bool,
    pub(crate) groups: Vec<String>,
    pub(crate) categories: Vec<DataCategory>,
    pub(crate) tags: Vec<String>,
}

impl ListDataRequest<'_> {
    /// Append `include-schema=true` to the request.
    #[must_use]
    pub fn schema(mut self, include: bool) -> Self {
        self.schema = include;
        self
    }

    /// Filter by data groups.
    #[must_use]
    pub fn groups<I, S>(mut self, groups: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.groups = groups.into_iter().map(|s| s.as_ref().to_owned()).collect();
        self
    }

    /// Filter by data categories.
    #[must_use]
    pub fn categories<I, C>(mut self, categories: I) -> Self
    where
        I: IntoIterator<Item = C>,
        C: Borrow<DataCategory>,
    {
        self.categories = categories.into_iter().map(|c| c.borrow().clone()).collect();
        self
    }

    /// Filter by data tags.
    #[must_use]
    pub fn tags<I, S>(mut self, tags: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.tags = tags.into_iter().map(|s| s.as_ref().to_owned()).collect();
        self
    }

    /// Send the request.
    pub async fn send(&self) -> Result<Response<DataList>> {
        let mut query: Vec<(String, String)> = Vec::new();
        for g in &self.groups {
            query.push(("groups".into(), encode(g)));
        }
        for c in &self.categories {
            query.push(("categories".into(), encode(c.as_str())));
        }
        for t in &self.tags {
            query.push(("tags".into(), encode(t)));
        }
        if let [(k, v)] = schema_query(self.schema) {
            query.push((k.to_string(), v.to_string()));
        }
        let refs: Vec<(&str, &str)> = query
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        self.client.get(&self.path, &refs).await
    }
}

/// A reference to a specific data item, used to build read/write requests.
pub struct DataRequest<'a> {
    pub(crate) client: &'a Client,
    pub(crate) path: String,
}

impl DataRequest<'_> {
    /// Returns a request builder for reading this data item.
    #[must_use]
    pub fn read(&self) -> ReadDataRequest<'_> {
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
    pub fn write(&self, value: &impl Serialize) -> Result<WriteDataRequest<'_>> {
        Ok(WriteDataRequest {
            client: self.client,
            path: self.path.clone(),
            value: serde_json::to_value(value)?,
        })
    }
}

/// Request builder for reading a single data value.
pub struct ReadDataRequest<'a> {
    client: &'a Client,
    path: String,
    schema: bool,
}

impl ReadDataRequest<'_> {
    /// Append `include-schema=true` to the request.
    #[must_use]
    pub fn schema(mut self, include: bool) -> Self {
        self.schema = include;
        self
    }

    /// Send the request.
    pub async fn send(&self) -> Result<ReadResponse> {
        self.client.get(&self.path, schema_query(self.schema)).await
    }
}

/// Request builder for writing a single data value.
pub struct WriteDataRequest<'a> {
    client: &'a Client,
    path: String,
    value: serde_json::Value,
}

impl WriteDataRequest<'_> {
    /// Send the request.
    pub async fn send(&self) -> Result<()> {
        let body = WriteRequest {
            data: self.value.clone(),
            signature: None,
        };
        self.client.put(&self.path, &[], &body).await
    }
}
