// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! In-memory bulk-data provider for tests.

use std::collections::{HashMap, HashSet};
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
    permanent: Arc<RwLock<HashSet<(String, String)>>>,
}

#[allow(clippy::unwrap_used)]
impl InMemoryBulkDataProvider {
    /// Adds an entry that is always present and can never be deleted.
    ///
    /// # Panics
    ///
    /// Panics if the internal locks are poisoned.
    #[must_use]
    pub fn with_permanent_entry(self, category_id: &str, data_id: &str) -> Self {
        self.store
            .write()
            .unwrap()
            .entry(category_id.to_string())
            .or_default()
            .insert(data_id.to_string(), Vec::new());
        self.permanent
            .write()
            .unwrap()
            .insert((category_id.to_string(), data_id.to_string()));
        self
    }

    fn is_permanent(&self, category_id: &str, data_id: &str) -> bool {
        self.permanent
            .read()
            .unwrap()
            .contains(&(category_id.to_string(), data_id.to_string()))
    }
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
        if self.is_permanent(category_id, data_id) {
            return Err(BulkDataError::DeletionFailed(format!(
                "entry is protected: {category_id}/{data_id}"
            )));
        }

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
        let permanent = self.permanent.read().unwrap();
        let mut store = self.store.write().unwrap();

        let entries = store
            .get_mut(category_id)
            .ok_or_else(|| BulkDataError::NotFound(format!("not found: {category_id}")))?;

        let ids: Vec<String> = entries.keys().cloned().collect();
        let mut deleted = Vec::with_capacity(ids.len());
        for id in ids {
            if permanent.contains(&(category_id.to_string(), id.clone())) {
                deleted.push(DeletedBulkDataItem {
                    error: Some(BulkDataError::DeletionFailed(format!(
                        "entry is protected: {category_id}/{id}"
                    ))),
                    id,
                });
            } else {
                entries.remove(&id);
                deleted.push(DeletedBulkDataItem { id, error: None });
            }
        }

        if entries.is_empty() {
            store.remove(category_id);
        }

        deleted.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(deleted)
    }
}
