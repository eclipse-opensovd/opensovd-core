// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Bulk-Data resource endpoints.
//!
//! Provides routes below the path of an entity, `{entity_path}`, which is
//! empty for the server:
//! - GET /{entity_path}/bulk-data - Retrieve bulk-data categories
//! - GET /{entity_path}/bulk-data/{category} - Retrieve the bulk data of a category
//! - POST /{entity_path}/bulk-data/{category} - Upload bulk data
//! - DELETE /{entity_path}/bulk-data/{category} - Delete all bulk data of a category
//! - GET /{entity_path}/bulk-data/{category}/{bulk_data_id} - Download bulk data
//! - DELETE /{entity_path}/bulk-data/{category}/{bulk_data_id} - Delete a bulk-data resource

use std::vec;

use axum::{
    Router,
    body::Body,
    extract::{DefaultBodyLimit, FromRequest, Multipart, Request, State},
    response::IntoResponse,
    routing::get,
};
use axum_extra::headers::{ContentLength, ContentType, Header};
use futures::StreamExt;
use http::{HeaderMap, HeaderName, HeaderValue, StatusCode, request::Parts};
use opensovd_core::{BulkDataError, CategoryFilter, Topology};
use opensovd_models::bulkdata::{
    AvailableBulkDataCategories, BulkDataCategoriesQuery, BulkDataCategory, BulkDataDescriptor,
    BulkDataDescriptorsQuery, BulkDataMetadata, BulkDataUpload, DeleteBulkDataError,
    DeleteBulkDataResult,
};
use serde::Deserialize;

use crate::routes::{
    discovery::{encode_path, encode_path_segment},
    entity::Entity,
    error::{Result, bulk_data_error},
    extract::{Json, Path, Query, TypedHeader},
    versioned_uri,
};
use crate::schema::JsonSchema;

// Maximum allowed size for bulk data uploads (1 GiB).
const MAX_BULKDATA_BODY_BYTES: usize = 1024 * 1024 * 1024;

pub fn routes() -> Router<Topology> {
    Router::new()
        .route("/bulk-data", get(bulk_data_categories))
        .route(
            "/bulk-data/{category}",
            get(bulk_data_descriptors)
                .post(bulk_data_upload)
                .delete(bulk_data_delete_category),
        )
        .route(
            "/bulk-data/{category}/{bulk_data_id}",
            get(bulk_data_download).delete(bulk_data_delete),
        )
        .layer(DefaultBodyLimit::max(MAX_BULKDATA_BODY_BYTES))
}

#[derive(Deserialize)]
struct Category {
    category: String,
}

#[derive(Deserialize)]
struct BulkDataId {
    category: String,
    bulk_data_id: String,
}

/// GET /{entity_path}/bulk-data - Retrieve bulk-data categories.
///
/// Returns the bulk-data categories of an entity.
async fn bulk_data_categories(
    State(topology): State<Topology>,
    entity: Entity,
    Query(query): Query<BulkDataCategoriesQuery>,
) -> Result<Json<AvailableBulkDataCategories>> {
    let provider = entity.bulk_data_provider(&topology.read().await)?;
    Ok(Json(AvailableBulkDataCategories {
        items: provider
            .bulk_categories()
            .await?
            .iter()
            .map(|c| BulkDataCategory(c.category.clone()))
            .collect::<Vec<_>>(),
        schema: query
            .include_schema
            .then(AvailableBulkDataCategories::schema),
    }))
}

fn category_filter(query: &BulkDataDescriptorsQuery) -> CategoryFilter {
    CategoryFilter {
        created_before: query.created_before,
        created_after: query.created_after,
        tags: query.tags.clone(),
    }
}

/// GET /{entity_path}/bulk-data/{category} - Retrieve the bulk data of a category.
///
/// Returns the descriptors of the bulk data in a category, optionally filtered
/// by creation time or tags.
async fn bulk_data_descriptors(
    State(topology): State<Topology>,
    entity: Entity,
    Path(Category { category }): Path<Category>,
    Query(query): Query<BulkDataDescriptorsQuery>,
) -> Result<Json<BulkDataMetadata>> {
    let provider = entity.bulk_data_provider(&topology.read().await)?;

    Ok(Json(BulkDataMetadata {
        items: provider
            .bulk_list(&category, category_filter(&query))
            .await?
            .iter()
            .map(|metadata| BulkDataDescriptor {
                id: metadata.id.clone(),
                mimetype: metadata.mimetype.clone(),
                name: metadata.name.clone(),
                translation_id: metadata.translation_id.clone(),
                size: metadata.size,
                creation_date: metadata.creation_date,
                last_modified: metadata.last_modified,
                hash: metadata.hash.clone(),
                hash_algorithm: metadata.hash_algorithm.clone(),
                tags: metadata.tags.clone(),
            })
            .collect::<Vec<_>>(),
        schema: query.include_schema.then(BulkDataMetadata::schema),
    }))
}

