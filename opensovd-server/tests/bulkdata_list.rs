// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

mod common;

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use async_trait::async_trait;
use bytes::Bytes;
use futures::Stream;
use hyper::{Method, StatusCode};
use opensovd_core::{
    App, BulkData, BulkDataError, BulkDataMetadata, BulkDataProvider, CategoryFilter, CategoryInfo,
    DeletedBulkDataItem, Topology,
};

#[derive(Debug, Clone)]
struct RecordingProvider {
    list_called: Arc<AtomicBool>,
}

#[async_trait]
impl BulkDataProvider for RecordingProvider {
    async fn bulk_categories(&self) -> Result<Vec<CategoryInfo>, BulkDataError> {
        Ok(Vec::new())
    }

    async fn bulk_list(
        &self,
        _category_id: &str,
        _filter: CategoryFilter,
    ) -> Result<Vec<BulkDataMetadata>, BulkDataError> {
        self.list_called.store(true, Ordering::SeqCst);
        Ok(Vec::new())
    }

    async fn bulk_download(
        &self,
        _category_id: &str,
        data_id: &str,
    ) -> Result<BulkData, BulkDataError> {
        Err(BulkDataError::NotFound(data_id.into()))
    }

    async fn bulk_upload(
        &self,
        _category_id: &str,
        _data_id: &str,
        _size_upper_bound: u64,
        _data: &mut (dyn Stream<Item = Result<Bytes, BulkDataError>> + Send + Unpin),
        _signature: Option<&String>,
    ) -> Result<(), BulkDataError> {
        Err(BulkDataError::InvalidRequest("not supported".into()))
    }

    async fn bulk_delete(&self, _category_id: &str, _data_id: &str) -> Result<(), BulkDataError> {
        Ok(())
    }

    async fn bulk_delete_category(
        &self,
        _category_id: &str,
    ) -> Result<Vec<DeletedBulkDataItem>, BulkDataError> {
        Ok(Vec::new())
    }
}

#[tokio::test]
async fn invalid_created_at_ranges_are_rejected_before_provider() {
    let list_called = Arc::new(AtomicBool::new(false));
    let topology = Topology::new();
    topology.write().await.add_app(
        App::new("bulkdata-app", "Bulk Data").with_bulkdata_provider(RecordingProvider {
            list_called: Arc::clone(&list_called),
        }),
    );
    let server = common::TestServer::builder()
        .topology(topology)
        .build()
        .await;

    for query in [
        "created-after=2026-10-14T00:00:00Z&created-before=2026-10-13T00:00:00Z",
        "created-after=2026-10-13T00:00:00Z&created-before=2026-10-13T00:00:00Z",
    ] {
        let (status, body) = common::send(
            &server,
            Method::GET,
            &format!("/sovd/v1/apps/bulkdata-app/bulk-data/logs?{query}"),
        )
        .await;

        assert_eq!(status, StatusCode::BAD_REQUEST.as_u16());
        assert_eq!(body["error_code"], "incomplete-request");
        assert!(!list_called.load(Ordering::SeqCst));
    }

    let (status, _) = common::send(
        &server,
        Method::GET,
        "/sovd/v1/apps/bulkdata-app/bulk-data/logs?created-after=2026-10-13T00:00:00Z&created-before=2026-10-14T00:00:00Z",
    )
    .await;
    assert_eq!(status, StatusCode::OK.as_u16());
    assert!(list_called.load(Ordering::SeqCst));
}
