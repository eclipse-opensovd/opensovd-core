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
    extract::{OriginalUri, Path, State},
    http::StatusCode,
    response::Json,
    routing::get,
};
use axum_extra::extract::{Query, WithRejection};
use opensovd_core::{DataFilter, DataScope, Topology};
use opensovd_models::data::{
    DataCategories, DataCategoryInformation, DataGroups, DataGroupsQuery, DataList, DataQuery,
    Group, Metadata, ReadDataQuery, ReadResponse, WriteRequest,
};
use serde_json::Value;

use super::AppState;
use super::error::{Error, Result};
use crate::schema::{JsonSchema, normalize};

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
    OriginalUri(uri): OriginalUri,
    WithRejection(Path(component_id), _): WithRejection<Path<String>, Error>,
) -> Result<Json<DataCategories>> {
    let topo = topology.read().await;
    let entity = topo
        .get_component(&component_id)
        .map_err(|_| Error::EntityNotFound(component_id.clone()))?;
    let provider = entity
        .data_provider()
        .ok_or_else(|| Error::ResourceNotFound(uri.path().to_owned()))?;

    let items = provider
        .data_categories()
        .await?
        .into_iter()
        .map(|c| DataCategoryInformation {
            item: c.category.into(),
            category_translation_id: c.translation_id,
        })
        .collect();

    Ok(Json(DataCategories {
        items,
        schema: None,
    }))
}

/// GET /components/{component_id}/data-groups - List data groups.
///
/// Returns the groups defined for a component, optionally filtered by category.
async fn component_data_groups(
    State(topology): State<Topology>,
    OriginalUri(uri): OriginalUri,
    WithRejection(Path(component_id), _): WithRejection<Path<String>, Error>,
    WithRejection(Query(query), _): WithRejection<Query<DataGroupsQuery>, Error>,
) -> Result<Json<DataGroups>> {
    let topo = topology.read().await;
    let entity = topo
        .get_component(&component_id)
        .map_err(|_| Error::EntityNotFound(component_id.clone()))?;
    let provider = entity
        .data_provider()
        .ok_or_else(|| Error::ResourceNotFound(uri.path().to_owned()))?;

    let category_filter = query
        .category
        .as_ref()
        .map(opensovd_models::data::DataCategory::as_str);
    let items = provider
        .data_groups(category_filter)
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

    Ok(Json(DataGroups {
        items,
        schema: None,
    }))
}

/// Resolve a `DataQuery` into a provider `DataFilter`.
///
/// Applies groups/categories precedence: when both are given, groups wins and
/// categories is ignored.
fn data_filter(query: DataQuery) -> DataFilter {
    let groups = query.groups.unwrap_or_default();
    let categories = query.categories.unwrap_or_default();

    let scope = if !groups.is_empty() {
        Some(DataScope::Groups(groups))
    } else if !categories.is_empty() {
        Some(DataScope::Categories(categories))
    } else {
        None
    };

    DataFilter {
        scope,
        tags: query.tags.unwrap_or_default(),
    }
}

/// Schema of a read response, with the data value schema in place of `data`.
fn read_response_schema(data: Value) -> Value {
    let mut schema = ReadResponse::schema();
    // Without `jsonschema`, the response schema has no properties.
    if let Some(properties) = schema.as_object_mut().and_then(|object| {
        object
            .entry("properties")
            .or_insert_with(|| serde_json::json!({}))
            .as_object_mut()
    }) {
        properties.insert("data".into(), normalize(data));
    }
    schema
}

/// GET /components/{component_id}/data - List data resources.
///
/// Returns the list of data resources available for a component, optionally
/// filtered by category, group, or tags.
async fn component_data_list(
    State(topology): State<Topology>,
    OriginalUri(uri): OriginalUri,
    WithRejection(Path(component_id), _): WithRejection<Path<String>, Error>,
    WithRejection(Query(query), _): WithRejection<Query<DataQuery>, Error>,
) -> Result<Json<DataList>> {
    let topo = topology.read().await;
    let entity = topo
        .get_component(&component_id)
        .map_err(|_| Error::EntityNotFound(component_id.clone()))?;
    let provider = entity
        .data_provider()
        .ok_or_else(|| Error::ResourceNotFound(uri.path().to_owned()))?;

    let include_schema = query.include_schema;
    let filter = data_filter(query);

    let items = provider
        .data_list(filter)
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

    Ok(Json(DataList {
        items,
        schema: include_schema.then(DataList::schema),
    }))
}