#[derive(Clone, Debug)]
pub struct ContentDisposition {
    _disposition_type: String,
    filename: Option<String>,
    name: Option<String>,
}

impl Header for ContentDisposition {
    fn name() -> &'static HeaderName {
        &::http::header::CONTENT_DISPOSITION
    }

    fn decode<'i, I: Iterator<Item = &'i HeaderValue>>(
        values: &mut I,
    ) -> std::result::Result<Self, axum_extra::headers::Error> {
        values
            .next()
            .cloned()
            .map(|hv| {
                let s = hv
                    .to_str()
                    .map_err(|_| axum_extra::headers::Error::invalid())?;
                let parts: Vec<&str> = s.split(';').collect();
                let disposition_type = parts
                    .first()
                    .map(|s| s.trim().to_string())
                    .unwrap_or_default();
                let mut filename = None;
                let mut name = None;
                for part in parts.get(1..).unwrap_or(&[]) {
                    let part = part.trim();
                    if part.starts_with("filename=") {
                        filename = Some(
                            part.trim_start_matches("filename=")
                                .trim_matches('"')
                                .to_string(),
                        );
                    } else if part.starts_with("name=") {
                        name = Some(
                            part.trim_start_matches("name=")
                                .trim_matches('"')
                                .to_string(),
                        );
                    }
                }
                Ok(ContentDisposition {
                    _disposition_type: disposition_type,
                    filename,
                    name,
                })
            })
            .transpose()?
            .ok_or_else(axum_extra::headers::Error::invalid)
    }

    fn encode<E: Extend<HeaderValue>>(&self, _values: &mut E) {
        unimplemented!();
    }
}

/// POST /{entity_path}/bulk-data/{category} - Upload bulk data.
///
/// Stores an octet-stream or multipart body and answers its location.
#[expect(clippy::too_many_arguments)]
async fn bulk_data_upload(
    State(topology): State<Topology>,
    entity: Entity,
    Path(Category { category }): Path<Category>,
    parts: Parts,
    TypedHeader(content_type): TypedHeader<ContentType>,
    TypedHeader(content_disposition): TypedHeader<ContentDisposition>,
    TypedHeader(content_length): TypedHeader<ContentLength>,
    req: Request,
) -> Result<(StatusCode, HeaderMap, Json<BulkDataUpload>)> {
    let provider = entity.bulk_data_provider(&topology.read().await)?;

    let filename = content_disposition
        .filename
        .clone()
        .or_else(|| content_disposition.name.clone())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    if content_length.0 > MAX_BULKDATA_BODY_BYTES as u64 {
        return Err(BulkDataError::InvalidRequest(format!(
            "Content-Length exceeds maximum allowed size of {MAX_BULKDATA_BODY_BYTES} bytes",
        ))
        .into());
    }

    if content_type == ContentType::octet_stream() {
        let mut stream = req
            .into_body()
            .into_data_stream()
            .map(|r| r.map_err(|e| BulkDataError::Internal(e.to_string())));
        provider
            .bulk_upload(&category, &filename, content_length.0, &mut stream, None)
            .await?;
    } else {
        let mut multipart = Multipart::from_request(req, &())
            .await
            .map_err(|e| BulkDataError::InvalidRequest(e.to_string()))?;
        let mut sig = None;
        let mut file_content_received = false;
        while let Some(field) = multipart
            .next_field()
            .await
            .map_err(|e| BulkDataError::InvalidRequest(e.to_string()))?
        {
            match field.content_type() {
                Some("application/json") => {
                    let json: serde_json::Value = serde_json::from_str(
                        &field
                            .text()
                            .await
                            .map_err(|e| BulkDataError::Internal(e.to_string()))?,
                    )
                    .map_err(|e| BulkDataError::InvalidRequest(e.to_string()))?;

                    if let Some(signature) = json.get("signature").and_then(|s| s.as_str()) {
                        sig = Some(signature.to_string());
                    }
                }
                Some("application/octet-stream") => {
                    // if the field has its own Content-Length header, we can
                    // get a better estimate of the size of the data being
                    // uploaded. Otherwise, we fall back to the Content-Length
                    // header of the entire request.
                    let length = field
                        .headers()
                        .get("Content-Length")
                        .and_then(|v| v.to_str().ok())
                        .and_then(|s| s.parse::<u64>().ok())
                        .unwrap_or(content_length.0);
                    let mut data_stream = field.map(|chunk| {
                        chunk.map_err(|e| BulkDataError::InvalidRequest(e.to_string()))
                    });
                    provider
                        .bulk_upload(&category, &filename, length, &mut data_stream, sig.as_ref())
                        .await?;
                    file_content_received = true;
                    break;
                }
                _ => {
                    return Err(BulkDataError::InvalidRequest(
                        "unexpected content type".to_string(),
                    )
                    .into());
                }
            }
        }
        if !file_content_received {
            return Err(
                BulkDataError::InvalidRequest("no file content received".to_string()).into(),
            );
        }
    }

    let mut headers = HeaderMap::new();
    let location = HeaderValue::from_str(&format!(
        "{}/{}/bulk-data/{}/{}",
        versioned_uri(&parts),
        encode_path(entity.path()),
        encode_path_segment(&category),
        encode_path_segment(&filename),
    ))
    .map_err(|e| BulkDataError::Internal(e.to_string()))?;
    headers.insert("Location", location);

    Ok((
        StatusCode::CREATED,
        headers,
        Json(BulkDataUpload { id: filename }),
    ))
}

