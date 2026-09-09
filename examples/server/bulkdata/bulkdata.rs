// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

#![allow(clippy::expect_used)]

//! Bulk-data example backed by a temporary filesystem directory.
//!
//! Starts a server on port 7690 with a single app-scoped bulk-data provider at
//! `/sovd/v1/apps/bulkdata-example/bulk-data`.
//!
//! Categories are mapped to directories under a temporary root. The example
//! starts empty and creates category directories on first upload.
//!
//! Run with: `cargo run -p opensovd-examples-server --example bulkdata`

use std::path::PathBuf;

use async_trait::async_trait;
use bytes::Bytes;
use chrono::{DateTime, Utc};
use futures::{Stream, StreamExt, stream};
use opensovd_core::{
    App, BulkData, BulkDataError, BulkDataMetadata, BulkDataProvider, CategoryFilter, CategoryInfo,
    Component,
};
use opensovd_server::{Server, Topology};
use tempfile::TempDir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

const COMPONENT_ID: &str = "bulkdata-host";
const APP_ID: &str = "bulkdata-example";
const DEFAULT_MIMETYPE: &str = "application/octet-stream";
const STREAM_CHUNK_SIZE: usize = 64 * 1024;

#[derive(Debug)]
struct TempFsBulkDataProvider {
    root: TempDir,
}

impl TempFsBulkDataProvider {
    fn validate_segment(kind: &str, value: &str) -> std::result::Result<(), BulkDataError> {
        if value.is_empty()
            || value == "."
            || value == ".."
            || value.contains('/')
            || value.contains('\\')
        {
            return Err(BulkDataError::InvalidRequest(format!(
                "invalid {kind}: {value}"
            )));
        }
        Ok(())
    }

    fn category_path(&self, category_id: &str) -> std::result::Result<PathBuf, BulkDataError> {
        Self::validate_segment("category", category_id)?;
        Ok(self.root.path().join(category_id))
    }

    fn data_path(
        &self,
        category_id: &str,
        data_id: &str,
    ) -> std::result::Result<PathBuf, BulkDataError> {
        Self::validate_segment("data id", data_id)?;
        Ok(self.category_path(category_id)?.join(data_id))
    }

    async fn ensure_category_dir(
        &self,
        category_id: &str,
    ) -> std::result::Result<PathBuf, BulkDataError> {
        let category_path = self.category_path(category_id)?;
        tokio::fs::create_dir_all(&category_path)
            .await
            .map_err(|error| BulkDataError::Internal(error.to_string()))?;
        Ok(category_path)
    }

    fn include_metadata(metadata: &BulkDataMetadata, filter: &CategoryFilter) -> bool {
        if filter.tags.as_ref().is_some_and(|tags| !tags.is_empty()) {
            return false;
        }

        if let Some(created_before) = filter.created_before
            && metadata
                .creation_date
                .as_ref()
                .is_some_and(|created_at| created_at >= &created_before)
        {
            return false;
        }

        if let Some(created_after) = filter.created_after
            && metadata
                .creation_date
                .as_ref()
                .is_some_and(|created_at| created_at <= &created_after)
        {
            return false;
        }

        true
    }

    async fn file_metadata(
        &self,
        entry: &tokio::fs::DirEntry,
    ) -> std::result::Result<Option<BulkDataMetadata>, BulkDataError> {
        let file_type = entry
            .file_type()
            .await
            .map_err(|error| BulkDataError::Internal(error.to_string()))?;
        if !file_type.is_file() {
            return Ok(None);
        }

        let data_id = entry.file_name().to_string_lossy().to_string();
        let stats = entry
            .metadata()
            .await
            .map_err(|error| BulkDataError::Internal(error.to_string()))?;

        let creation_date = stats.created().ok().map(Into::<DateTime<Utc>>::into);
        let last_modified = stats.modified().ok().map(Into::<DateTime<Utc>>::into);

        Ok(Some(BulkDataMetadata {
            id: data_id.clone(),
            mimetype: DEFAULT_MIMETYPE.to_string(),
            name: Some(data_id),
            translation_id: None,
            size: Some(stats.len()),
            creation_date,
            last_modified,
            hash: None,
            hash_algorithm: None,
            tags: None,
        }))
    }
}

#[async_trait]
impl BulkDataProvider for TempFsBulkDataProvider {
    async fn categories(&self) -> std::result::Result<Vec<CategoryInfo>, BulkDataError> {
        let mut entries = tokio::fs::read_dir(self.root.path())
            .await
            .map_err(|error| BulkDataError::Internal(error.to_string()))?;
        let mut categories = Vec::new();

        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|error| BulkDataError::Internal(error.to_string()))?
        {
            let file_type = entry
                .file_type()
                .await
                .map_err(|error| BulkDataError::Internal(error.to_string()))?;
            if file_type.is_dir() {
                categories.push(CategoryInfo {
                    category: entry.file_name().to_string_lossy().to_string(),
                    translation_id: None,
                });
            }
        }

