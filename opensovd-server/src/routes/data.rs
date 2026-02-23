// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Data resource endpoints.
//!
//! Provides routes for:
//! - GET /components/{component_id}/data-categories - List data categories
//! - GET /components/{component_id}/data-groups - List data groups
//! - GET /components/{component_id}/data - List data resources
//! - GET /components/{component_id}/data/{data_id} - Read a data value
//! - PUT /components/{component_id}/data/{data_id} - Write a data value
//! - GET /apps/{app_id}/data-categories - List app data categories
//! - GET /apps/{app_id}/data-groups - List app data groups
//! - GET /apps/{app_id}/data - List app data resources
//! - GET /apps/{app_id}/data/{data_id} - Read an app data value
//! - PUT /apps/{app_id}/data/{data_id} - Write an app data value

use axum::{
    Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
    routing::get,
};
use axum_extra::extract::WithRejection;
use opensovd_core::DataFilter;
use opensovd_core::Topology;
use opensovd_models::Response;
use opensovd_models::data::{
    DataCategories, DataCategoryInformation, DataGroups, DataGroupsQuery, DataList, DataQuery,
    Group, Metadata, ReadDataQuery, ReadResponse, WriteRequest,
};

use super::AppState;
use super::error::{Error, Result};
use crate::schema::JsonSchema;

pub fn routes<V>() -> Router<AppState<V>>
where
    V: Clone + Send + Sync + 'static,
{
    Router::new()
        .route(
            "/components/{component_id}/data-categories",
            get(component_data_categories),
        )
        .route(
            "/components/{component_id}/data-groups",
            get(component_data_groups),
        )
        .route("/components/{component_id}/data", get(component_data_list))
        .route(
            "/components/{component_id}/data/{data_id}",
            get(component_data_read).put(component_data_write),
        )
        .route("/apps/{app_id}/data-categories", get(app_data_categories))
        .route("/apps/{app_id}/data-groups", get(app_data_groups))
        .route("/apps/{app_id}/data", get(app_data_list))
        .route(
            "/apps/{app_id}/data/{data_id}",
            get(app_data_read).put(app_data_write),
        )
}

/// GET /components/{component_id}/data-categories - List data categories.
///
/// Returns the data categories provided by a component.
async fn component_data_categories(
    State(topology): State<Topology>,
    Path(component_id): Path<String>,
) -> Result<Json<Response<DataCategories>>> {
    let topo = topology.read().await;
    let entity = topo
        .get_component(&component_id)
        .map_err(|_| Error::EntityNotFound(component_id.clone()))?;
    let provider = entity
        .data_provider()
        .ok_or_else(|| Error::ProviderNotAvailable("data".into()))?;

    let items = provider
        .categories()
        .await?
        .into_iter()
        .map(|c| DataCategoryInformation {
            item: c.category.into(),
            category_translation_id: c.translation_id,
        })
        .collect();

    Ok(Json(Response {
        data: DataCategories { items },
        schema: None,
    }))
}

/// GET /components/{component_id}/data-groups - List data groups.
///
/// Returns the groups defined for a component, optionally filtered by category.
async fn component_data_groups(
    State(topology): State<Topology>,
    Path(component_id): Path<String>,
    WithRejection(Query(query), _): WithRejection<Query<DataGroupsQuery>, Error>,
) -> Result<Json<Response<DataGroups>>> {
    let topo = topology.read().await;
    let entity = topo
        .get_component(&component_id)
        .map_err(|_| Error::EntityNotFound(component_id.clone()))?;
    let provider = entity
        .data_provider()
        .ok_or_else(|| Error::ProviderNotAvailable("data".into()))?;

    let category_filter = query
        .category
        .as_ref()
        .map(opensovd_models::data::DataCategory::as_str);
    let items = provider
        .groups(category_filter)
        .await?
        .into_iter()
        .map(|g| Group {
            id: g.id,
            category: g.category.into(),
            category_translation_id: g.category_translation_id,
            group: g.group,
            group_translation_id: g.group_translation_id,
        })
        .collect();

    Ok(Json(Response {
        data: DataGroups { items },
        schema: None,
    }))
}

