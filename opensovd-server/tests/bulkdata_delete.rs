// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Deleting a bulk-data category reports each failed item like a single
//! error.

mod common;

use async_trait::async_trait;
use bytes::Bytes;
use futures::Stream;
use http_body_util::BodyExt;
use hyper::{Request, StatusCode};
use opensovd_core::{
    BulkData, BulkDataError, BulkDataMetadata, BulkDataProvider, CategoryFilter, CategoryInfo,
    Component, DeletedBulkDataItem, Topology,
};
use serde_json::{Value, json};

/// Logs whose deletion fails in every way but one.
#[derive(Debug)]
struct LogsProvider;

#[async_trait]
impl BulkDataProvider for LogsProvider {
    async fn bulk_categories(&self) -> Result<Vec<CategoryInfo>, BulkDataError> {
        Ok(Vec::new())
    }

    async fn bulk_list(
        &self,
        _category_id: &str,
        _filter: CategoryFilter,
    ) -> Result<Vec<BulkDataMetadata>, BulkDataError> {
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
        Err(BulkDataError::InvalidRequest("read only".into()))
    }

    async fn bulk_delete(&self, _category_id: &str, _data_id: &str) -> Result<(), BulkDataError> {
        Ok(())
    }

    async fn bulk_delete_category(
        &self,
        _category_id: &str,
    ) -> Result<Vec<DeletedBulkDataItem>, BulkDataError> {
        let item = |id: &str, error| DeletedBulkDataItem {
            id: id.into(),
            error,
        };
        Ok(vec![
            item("boot.log", None),
            item(
                "crash.log",
                Some(BulkDataError::DeletionFailed("in use".into())),
            ),
            item(
                "trace.log",
                Some(BulkDataError::Internal(
                    "/var/lib/logs: permission denied".into(),
                )),
            ),
        ])
    }
}

#[tokio::test]
async fn failed_items_answer_their_own_error_codes() {
    let topology = Topology::new();
    topology
        .write()
        .await
        .add_component(Component::new("Radio", "Radio").with_bulkdata_provider(LogsProvider));
    let server = common::TestServer::builder()
        .topology(topology)
        .build()
        .await;
    let request = Request::builder()
        .method("DELETE")
        .uri(server.url("/sovd/v1/components/Radio/bulk-data/logs"))
        .body(http_body_util::Empty::<Bytes>::new())
        .unwrap();
    let response = common::client().request(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(body["deleted_ids"], json!(["boot.log"]));
    let errors = body["errors"].as_array().unwrap();
    let error = |id: &str| {
        &errors
            .iter()
            .find(|e| e["id"] == id)
            .unwrap_or_else(|| panic!("no error for {id}"))["error"]
    };
    assert_eq!(error("crash.log")["vendor_code"], "unable-to-delete");
    assert_eq!(error("trace.log")["error_code"], "sovd-server-failure");
    assert_eq!(error("trace.log")["message"], "An internal error occurred");
}
