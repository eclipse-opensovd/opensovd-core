// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

use std::{ops::Deref, sync::Arc};

use axum::{
    Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json},
    routing::{get, put},
};
use axum_extra::extract::{Query, WithRejection};
use http::{HeaderValue, request::Parts};
use opensovd_core::{Topology, TopologyReadGuard, UpdateError, Updates};
use opensovd_models::updates::{
    AvailableUpdates, AvailableUpdatesResponse, EntityPathParams, PathParams, Phase, Status,
    UpdateDetail, UpdateDetailQuery, UpdateDetailResponse, UpdateOrigins, UpdateStatus,
    UpdateStatusResponse,
};

use super::error::Result;
use crate::routes::{AppState, error::Error};
use crate::schema::JsonSchema;

pub fn routes<V>() -> Router<AppState<V>>
where
    V: Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/updates", get(list_updates).post(create_updates))
        .route(
            "/{entity-collection}/{entity-id}/updates",
            get(list_updates).post(create_updates),
        )
        .route(
            "/updates/{update-package-id}",
            get(get_update_package).delete(delete_update_package),
        )
        .route(
            "/{entity-collection}/{entity-id}/updates/{update-package-id}",
            get(get_update_package).delete(delete_update_package),
        )
        .route(
            "/updates/{update-package-id}/status",
            get(update_package_status),
        )
        .route(
            "/{entity-collection}/{entity-id}/updates/{update-package-id}/status",
            get(update_package_status),
        )
        .route(
            "/updates/{update-package-id}/automated",
            put(update_package_automated),
        )
        .route(
            "/{entity-collection}/{entity-id}/updates/{update-package-id}/automated",
            put(update_package_automated),
        )
        .route(
            "/updates/{update-package-id}/execute",
            put(update_package_execute),
        )
        .route(
            "/{entity-collection}/{entity-id}/updates/{update-package-id}/execute",
            put(update_package_execute),
        )
        .route(
            "/updates/{update-package-id}/prepare",
            put(update_package_prepare),
        )
        .route(
            "/{entity-collection}/{entity-id}/updates/{update-package-id}/prepare",
            put(update_package_prepare),
        )
}

async fn list_updates(
    Path(EntityPathParams {
        entity_collection,
        entity_id,
    }): Path<EntityPathParams>,
    State(topology): State<Topology>,
) -> Result<impl IntoResponse> {
    let topo = topology.read().await;
    Ok(Json(AvailableUpdatesResponse {
        data: AvailableUpdates {
            items: get_updates(topo, entity_collection.as_ref(), entity_id.as_ref())?
                .available()
                .await
                .iter()
                .map(|u| u.id().clone())
                .collect(),
        },
        schema: None,
    }))
}

async fn create_updates(
    Path(EntityPathParams {
        entity_collection,
        entity_id,
    }): Path<EntityPathParams>,
    State(topology): State<Topology>,
    parts: Parts,
    Json(body): Json<UpdateDetail>,
) -> Result<(StatusCode, HeaderMap)> {
    let topo = topology.read().await;
    let id = get_updates(topo, entity_collection.as_ref(), entity_id.as_ref())?
        .push(&body)
        .await?;
    let versioned_uri = super::versioned_uri(&parts);

    let mut headers = HeaderMap::new();
    let location = HeaderValue::from_str(&format!(
        "{versioned_uri}/updates/{}",
        super::entities::encode_path_segment(&id)
    ))
    .map_err(|e| UpdateError::ResponseFailed(e.to_string()))?;
    headers.insert("Location", location);

    Ok((StatusCode::CREATED, headers))
}

async fn get_update_package(
    Path(PathParams {
        entity_collection,
        entity_id,
        update_package_id,
    }): Path<PathParams>,
    State(topology): State<Topology>,
    WithRejection(Query(query), _): WithRejection<Query<UpdateDetailQuery>, Error>,
) -> Result<impl IntoResponse> {
    let topo = topology.read().await;
    get_updates(topo, entity_collection.as_ref(), entity_id.as_ref())?
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
    Path(PathParams {
        entity_collection,
        entity_id,
        update_package_id,
    }): Path<PathParams>,
    State(topology): State<Topology>,
) -> Result<StatusCode> {
    let topo = topology.read().await;
    let updates = get_updates(topo, entity_collection.as_ref(), entity_id.as_ref())?;
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
    Path(PathParams {
        entity_collection,
        entity_id,
        update_package_id,
    }): Path<PathParams>,
    State(topology): State<Topology>,
) -> Result<impl IntoResponse> {
    let topo = topology.read().await;
    let updates = get_updates(topo, entity_collection.as_ref(), entity_id.as_ref())?;
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
    Path(PathParams {
        entity_collection,
        entity_id,
        update_package_id,
    }): Path<PathParams>,
    parts: Parts,
    State(topology): State<Topology>,
) -> Result<(StatusCode, HeaderMap)> {
    let topo = topology.read().await;
    let updates = get_updates(topo, entity_collection.as_ref(), entity_id.as_ref())?;
    if let Some(id) = updates
        .all_feedback::<UpdateStatus>()
        .await
        .into_iter()
        .find_map(|(id, feedback)| {
            feedback
                .as_ref()
                .filter(|feedback| {
                    feedback.phase == Phase::Execute && feedback.status == Status::InProgress
                })
                .map(|_| id.clone())
        })
    {
        return Err(UpdateError::UpdateExecutionInProgress(id).into());
    }

    let update = updates
        .find(&update_package_id)
        .await
        .ok_or_else(|| Error::EntityNotFound(update_package_id.clone()))?;

    if update.automated() != Some(true) {
        return Err(UpdateError::AutomatedUpdateNotSupported.into());
    }

    let feedback = updates.feedback_sender(&update_package_id).await;

    updates
        .provider()
        .await
        .ok_or(UpdateError::UpdateProviderNotConfigured)?
        .automated(&*update, feedback)?;

    let versioned_uri = super::versioned_uri(&parts);

    let mut headers = HeaderMap::new();
    let location = HeaderValue::from_str(&format!(
        "{versioned_uri}/updates/{update_package_id}/status"
    ))
    .map_err(|e| UpdateError::ResponseFailed(e.to_string()))?;
    headers.insert("Location", location);

    Ok((StatusCode::ACCEPTED, headers))
}