        categories.sort_by(|left, right| left.category.cmp(&right.category));
        Ok(categories)
    }

    async fn list(
        &self,
        category_id: &str,
        filter: CategoryFilter,
    ) -> std::result::Result<Vec<BulkDataMetadata>, BulkDataError> {
        let category_path = self.category_path(category_id)?;
        let mut entries = match tokio::fs::read_dir(&category_path).await {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(BulkDataError::Internal(error.to_string())),
        };
        let mut items = Vec::new();

        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|error| BulkDataError::Internal(error.to_string()))?
        {
            if let Some(metadata) = self.file_metadata(&entry).await?
                && Self::include_metadata(&metadata, &filter)
            {
                items.push(metadata);
            }
        }

        items.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(items)
    }

    async fn download(
        &self,
        category_id: &str,
        data_id: &str,
    ) -> std::result::Result<BulkData, BulkDataError> {
        let path = self.data_path(category_id, data_id)?;
        let file = tokio::fs::File::open(&path).await.map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                BulkDataError::NotFound(format!("bulk data not found: {category_id}/{data_id}"))
            } else {
                BulkDataError::Internal(error.to_string())
            }
        })?;

        let read_stream = stream::try_unfold(file, |mut file| async move {
            let mut buffer = vec![0u8; STREAM_CHUNK_SIZE];
            let bytes_read = file.read(&mut buffer).await?;
            if bytes_read == 0 {
                return Ok(None);
            }
            buffer.truncate(bytes_read);
            Ok(Some((Bytes::from(buffer), file)))
        })
        .map(|result| {
            result.map_err(|error: std::io::Error| BulkDataError::Internal(error.to_string()))
        });

        Ok(BulkData {
            signature: None,
            data: Box::new(Box::pin(read_stream)),
        })
    }

    async fn upload(
        &self,
        category_id: &str,
        data_id: &str,
        _size: u64,
        data: &mut (dyn Stream<Item = std::result::Result<Bytes, BulkDataError>> + Send + Unpin),
        _signature: Option<&String>,
    ) -> std::result::Result<(), BulkDataError> {
        let _category_path = self.ensure_category_dir(category_id).await?;
        let path = self.data_path(category_id, data_id)?;
        let temp_path = self.data_path(category_id, &format!("{data_id}.tmp"))?;
        let mut file = tokio::fs::File::create(&temp_path)
            .await
            .map_err(|error| BulkDataError::Internal(error.to_string()))?;

        while let Some(chunk) = data.next().await {
            let chunk = chunk?;
            if let Err(e) = file.write_all(&chunk).await {
                let _ = tokio::fs::remove_file(&temp_path).await;
                return Err(BulkDataError::Internal(e.to_string()));
            }
        }

        let _ = file.flush().await;
        drop(file);
        tokio::fs::rename(&temp_path, &path)
            .await
            .map_err(|error| BulkDataError::Internal(error.to_string()))?;

        Ok(())
    }

    async fn delete(
        &self,
        category_id: &str,
        data_id: Option<&str>,
    ) -> std::result::Result<(), BulkDataError> {
        if let Some(data_id) = data_id {
            let path = self.data_path(category_id, data_id)?;
            tokio::fs::remove_file(&path).await.map_err(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    BulkDataError::DeletionFailed(format!(
                        "bulk data not found: {category_id}/{data_id}"
                    ))
                } else {
                    BulkDataError::DeletionFailed(error.to_string())
                }
            })?;
        } else {
            let category_path = self.category_path(category_id)?;
            if let Err(error) = tokio::fs::remove_dir_all(&category_path).await
                && error.kind() != std::io::ErrorKind::NotFound
            {
                return Err(BulkDataError::DeletionFailed(error.to_string()));
            }
        }
        Ok(())
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    libcli::init_tracing("info", None)?;

    let root = tempfile::tempdir()?;
    let root_path = root.path().display().to_string();
    let provider = TempFsBulkDataProvider { root };

    let component = Component::new(COMPONENT_ID, "Bulkdata Host");
    let app = App::new(APP_ID, "Filesystem Bulkdata Example", COMPONENT_ID)
        .with_bulkdata_provider(provider);

    let topology = Topology::new();
    {
        let mut topology_guard = topology.write().await;
        topology_guard.add_component(component);
        topology_guard.add_app(app);
    }

    let listener = TcpListener::bind("127.0.0.1:7690").await?;
    let server = Server::builder()
        .base_uri("http://127.0.0.1:7690/sovd")?
        .listener(listener)
        .topology(topology)
        .layer(libcli::trace::trace_layer())
        .build()?;

    tracing::info!(
        root_path,
        app_id = APP_ID,
        "Bulkdata example server running"
    );
    server.serve().await?;
    Ok(())
}
