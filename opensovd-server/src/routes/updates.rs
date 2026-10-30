// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

use std::{ops::Deref, sync::Arc};

use axum::{
    Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, put},
};
use http::{HeaderValue, request::Parts};
use opensovd_core::{Topology, UpdateError};
use opensovd_models::updates::{
    AvailableUpdatesResponse, Status, UpdateDetail, UpdateDetailQuery, UpdateDetailResponse,
    UpdateOrigins, UpdatePackageId, UpdatePackagePath, UpdateQuery, UpdateStatus,
    UpdateStatusResponse,
};

use super::error::Result;
use crate::routes::{
    entity::Entity,
    error::Error,
    extract::{Json, Path, Query},
};
use crate::schema::JsonSchema;

pub fn routes() -> Router<Topology> {
    Router::new()
        .route("/updates", get(list_updates).post(create_updates))
        .route(
            "/updates/{update-package-id}",
            get(get_update_package).delete(delete_update_package),
        )
        .route(
            "/updates/{update-package-id}/status",
            get(update_package_status),
        )
        .route(
            "/updates/{update-package-id}/automated",
            put(update_package_automated),
        )
        .route(
            "/updates/{update-package-id}/execute",
            put(update_package_execute),
        )
        .route(
            "/updates/{update-package-id}/prepare",
            put(update_package_prepare),
        )
}

async fn list_updates(
    State(topology): State<Topology>,
    entity: Entity,
    Query(query): Query<UpdateQuery>,
) -> Result<impl IntoResponse> {
    let topo = topology.read().await;
    Ok(Json(AvailableUpdatesResponse {
        items: entity
            .updates_provider(&topo)?
            .available()
            .await
            .iter()
            .filter_map(|u| {
                if let Some(target_version) = &query.target_version {
                    // It is unclear how and against what the target_version
                    // should be compared from the specification. Match it
                    // against the update_name field which likely contains
                    // some version information.
                    if !u.update_name().contains(target_version) {
                        return None;
                    }
                }
                if let Some(origins) = u.origin()
                    && !origins.contains(&query.origin.to_string())
                {
                    return None;
                }
                Some(u.id().clone())
            })
            .collect(),
        schema: None,
    }))
}

async fn create_updates(
    State(topology): State<Topology>,
    entity: Entity,
    parts: Parts,
    Json(body): Json<UpdateDetail>,
) -> Result<impl IntoResponse> {
    let topo = topology.read().await;
    let id = entity.updates_provider(&topo)?.push(&body).await?;
    let versioned_uri = super::versioned_uri(&parts);

    let mut headers = HeaderMap::new();
    let location = HeaderValue::from_str(&format!(
        "{versioned_uri}{}/updates/{}",
        entity
            .path()
            .segments()
            .fold(String::new(), |acc, (c, id)| acc + "/" + c + "/" + id),
        super::discovery::encode_path_segment(&id)
    ))
    .map_err(|e| UpdateError::ResponseFailed(e.to_string()))?;
    headers.insert("Location", location);

    Ok((StatusCode::CREATED, headers, Json(UpdatePackageId { id })))
}

async fn get_update_package(
    State(topology): State<Topology>,
    entity: Entity,
    Path(UpdatePackagePath { update_package_id }): Path<UpdatePackagePath>,
    Query(query): Query<UpdateDetailQuery>,
) -> Result<impl IntoResponse> {
    let topo = topology.read().await;
    entity
        .updates_provider(&topo)?
        .find(&update_package_id)
        .await
        .map_or_else(
            || Err(Error::EntityNotFound(update_package_id)),
            |update| {
                Ok(Json(UpdateDetailResponse {
                    data: UpdateDetail {
                        id: Some(update.id()),
                        update_name: update.update_name(),
                        automated: update.automated(),
                        origin: update.origin().as_ref().map(|origins| {
                            origins
                                .iter()
                                .filter_map(|o| match o.as_str() {
                                    "remote" => Some(UpdateOrigins::Remote),
                                    "proximity" => Some(UpdateOrigins::Proximity),
                                    _ => None,
                                })
                                .collect()
                        }),
                        update_translation_id: update.update_translation_id(),
                        notes: update.notes(),
                        notes_translation_id: update.notes_translation_id(),
                        user_activity: update.user_activity(),
                        user_activity_translation_id: update.user_activity_translation_id(),
                        preconditions: update.preconditions(),
                        preconditions_translation_id: update.preconditions_translation_id(),
                        execution_conditions: update.execution_conditions(),
                        duration: update.duration(),
                        size: update.size(),
                        updated_components: update.updated_components(),
                        affected_components: update.affected_components(),
                        // the following fields are not included in the response
                        authentication: None,
                        authentication_token: None,
                        targets: vec![],
                    },
                    schema: query.include_schema.then_some(UpdateDetail::schema()),
                }))
            },
        )
}