/// GET /components/{component_id}/data - List data resources.
///
/// Returns the list of data resources available for a component, optionally
/// filtered by category, group, or tags.
async fn component_data_list(
    State(topology): State<Topology>,
    Path(component_id): Path<String>,
    WithRejection(Query(query), _): WithRejection<Query<DataQuery>, Error>,
) -> Result<Json<Response<DataList>>> {
    let topo = topology.read().await;
    let entity = topo
        .get_component(&component_id)
        .map_err(|_| Error::EntityNotFound(component_id.clone()))?;
    let provider = entity
        .data_provider()
        .ok_or_else(|| Error::ProviderNotAvailable("data".into()))?;

    let filter = DataFilter {
        groups: query.groups.clone().unwrap_or_default(),
        categories: query.categories.clone().unwrap_or_default(),
        tags: query.tags.clone().unwrap_or_default(),
    };

    let items = provider
        .list(filter)
        .await?
        .into_iter()
        .map(|m| Metadata {
            id: m.id,
            name: m.name,
            category: m.category.into(),
            translation_id: m.translation_id,
            groups: (!m.groups.is_empty()).then_some(m.groups),
            tags: (!m.tags.is_empty()).then_some(m.tags),
        })
        .collect();

    Ok(Json(Response {
        data: DataList { items },
        schema: query.include_schema.then(DataList::schema),
    }))
}

/// GET /components/{component_id}/data/{data_id} - Read a data value.
///
/// Retrieves the value of a single data resource from a component.
async fn component_data_read(
    State(topology): State<Topology>,
    Path((component_id, data_id)): Path<(String, String)>,
    WithRejection(Query(query), _): WithRejection<Query<ReadDataQuery>, Error>,
) -> Result<Json<ReadResponse>> {
    let topo = topology.read().await;
    let entity = topo
        .get_component(&component_id)
        .map_err(|_| Error::EntityNotFound(component_id.clone()))?;
    let provider = entity
        .data_provider()
        .ok_or_else(|| Error::ProviderNotAvailable("data".into()))?;

    let value = provider.read(&data_id, query.include_schema).await?;

    Ok(Json(ReadResponse {
        id: data_id,
        data: value.data,
        errors: None,
        schema: value.schema,
    }))
}

