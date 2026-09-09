// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Bulk-Data provider trait and types.

use async_trait::async_trait;
use bytes::Bytes;
use chrono::{DateTime, Utc};
use futures_core::Stream;

use crate::CategoryInfo;

#[derive(Debug, thiserror::Error)]
pub enum BulkDataError {
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    #[error("bulk data deletion failed: {0}")]
    DeletionFailed(String),
    #[error("{0}")]
    Internal(String),
    #[error("bulk data not found: {0}")]
    NotFound(String),
}

pub struct CategoryFilter {
    pub created_before: Option<DateTime<Utc>>,
    pub created_after: Option<DateTime<Utc>>,
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
pub struct BulkDataMetadata {
    pub id: String,
    pub mimetype: String,
    pub name: Option<String>,
    pub translation_id: Option<String>,
    pub size: Option<u64>,
    pub creation_date: Option<DateTime<Utc>>,
    pub last_modified: Option<DateTime<Utc>>,
    pub hash: Option<String>,
    pub hash_algorithm: Option<String>,
    pub tags: Option<Vec<String>>,
}

pub struct BulkData {
    pub signature: Option<String>,
    pub data: Box<dyn Stream<Item = Result<Bytes>> + Send + Unpin>,
}

/// A `Result` alias where the `Err` variant is [`BulkDataError`].
pub type Result<T> = std::result::Result<T, BulkDataError>;

#[async_trait]
pub trait BulkDataProvider: Send + Sync + std::fmt::Debug + 'static {
    async fn categories(&self) -> Result<Vec<CategoryInfo>>;

    async fn list(
        &self,
        category_id: &str,
        filter: CategoryFilter,
    ) -> Result<Vec<BulkDataMetadata>>;

    async fn download(&self, category_id: &str, data_id: &str) -> Result<BulkData>;

    async fn upload(
        &self,
        category_id: &str,
        data_id: &str,
        size_upper_bound: u64,
        data: &mut (dyn Stream<Item = Result<Bytes>> + Send + Unpin),
        signature: Option<&String>,
    ) -> Result<()>;

    async fn delete(&self, category_id: &str, data_id: Option<&str>) -> Result<()>;
}