async fn delete_update_package(
    State(topology): State<Topology>,
    entity: Entity,
    Path(UpdatePackagePath { update_package_id }): Path<UpdatePackagePath>,
) -> Result<StatusCode> {
    let topo = topology.read().await;
    let updates = entity.updates_provider(&topo)?;
    // if the update's status is InProgress, we cannot delete it
    if updates
        .feedback(&update_package_id)
        .await
        .is_some_and(|feedback: Arc<UpdateStatus>| feedback.status == Status::InProgress)
    {
        return Ok(StatusCode::METHOD_NOT_ALLOWED);
    }

    updates.remove(&update_package_id).await;
    Ok(StatusCode::NO_CONTENT)
}

async fn update_package_status(
    State(topology): State<Topology>,
    entity: Entity,
    Path(UpdatePackagePath { update_package_id }): Path<UpdatePackagePath>,
) -> Result<impl IntoResponse> {
    let topo = topology.read().await;
    let updates = entity.updates_provider(&topo)?;
    updates
        .feedback(&update_package_id)
        .await
        .map(|feedback: Arc<UpdateStatus>| {
            Json(UpdateStatusResponse {
                data: feedback.deref().clone(),
                schema: Some(UpdateStatus::schema()),
            })
        })
        .ok_or_else(|| Error::EntityNotFound(update_package_id.clone()))
}

async fn update_package_automated(
    parts: Parts,
    State(topology): State<Topology>,
    entity: Entity,
    Path(UpdatePackagePath { update_package_id }): Path<UpdatePackagePath>,
) -> Result<(StatusCode, HeaderMap)> {
    let topo = topology.read().await;
    let updates = entity.updates_provider(&topo)?;

    updates.automated(&update_package_id).await?;

    let versioned_uri = super::versioned_uri(&parts);

    let mut headers = HeaderMap::new();
    let location = HeaderValue::from_str(&format!(
        "{versioned_uri}{}/updates/{update_package_id}/status",
        entity
            .path()
            .segments()
            .fold(String::new(), |acc, (c, id)| acc + "/" + c + "/" + id)
    ))
    .map_err(|e| UpdateError::ResponseFailed(e.to_string()))?;
    headers.insert("Location", location);

    Ok((StatusCode::ACCEPTED, headers))
}

async fn update_package_execute(
    parts: Parts,
    State(topology): State<Topology>,
    entity: Entity,
    Path(UpdatePackagePath { update_package_id }): Path<UpdatePackagePath>,
) -> Result<(StatusCode, HeaderMap)> {
    let topo = topology.read().await;
    let updates = entity.updates_provider(&topo)?;

    updates.execute(&update_package_id).await?;

    let versioned_uri = super::versioned_uri(&parts);

    let mut headers = HeaderMap::new();
    let location = HeaderValue::from_str(&format!(
        "{versioned_uri}{}/updates/{update_package_id}/status",
        entity
            .path()
            .segments()
            .fold(String::new(), |acc, (c, id)| acc + "/" + c + "/" + id)
    ))
    .map_err(|e| UpdateError::ResponseFailed(e.to_string()))?;
    headers.insert("Location", location);

    Ok((StatusCode::ACCEPTED, headers))
}

async fn update_package_prepare(
    parts: Parts,
    State(topology): State<Topology>,
    entity: Entity,
    Path(UpdatePackagePath { update_package_id }): Path<UpdatePackagePath>,
) -> Result<(StatusCode, HeaderMap)> {
    let topo = topology.read().await;
    let updates = entity.updates_provider(&topo)?;

    updates.prepare(&update_package_id).await?;

    let versioned_uri = super::versioned_uri(&parts);

    let mut headers = HeaderMap::new();
    let location = HeaderValue::from_str(&format!(
        "{versioned_uri}{}/updates/{update_package_id}/status",
        entity
            .path()
            .segments()
            .fold(String::new(), |acc, (c, id)| acc + "/" + c + "/" + id)
    ))
    .map_err(|e| UpdateError::ResponseFailed(e.to_string()))?;
    headers.insert("Location", location);

    Ok((StatusCode::ACCEPTED, headers))
}
