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
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    routing::get,
};
use axum_extra::extract::{Query, WithRejection};
use opensovd_core::{DataFilter, DataScope, Topology};
use opensovd_models::Response;
use opensovd_models::data::{
    DataCategories, DataCategoryInformation, DataGroups, DataGroupsQuery, DataList, DataQuery,
    Group, Metadata, ReadDataQuery, ReadResponse, WriteRequest,
};
use serde_json::{Map, Value};

use super::AppState;
use super::entities::encode_path_segment;
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
        .data_categories()
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

    Ok(Json(Response {
        data: DataGroups { items },
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

/// `$id` of the schema of a data resource, unique per entity and data id.
fn data_schema_id(collection: &str, entity_id: &str, data_id: &str) -> String {
    format!(
        "urn:opensovd:{collection}/{}/data/{}",
        encode_path_segment(entity_id),
        encode_path_segment(data_id)
    )
}

/// Schema of a read response, with the data value schema in place of `data`.
///
/// The data schema keeps its own `$id` or gets `id`, so its references keep
/// resolving against itself.
fn read_response_schema(id: &str, mut data: Value) -> Value {
    if let Some(object) = data.as_object_mut() {
        let mut rest = std::mem::take(object);
        let own_id = rest.remove("$id").filter(Value::is_string);
        object.insert("$id".into(), own_id.unwrap_or_else(|| id.into()));
        // ajv recurses forever on a `$ref` next to the `$id` of a subschema;
        // `allOf` applies it the same way.
        if let Some(reference) = rest.remove("$ref") {
            let reference = serde_json::json!({ "$ref": reference });
            match rest.get_mut("allOf") {
                Some(Value::Array(all_of)) => all_of.push(reference),
                _ => {
                    rest.insert("allOf".into(), Value::Array(vec![reference]));
                }
            }
        }
        object.extend(rest);
    }

    let mut schema = match ReadResponse::schema() {
        Value::Object(schema) => schema,
        _ => Map::new(),
    };
    if let Some(properties) = schema
        .entry("properties")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
    {
        properties.insert("data".into(), data);
    }
    schema.into()
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

    Ok(Json(Response {
        data: DataList { items },
        schema: include_schema.then(DataList::schema),
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

    let value = provider.data_read(&data_id, query.include_schema).await?;
    let schema = query.include_schema.then(|| {
        read_response_schema(
            &data_schema_id("components", &component_id, &data_id),
            value.schema.unwrap_or(Value::Bool(true)),
        )
    });

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
    Path((component_id, data_id)): Path<(String, String)>,
    WithRejection(Json(body), _): WithRejection<Json<WriteRequest>, Error>,
) -> Result<StatusCode> {
    let topo = topology.read().await;
    let entity = topo
        .get_component(&component_id)
        .map_err(|_| Error::EntityNotFound(component_id.clone()))?;
    let provider = entity
        .data_provider()
        .ok_or_else(|| Error::ProviderNotAvailable("data".into()))?;

    provider.data_write(&data_id, body.data).await?;
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
        .data_categories()
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

    Ok(Json(Response {
        data: DataList { items },
        schema: include_schema.then(DataList::schema),
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

    let value = provider.data_read(&data_id, query.include_schema).await?;
    let schema = query.include_schema.then(|| {
        read_response_schema(
            &data_schema_id("apps", &app_id, &data_id),
            value.schema.unwrap_or(Value::Bool(true)),
        )
    });

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
    Path((app_id, data_id)): Path<(String, String)>,
    WithRejection(Json(body), _): WithRejection<Json<WriteRequest>, Error>,
) -> Result<StatusCode> {
    let topo = topology.read().await;
    let entity = topo
        .get_app(&app_id)
        .map_err(|_| Error::EntityNotFound(app_id.clone()))?;
    let provider = entity
        .data_provider()
        .ok_or_else(|| Error::ProviderNotAvailable("data".into()))?;

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

    const ID: &str = "urn:opensovd:components/climate/data/level";

    #[test]
    fn data_schema_id_encodes_ids() {
        assert_eq!(
            data_schema_id("apps", "radio 1", "station/name"),
            "urn:opensovd:apps/radio%201/data/station%2Fname"
        );
    }

    #[cfg(feature = "jsonschema")]
    #[test]
    fn read_response_schema_embeds_data_schema_as_resource() {
        let data = serde_json::json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "properties": {
                "error": { "$ref": "#/$defs/GenericError" },
                "next": { "$ref": "#" },
            },
            "$defs": { "GenericError": { "type": "integer" } },
        });
        let schema = read_response_schema(ID, data.clone());
        let mut embedded = schema.pointer("/properties/data").cloned().unwrap();
        assert_eq!(
            embedded
                .as_object_mut()
                .and_then(|object| object.remove("$id")),
            Some(ID.into())
        );
        assert_eq!(embedded, data);
        assert_eq!(
            schema.pointer("/properties/id"),
            Some(&serde_json::json!({ "type": "string" }))
        );
        assert_eq!(
            schema.pointer("/$defs/GenericError"),
            ReadResponse::schema().pointer("/$defs/GenericError")
        );
    }

    #[test]
    fn read_response_schema_keeps_data_schema_with_id() {
        let data = serde_json::json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "$id": "urn:example:level",
            "properties": { "level": { "$ref": "#/$defs/Level" } },
            "$defs": { "Level": { "type": "integer" } },
        });
        let schema = read_response_schema(ID, data.clone());
        assert_eq!(schema.pointer("/properties/data"), Some(&data));
    }

    #[test]
    fn read_response_schema_moves_root_ref_into_all_of() {
        let schema = read_response_schema(
            ID,
            serde_json::json!({
                "$ref": "#/$defs/Level",
                "allOf": [{ "minimum": 1 }],
                "$defs": { "Level": { "type": "integer" } },
            }),
        );
        assert_eq!(
            schema.pointer("/properties/data"),
            Some(&serde_json::json!({
                "$id": ID,
                "allOf": [{ "minimum": 1 }, { "$ref": "#/$defs/Level" }],
                "$defs": { "Level": { "type": "integer" } },
            }))
        );
    }

    #[test]
    fn read_response_schema_moves_root_ref_next_to_own_id() {
        let schema = read_response_schema(
            ID,
            serde_json::json!({
                "$id": "urn:example:level",
                "$ref": "#/$defs/Level",
                "$defs": { "Level": { "type": "integer" } },
            }),
        );
        assert_eq!(
            schema.pointer("/properties/data"),
            Some(&serde_json::json!({
                "$id": "urn:example:level",
                "allOf": [{ "$ref": "#/$defs/Level" }],
                "$defs": { "Level": { "type": "integer" } },
            }))
        );
    }

    #[test]
    fn read_response_schema_replaces_non_string_id() {
        let schema =
            read_response_schema(ID, serde_json::json!({ "$id": null, "type": "integer" }));
        assert_eq!(
            schema.pointer("/properties/data"),
            Some(&serde_json::json!({ "$id": ID, "type": "integer" }))
        );
    }
}
