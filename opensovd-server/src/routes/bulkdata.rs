// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Bulk-Data resource endpoints.
//!
//! Provides routes for:
//! - GET /{entity-collection}/{entity-id}/bulk-data - List bulk-data categories
//! - GET /{entity-collection}/{entity-id}/bulk-data/{category} - List of BulkDataDescriptors for a specific category
//! - POST /{entity-collection}/{entity-id}/bulk-data/{category} - Upload bulk data to the SOVD server and create a resource for the bulk data
//! - DELETE /{entity-collection}/{entity-id}/bulk-data/{category} - Delete all bulk data resources for a specific category
//! - GET /{entity-collection}/{entity-id}/bulk-data/{category}/{bulk-data-id} - Download a specific bulk data resource
//! - DELETE /{entity-collection}/{entity-id}/bulk-data/{category}/{bulk-data-id} - Delete a specific bulk data resource

use std::sync::Arc;

use axum::{
    Json, Router,
    body::Body,
    extract::{DefaultBodyLimit, FromRequest, Multipart, Path, Request, State},
    response::IntoResponse,
    routing::get,
};
use axum_extra::{
    TypedHeader,
    extract::{Query, WithRejection},
    headers::{ContentLength, ContentType, Header},
};
use futures::StreamExt;
use http::{HeaderMap, HeaderName, HeaderValue, StatusCode, request::Parts};
use opensovd_core::{BulkDataError, BulkDataProvider, CategoryFilter, Topology, TopologyReadGuard};
use opensovd_models::{
    Response,
    bulkdata::{
        AvailableBulkDataCategories, BulkDataCategoriesQuery, BulkDataCategory, BulkDataDescriptor,
        BulkDataDescriptorsQuery, BulkDataMetadata, BulkDataUpload,
    },
    types::SupportedTags,
};

use crate::routes::{
    AppState,
    error::{Error, Result},
};
use crate::schema::JsonSchema;

// Maximum allowed size for bulk data uploads (1 GiB).
const MAX_BULKDATA_BODY_BYTES: usize = 1024 * 1024 * 1024;

pub fn routes<V>() -> Router<AppState<V>>
where
    V: Clone + Send + Sync + 'static,
{
    Router::new()
        .route(
            "/{entity-collection}/{entity-id}/bulk-data",
            get(bulk_data_categories),
        )
        .route(
            "/{entity-collection}/{entity-id}/bulk-data/{category}",
            get(bulk_data_descriptors)
                .post(upload_bulk_data)
                .delete(delete_bulk_data_category),
        )
        .route(
            "/{entity-collection}/{entity-id}/bulk-data/{category}/{bulk-data-id}",
            get(download_bulk_data).delete(delete_bulk_data),
        )
        .layer(DefaultBodyLimit::max(MAX_BULKDATA_BODY_BYTES))
}

async fn bulk_data_categories(
    State(topology): State<Topology>,
    Path((entity_collection, entity_id)): Path<(String, String)>,
    WithRejection(Query(query), _): WithRejection<Query<BulkDataCategoriesQuery>, Error>,
) -> Result<Json<Response<AvailableBulkDataCategories>>> {
    let topo = topology.read().await;
    let provider = get_provider(topo, &entity_collection, &entity_id)?;
    Ok(Json(Response {
        data: AvailableBulkDataCategories {
            items: provider
                .categories()
                .await?
                .iter()
                .map(|c| BulkDataCategory(c.category.clone()))
                .collect::<Vec<_>>(),
        },
        schema: query
            .include_schema
            .then_some(AvailableBulkDataCategories::schema()),
    }))
}

fn category_filter(query: &BulkDataDescriptorsQuery) -> CategoryFilter {
    CategoryFilter {
        created_before: query.created_before,
        created_after: query.created_after,
        tags: query.tags.clone(),
    }
}