/// GET /components/{component_id}/data/{data_id} - Read a data value.
///
/// Retrieves the value of a single data resource from a component.
async fn component_data_read(
    State(topology): State<Topology>,
    OriginalUri(uri): OriginalUri,
    WithRejection(Path((component_id, data_id)), _): WithRejection<Path<(String, String)>, Error>,
    WithRejection(Query(query), _): WithRejection<Query<ReadDataQuery>, Error>,
) -> Result<Json<ReadResponse>> {
    let topo = topology.read().await;
    let entity = topo
        .get_component(&component_id)
        .map_err(|_| Error::EntityNotFound(component_id.clone()))?;
    let provider = entity
        .data_provider()
        .ok_or_else(|| Error::ResourceNotFound(uri.path().to_owned()))?;

    let value = provider.data_read(&data_id, query.include_schema).await?;
    let schema = query
        .include_schema
        .then(|| read_response_schema(value.schema.unwrap_or(Value::Bool(true))));

    Ok(Json(ReadResponse {
        id: data_id,
        data: value.data,
        errors: None,
        schema,
    }))
}

/// PUT /components/{component_id}/data/{data_id} - Write a data value.
///
/// Writes a value to a data resource of a component.
async fn component_data_write(
    State(topology): State<Topology>,
    OriginalUri(uri): OriginalUri,
    WithRejection(Path((component_id, data_id)), _): WithRejection<Path<(String, String)>, Error>,
    WithRejection(Json(body), _): WithRejection<Json<WriteRequest>, Error>,
) -> Result<StatusCode> {
    let topo = topology.read().await;
    let entity = topo
        .get_component(&component_id)
        .map_err(|_| Error::EntityNotFound(component_id.clone()))?;
    let provider = entity
        .data_provider()
        .ok_or_else(|| Error::ResourceNotFound(uri.path().to_owned()))?;

    provider.data_write(&data_id, body.data).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// GET /apps/{app_id}/data-categories - List app data categories.
///
/// Returns the data categories provided by an app.
async fn app_data_categories(
    State(topology): State<Topology>,
    OriginalUri(uri): OriginalUri,
    WithRejection(Path(app_id), _): WithRejection<Path<String>, Error>,
) -> Result<Json<DataCategories>> {
    let topo = topology.read().await;
    let entity = topo
        .get_app(&app_id)
        .map_err(|_| Error::EntityNotFound(app_id.clone()))?;
    let provider = entity
        .data_provider()
        .ok_or_else(|| Error::ResourceNotFound(uri.path().to_owned()))?;

    let items = provider
        .data_categories()
        .await?
        .into_iter()
        .map(|c| DataCategoryInformation {
            item: c.category.into(),
            category_translation_id: c.translation_id,
        })
        .collect();

    Ok(Json(DataCategories {
        items,
        schema: None,
    }))
}

/// GET /apps/{app_id}/data-groups - List app data groups.
///
/// Returns the groups defined for an app, optionally filtered by category.
async fn app_data_groups(
    State(topology): State<Topology>,
    OriginalUri(uri): OriginalUri,
    WithRejection(Path(app_id), _): WithRejection<Path<String>, Error>,
    WithRejection(Query(query), _): WithRejection<Query<DataGroupsQuery>, Error>,
) -> Result<Json<DataGroups>> {
    let topo = topology.read().await;
    let entity = topo
        .get_app(&app_id)
        .map_err(|_| Error::EntityNotFound(app_id.clone()))?;
    let provider = entity
        .data_provider()
        .ok_or_else(|| Error::ResourceNotFound(uri.path().to_owned()))?;

    let category_filter = query
        .category
        .as_ref()
        .map(opensovd_models::data::DataCategory::as_str);
    let items = provider
        .data_groups(category_filter)
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

    Ok(Json(DataGroups {
        items,
        schema: None,
    }))
}

/// GET /apps/{app_id}/data - List app data resources.
///
/// Returns the list of data resources available for an app, optionally
/// filtered by category, group, or tags.
async fn app_data_list(
    State(topology): State<Topology>,
    OriginalUri(uri): OriginalUri,
    WithRejection(Path(app_id), _): WithRejection<Path<String>, Error>,
    WithRejection(Query(query), _): WithRejection<Query<DataQuery>, Error>,
) -> Result<Json<DataList>> {
    let topo = topology.read().await;
    let entity = topo
        .get_app(&app_id)
        .map_err(|_| Error::EntityNotFound(app_id.clone()))?;
    let provider = entity
        .data_provider()
        .ok_or_else(|| Error::ResourceNotFound(uri.path().to_owned()))?;

    let include_schema = query.include_schema;
    let filter = data_filter(query);

    let items = provider
        .data_list(filter)
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

    Ok(Json(DataList {
        items,
        schema: include_schema.then(DataList::schema),
    }))
}

/// GET /apps/{app_id}/data/{data_id} - Read an app data value.
///
/// Retrieves the value of a single data resource from an app.
async fn app_data_read(
    State(topology): State<Topology>,
    OriginalUri(uri): OriginalUri,
    WithRejection(Path((app_id, data_id)), _): WithRejection<Path<(String, String)>, Error>,
    WithRejection(Query(query), _): WithRejection<Query<ReadDataQuery>, Error>,
) -> Result<Json<ReadResponse>> {
    let topo = topology.read().await;
    let entity = topo
        .get_app(&app_id)
        .map_err(|_| Error::EntityNotFound(app_id.clone()))?;
    let provider = entity
        .data_provider()
        .ok_or_else(|| Error::ResourceNotFound(uri.path().to_owned()))?;

    let value = provider.data_read(&data_id, query.include_schema).await?;
    let schema = query
        .include_schema
        .then(|| read_response_schema(value.schema.unwrap_or(Value::Bool(true))));

    Ok(Json(ReadResponse {
        id: data_id,
        data: value.data,
        errors: None,
        schema,
    }))
}

/// PUT /apps/{app_id}/data/{data_id} - Write an app data value.
///
/// Writes a value to a data resource of an app.
async fn app_data_write(
    State(topology): State<Topology>,
    OriginalUri(uri): OriginalUri,
    WithRejection(Path((app_id, data_id)), _): WithRejection<Path<(String, String)>, Error>,
    WithRejection(Json(body), _): WithRejection<Json<WriteRequest>, Error>,
) -> Result<StatusCode> {
    let topo = topology.read().await;
    let entity = topo
        .get_app(&app_id)
        .map_err(|_| Error::EntityNotFound(app_id.clone()))?;
    let provider = entity
        .data_provider()
        .ok_or_else(|| Error::ResourceNotFound(uri.path().to_owned()))?;

    provider.data_write(&data_id, body.data).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query(groups: Vec<&str>, categories: Vec<&str>, tags: Vec<&str>) -> DataQuery {
        DataQuery {
            groups: Some(groups.into_iter().map(String::from).collect()),
            categories: Some(categories.into_iter().map(String::from).collect()),
            tags: Some(tags.into_iter().map(String::from).collect()),
            include_schema: false,
        }
    }

    #[test]
    fn groups_take_precedence_over_categories() {
        let filter = data_filter(query(vec!["g"], vec!["c"], vec![]));
        assert_eq!(filter.scope, Some(DataScope::Groups(vec!["g".into()])));
    }

    #[test]
    fn no_scope_leaves_tags_only() {
        let filter = data_filter(query(vec![], vec![], vec!["t"]));
        assert_eq!(filter.scope, None);
        assert_eq!(filter.tags, vec!["t"]);
    }

    #[cfg(feature = "jsonschema")]
    #[test]
    fn read_response_schema_puts_value_schema_in_place_of_data() {
        let schema = read_response_schema(serde_json::json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "title": "Level",
            "type": "integer",
        }));
        assert_eq!(
            schema.pointer("/properties/data"),
            Some(&serde_json::json!({ "type": "integer" }))
        );
        assert_eq!(
            schema.pointer("/properties/id"),
            Some(&serde_json::json!({ "type": "string" }))
        );
    }
}
