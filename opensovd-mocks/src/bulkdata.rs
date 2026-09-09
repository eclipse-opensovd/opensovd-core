// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! In-memory bulk-data provider for tests.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use bytes::Bytes;
use futures::{Stream, StreamExt as _};
use opensovd_core::{
    BulkData, BulkDataError, BulkDataMetadata, BulkDataProvider, CategoryFilter, CategoryInfo,
    DeletedBulkDataItem,
};

type BulkDataMap = HashMap<String, HashMap<String, Vec<u8>>>;

/// In-memory BulkDataProvider for testing. Supports upload, download, list, and delete.
///
/// Storage layout: `category → (data_id → bytes)`.
#[derive(Clone, Default, Debug)]
pub struct InMemoryBulkDataProvider {
    store: Arc<RwLock<BulkDataMap>>,
}

#[async_trait]
#[allow(clippy::unwrap_used)]
impl BulkDataProvider for InMemoryBulkDataProvider {
    async fn categories(&self) -> Result<Vec<CategoryInfo>, BulkDataError> {
        let store = self.store.read().unwrap();
        let mut cats: Vec<CategoryInfo> = store
            .keys()
            .map(|k| CategoryInfo {
                category: k.clone(),
                translation_id: None,
            })
            .collect();
        cats.sort_by(|a, b| a.category.cmp(&b.category));
        Ok(cats)
    }

    async fn list(
        &self,
        category_id: &str,
        filter: CategoryFilter,
    ) -> Result<Vec<BulkDataMetadata>, BulkDataError> {
        let store = self.store.read().unwrap();
        let items = store
            .get(category_id)
            .map(|entries| {
                let mut v: Vec<BulkDataMetadata> = entries
                    .iter()
                    .filter(|_| {
                        // Tags filtering: skip item if filter requests non-empty tags
                        filter.tags.as_ref().is_none_or(Vec::is_empty)
                    })
                    .map(|(id, data)| BulkDataMetadata {
                        id: id.clone(),
                        mimetype: "application/octet-stream".into(),
                        name: Some(id.clone()),
                        translation_id: None,
                        size: Some(data.len() as u64),
                        creation_date: None,
                        last_modified: None,
                        hash: None,
                        hash_algorithm: None,
                        tags: None,
                    })
                    .collect();
                v.sort_by(|a, b| a.id.cmp(&b.id));
                v
            })
            .unwrap_or_default();
        Ok(items)
    }

    async fn download(&self, category_id: &str, data_id: &str) -> Result<BulkData, BulkDataError> {
        let data = self
            .store
            .read()
            .unwrap()
            .get(category_id)
            .and_then(|cat| cat.get(data_id))
            .cloned()
            .ok_or_else(|| {
                BulkDataError::NotFound(format!("not found: {category_id}/{data_id}"))
            })?;
        let stream = futures::stream::iter(vec![Ok(Bytes::from(data))]);
        Ok(BulkData {
            signature: None,
            data: Box::new(stream),
        })
    }

    async fn upload(
        &self,
        category_id: &str,
        data_id: &str,
        _size: u64,
        data: &mut (dyn Stream<Item = Result<Bytes, BulkDataError>> + Send + Unpin),
        _signature: Option<&String>,
    ) -> Result<(), BulkDataError> {
        let mut buf = Vec::new();
        while let Some(chunk) = data.next().await {
            buf.extend_from_slice(&chunk?);
        }
        self.store
            .write()
            .unwrap()
            .entry(category_id.to_string())
            .or_default()
            .insert(data_id.to_string(), buf);
        Ok(())
    }

    async fn delete(&self, category_id: &str, data_id: &str) -> Result<(), BulkDataError> {
        let mut store = self.store.write().unwrap();

        store
            .get_mut(category_id)
            .and_then(|cat| cat.remove(data_id))
            .ok_or_else(|| BulkDataError::NotFound(format!("not found: {category_id}/{data_id}")))
            .map(|_| ())
    }

    async fn delete_category(
        &self,
        category_id: &str,
    ) -> Result<Vec<DeletedBulkDataItem>, BulkDataError> {
        let mut store = self.store.write().unwrap();

        store
            .remove(category_id)
            .ok_or_else(|| BulkDataError::NotFound(format!("not found: {category_id}")))
            .map(|items| {
                items
                    .into_keys()
                    .map(|id| DeletedBulkDataItem { id, error: None })
                    .collect()
            })
    }
}