async fn update_package_execute(
    Path(PathParams {
        entity_collection,
        entity_id,
        update_package_id,
    }): Path<PathParams>,
    parts: Parts,
    State(topology): State<Topology>,
) -> Result<(StatusCode, HeaderMap)> {
    let topo = topology.read().await;
    let updates = get_updates(topo, entity_collection.as_ref(), entity_id.as_ref())?;
    if let Some(id) = updates
        .all_feedback::<UpdateStatus>()
        .await
        .into_iter()
        .find_map(|(id, feedback)| {
            feedback
                .as_ref()
                .filter(|feedback| {
                    feedback.phase == Phase::Execute && feedback.status == Status::InProgress
                })
                .map(|_| id.clone())
        })
    {
        return Err(UpdateError::UpdateExecutionInProgress(id).into());
    }

    let update = updates
        .find(&update_package_id)
        .await
        .ok_or_else(|| Error::EntityNotFound(update_package_id.clone()))?;

    let feedback = updates.feedback_sender(&update_package_id).await;

    updates
        .provider()
        .await
        .ok_or(UpdateError::UpdateProviderNotConfigured)?
        .execute(&*update, feedback)?;

    let versioned_uri = super::versioned_uri(&parts);

    let mut headers = HeaderMap::new();
    let location = HeaderValue::from_str(&format!(
        "{versioned_uri}/updates/{update_package_id}/status"
    ))
    .map_err(|e| UpdateError::ResponseFailed(e.to_string()))?;
    headers.insert("Location", location);

    Ok((StatusCode::ACCEPTED, headers))
}

async fn update_package_prepare(
    Path(PathParams {
        entity_collection,
        entity_id,
        update_package_id,
    }): Path<PathParams>,
    parts: Parts,
    State(topology): State<Topology>,
) -> Result<(StatusCode, HeaderMap)> {
    let topo = topology.read().await;
    let updates = get_updates(topo, entity_collection.as_ref(), entity_id.as_ref())?;
    let update = updates
        .find(&update_package_id)
        .await
        .ok_or_else(|| Error::EntityNotFound(update_package_id.clone()))?;

    let feedback = updates.feedback_sender(&update_package_id).await;

    updates
        .provider()
        .await
        .ok_or(UpdateError::UpdateProviderNotConfigured)?
        .prepare(&*update, feedback)?;

    let versioned_uri = super::versioned_uri(&parts);

    let mut headers = HeaderMap::new();
    let location = HeaderValue::from_str(&format!(
        "{versioned_uri}/updates/{update_package_id}/status"
    ))
    .map_err(|e| UpdateError::ResponseFailed(e.to_string()))?;
    headers.insert("Location", location);

    Ok((StatusCode::ACCEPTED, headers))
}

fn get_updates(
    topo: TopologyReadGuard,
    entity_collection: Option<&String>,
    entity_id: Option<&String>,
) -> Result<Updates> {
    let updates = match (entity_collection, entity_id) {
        (Some(s), Some(entity_id)) if s == "components" => {
            let component = topo
                .get_component(entity_id)
                .map_err(|_| Error::EntityNotFound(entity_id.clone()))?;
            component
                .updates()
                .ok_or_else(|| Error::ResourceNotFound("updates".into()))
        }
        (Some(s), Some(entity_id)) if s == "apps" => {
            let app = topo
                .get_app(entity_id)
                .map_err(|_| Error::EntityNotFound(entity_id.clone()))?;
            app.updates()
                .ok_or_else(|| Error::ResourceNotFound("updates".into()))
        }
        (None, None) => topo
            .get_updates()
            .ok_or_else(|| Error::ResourceNotFound("updates".into())),
        _ => Err(Error::EntityNotFound(format!(
            "entity_collection: {entity_collection:?}, entity_id: {entity_id:?}",
        ))),
    };
    drop(topo);
    updates
}