/// DELETE /{entity_path}/bulk-data/{category} - Delete all bulk data of a category.
///
/// Returns the deleted ids and an error for each item that was not deleted.
async fn bulk_data_delete_category(
    State(topology): State<Topology>,
    entity: Entity,
    Path(Category { category }): Path<Category>,
) -> Result<Json<DeleteBulkDataResult>> {
    let provider = entity.bulk_data_provider(&topology.read().await)?;

    let deleted = provider.bulk_delete_category(&category).await?;
    let mut result = DeleteBulkDataResult {
        deleted_ids: vec![],
        errors: vec![],
    };
    for item in deleted {
        if let Some(error) = item.error {
            let (_, error) = bulk_data_error(&error);
            result.errors.push(DeleteBulkDataError {
                id: item.id.clone(),
                error,
            });
        } else {
            result.deleted_ids.push(item.id.clone());
        }
    }

    Ok(Json(result))
}

/// GET /{entity_path}/bulk-data/{category}/{bulk_data_id} - Download bulk data.
///
/// Streams the content as an attachment, with its signature if it has one.
async fn bulk_data_download(
    State(topology): State<Topology>,
    entity: Entity,
    Path(BulkDataId {
        category,
        bulk_data_id,
    }): Path<BulkDataId>,
) -> Result<impl IntoResponse> {
    let provider = entity.bulk_data_provider(&topology.read().await)?;

    let bulkdata = provider.bulk_download(&category, &bulk_data_id).await?;

    let mut response = axum::response::Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/octet-stream")
        .header(
            "Content-Disposition",
            format!("attachment; filename=\"{bulk_data_id}\""),
        );

    if let Some(signature) = bulkdata.signature {
        response = response.header("X-Signature", signature);
    }

    Ok(response
        .body(Body::from_stream(bulkdata.data))
        .map_err(|e| BulkDataError::Internal(e.to_string()))?)
}

/// DELETE /{entity_path}/bulk-data/{category}/{bulk_data_id} - Delete a bulk-data resource.
///
/// Deletes one bulk-data resource of a category.
async fn bulk_data_delete(
    State(topology): State<Topology>,
    entity: Entity,
    Path(BulkDataId {
        category,
        bulk_data_id,
    }): Path<BulkDataId>,
) -> Result<StatusCode> {
    let provider = entity.bulk_data_provider(&topology.read().await)?;

    provider.bulk_delete(&category, &bulk_data_id).await?;

    Ok(StatusCode::NO_CONTENT)
}