/// PUT /components/{component_id}/data/{data_id} - Write a data value.
///
/// Writes a value to a data resource of a component.
async fn component_data_write(
    State(topology): State<Topology>,
    Path((component_id, data_id)): Path<(String, String)>,
    Json(body): Json<WriteRequest>,
) -> Result<StatusCode> {
    let topo = topology.read().await;
    let entity = topo
        .get_component(&component_id)
        .map_err(|_| Error::EntityNotFound(component_id.clone()))?;
    let provider = entity
        .data_provider()
        .ok_or_else(|| Error::ProviderNotAvailable("data".into()))?;

    provider.write(&data_id, body.data).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// GET /apps/{app_id}/data-categories - List app data categories.
///
/// Returns the data categories provided by an app.
async fn app_data_categories(
    State(topology): State<Topology>,
    Path(app_id): Path<String>,
) -> Result<Json<Response<DataCategories>>> {
    let topo = topology.read().await;
    let entity = topo
        .get_app(&app_id)
        .map_err(|_| Error::EntityNotFound(app_id.clone()))?;
    let provider = entity
        .data_provider()
        .ok_or_else(|| Error::ProviderNotAvailable("data".into()))?;

    let items = provider
        .categories()
        .await?
        .into_iter()
        .map(|c| DataCategoryInformation {
            item: c.category.into(),
            category_translation_id: c.translation_id,
        })
        .collect();

    Ok(Json(Response {
        data: DataCategories { items },
        schema: None,
    }))
}

/// GET /apps/{app_id}/data-groups - List app data groups.
///
/// Returns the groups defined for an app, optionally filtered by category.
async fn app_data_groups(
    State(topology): State<Topology>,
    Path(app_id): Path<String>,
    WithRejection(Query(query), _): WithRejection<Query<DataGroupsQuery>, Error>,
) -> Result<Json<Response<DataGroups>>> {
    let topo = topology.read().await;
    let entity = topo
        .get_app(&app_id)
        .map_err(|_| Error::EntityNotFound(app_id.clone()))?;
    let provider = entity
        .data_provider()
        .ok_or_else(|| Error::ProviderNotAvailable("data".into()))?;

    let category_filter = query
        .category
        .as_ref()
        .map(opensovd_models::data::DataCategory::as_str);
    let items = provider
        .groups(category_filter)
        .await?
        .into_iter()
        .map(|g| Group {
            id: g.id,
            category: g.category.into(),
            category_translation_id: g.category_translation_id,
            group: g.group,
            group_translation_id: g.group_translation_id,
        })
        .collect();

    Ok(Json(Response {
        data: DataGroups { items },
        schema: None,
    }))
}

/// GET /apps/{app_id}/data - List app data resources.
///
/// Returns the list of data resources available for an app, optionally
/// filtered by category, group, or tags.
async fn app_data_list(
    State(topology): State<Topology>,
    Path(app_id): Path<String>,
    WithRejection(Query(query), _): WithRejection<Query<DataQuery>, Error>,
) -> Result<Json<Response<DataList>>> {
    let topo = topology.read().await;
    let entity = topo
        .get_app(&app_id)
        .map_err(|_| Error::EntityNotFound(app_id.clone()))?;
    let provider = entity
        .data_provider()
        .ok_or_else(|| Error::ProviderNotAvailable("data".into()))?;

    let filter = DataFilter {
        groups: query.groups.clone().unwrap_or_default(),
        categories: query.categories.clone().unwrap_or_default(),
        tags: query.tags.clone().unwrap_or_default(),
    };

    let items = provider
        .list(filter)
        .await?
        .into_iter()
        .map(|m| Metadata {
            id: m.id,
            name: m.name,
            category: m.category.into(),
            translation_id: m.translation_id,
            groups: (!m.groups.is_empty()).then_some(m.groups),
            tags: (!m.tags.is_empty()).then_some(m.tags),
        })
        .collect();

    Ok(Json(Response {
        data: DataList { items },
        schema: query.include_schema.then(DataList::schema),
    }))
}

/// GET /apps/{app_id}/data/{data_id} - Read an app data value.
///
/// Retrieves the value of a single data resource from an app.
async fn app_data_read(
    State(topology): State<Topology>,
    Path((app_id, data_id)): Path<(String, String)>,
    WithRejection(Query(query), _): WithRejection<Query<ReadDataQuery>, Error>,
) -> Result<Json<ReadResponse>> {
    let topo = topology.read().await;
    let entity = topo
        .get_app(&app_id)
        .map_err(|_| Error::EntityNotFound(app_id.clone()))?;
    let provider = entity
        .data_provider()
        .ok_or_else(|| Error::ProviderNotAvailable("data".into()))?;

    let value = provider.read(&data_id, query.include_schema).await?;

    Ok(Json(ReadResponse {
        id: data_id,
        data: value.data,
        errors: None,
        schema: value.schema,
    }))
}

/// PUT /apps/{app_id}/data/{data_id} - Write an app data value.
///
/// Writes a value to a data resource of an app.
async fn app_data_write(
    State(topology): State<Topology>,
    Path((app_id, data_id)): Path<(String, String)>,
    Json(body): Json<WriteRequest>,
) -> Result<StatusCode> {
    let topo = topology.read().await;
    let entity = topo
        .get_app(&app_id)
        .map_err(|_| Error::EntityNotFound(app_id.clone()))?;
    let provider = entity
        .data_provider()
        .ok_or_else(|| Error::ProviderNotAvailable("data".into()))?;

    provider.write(&data_id, body.data).await?;
    Ok(StatusCode::NO_CONTENT)
}