async fn bulk_data_descriptors(
    State(topology): State<Topology>,
    Path((entity_collection, entity_id, category)): Path<(String, String, String)>,
    WithRejection(Query(query), _): WithRejection<Query<BulkDataDescriptorsQuery>, Error>,
) -> Result<Json<Response<BulkDataMetadata>>> {
    let topo = topology.read().await;
    let provider = get_provider(topo, &entity_collection, &entity_id)?;

    Ok(Json(Response {
        data: BulkDataMetadata {
            items: provider
                .list(&category, category_filter(&query))
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
                    tags: metadata
                        .tags
                        .as_ref()
                        .map(|tags| SupportedTags(tags.clone())),
                })
                .collect::<Vec<_>>(),
        },
        schema: query.include_schema.then_some(BulkDataMetadata::schema()),
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

async fn upload_bulk_data(
    State(topology): State<Topology>,
    parts: Parts,
    Path((entity_collection, entity_id, category)): Path<(String, String, String)>,
    WithRejection(TypedHeader(content_type), _): WithRejection<TypedHeader<ContentType>, Error>,
    WithRejection(TypedHeader(content_disposition), _): WithRejection<
        TypedHeader<ContentDisposition>,
        Error,
    >,
    WithRejection(TypedHeader(content_length), _): WithRejection<TypedHeader<ContentLength>, Error>,
    req: Request,
) -> Result<(StatusCode, HeaderMap, Json<Response<BulkDataUpload>>)> {
    let topo = topology.read().await;
    let provider = get_provider(topo, &entity_collection, &entity_id)?;

    let base_uri = super::base_uri(&parts);
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
            .upload(&category, &filename, content_length.0, &mut stream, None)
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
                        .upload(&category, &filename, length, &mut data_stream, sig.as_ref())
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
        "{base_uri}/{entity_collection}/{entity_id}/bulk-data/{category}/{filename}"
    ))
    .map_err(|e| BulkDataError::Internal(e.to_string()))?;
    headers.insert("Location", location);

    Ok((
        StatusCode::CREATED,
        headers,
        Json(Response {
            data: BulkDataUpload { id: filename },
            schema: Some(BulkDataUpload::schema()),
        }),
    ))
}

async fn delete_bulk_data_category(
    State(topology): State<Topology>,
    Path((entity_collection, entity_id, category)): Path<(String, String, String)>,
) -> Result<StatusCode> {
    let topo = topology.read().await;
    let provider = get_provider(topo, &entity_collection, &entity_id)?;

    provider.delete(&category, None).await?;

    Ok(StatusCode::OK)
}

async fn download_bulk_data(
    State(topology): State<Topology>,
    Path((entity_collection, entity_id, category, bulk_data_id)): Path<(
        String,
        String,
        String,
        String,
    )>,
) -> Result<impl IntoResponse> {
    let topo = topology.read().await;
    let provider = get_provider(topo, &entity_collection, &entity_id)?;

    let bulkdata = provider.download(&category, &bulk_data_id).await?;

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

async fn delete_bulk_data(
    State(topology): State<Topology>,
    Path((entity_collection, entity_id, category, bulk_data_id)): Path<(
        String,
        String,
        String,
        String,
    )>,
) -> Result<StatusCode> {
    let topo = topology.read().await;
    let provider = get_provider(topo, &entity_collection, &entity_id)?;

    provider.delete(&category, Some(&bulk_data_id)).await?;

    Ok(StatusCode::NO_CONTENT)
}

fn get_provider(
    topo: TopologyReadGuard,
    entity_collection: &str,
    entity_id: &str,
) -> Result<Arc<dyn BulkDataProvider>> {
    let provider = match entity_collection {
        "components" => {
            let component = topo
                .get_component(entity_id)
                .map_err(|_| Error::EntityNotFound(entity_id.to_string()))?;
            component
                .bulkdata_provider()
                .ok_or_else(|| Error::ProviderNotAvailable("bulkdata".into()))
        }
        "apps" => {
            let app = topo
                .get_app(entity_id)
                .map_err(|_| Error::EntityNotFound(entity_id.to_string()))?;
            app.bulkdata_provider()
                .ok_or_else(|| Error::ProviderNotAvailable("bulkdata".into()))
        }
        _ => Err(Error::EntityNotFound(entity_collection.to_string())),
    };
    drop(topo);
    provider
}
