// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Fault resource endpoints.
//!
//! These routes are served for components and apps. The server, areas, and
//! functions answer 404.
//!
//! - GET /{entity_path}/faults - List faults.
//! - GET /{entity_path}/faults/{fault_code} - Read a fault.
//! - DELETE /{entity_path}/faults - Answers 501 until fault deletion is implemented.
//! - DELETE /{entity_path}/faults/{fault_code} - Answers 501 until fault deletion is implemented.

use axum::{Router, extract::State, http::StatusCode, routing::get};
use opensovd_core::{Fault as CoreFault, FaultDataError, FaultError, FaultFilter, Topology};
use opensovd_models::faults::{Fault, FaultDetail, FaultDetailQuery, FaultList, FaultListQuery};
use opensovd_models::{DataError as ModelDataError, ErrorCode, GenericError};
use serde::Deserialize;
use serde_json::Value;

use super::{
    entity::Entity,
    error::Result,
    extract::{Json, Path, Query},
};
use crate::schema::{JsonSchema, normalize};

pub fn routes() -> Router<Topology> {
    Router::new()
        .route("/faults", get(fault_list).delete(fault_delete_all))
        .route("/faults/{fault_code}", get(fault_read).delete(fault_delete))
}

#[derive(Deserialize)]
struct FaultCode {
    fault_code: String,
}

/// The `status[key]=value` predicates of a query, in request order and
/// without grouping repeated keys. A key that starts with `status[` and is
/// not `status[<key>]` is rejected.
#[derive(Deserialize)]
#[serde(try_from = "Vec<(String, String)>")]
struct StatusPredicates(Vec<(String, String)>);

impl TryFrom<Vec<(String, String)>> for StatusPredicates {
    type Error = String;

    fn try_from(pairs: Vec<(String, String)>) -> std::result::Result<Self, Self::Error> {
        let mut predicates = Vec::new();
        for (param, value) in pairs {
            let Some(key) = param.strip_prefix("status[") else {
                continue;
            };
            match key.strip_suffix(']') {
                Some(key) if !key.is_empty() && !key.contains(['[', ']']) => {
                    predicates.push((key.to_owned(), value));
                }
                _ => return Err(format!("malformed status filter: {param}")),
            }
        }
        Ok(Self(predicates))
    }
}

fn model_fault(fault: CoreFault) -> Fault {
    Fault {
        code: fault.code,
        scope: fault.scope,
        display_code: fault.display_code,
        fault_name: fault.fault_name,
        fault_translation_id: fault.fault_translation_id,
        severity: fault.severity,
        status: fault.status,
        symptom: fault.symptom,
        symptom_translation_id: fault.symptom_translation_id,
        tags: (!fault.tags.is_empty()).then_some(fault.tags),
    }
}

fn model_error(error: FaultDataError) -> ModelDataError {
    ModelDataError {
        path: format!("/environment_data{}", error.path).into(),
        error: Some(GenericError::new(ErrorCode::ErrorResponse, error.message)),
    }
}

/// Schema of a detail response, with the environment data schema in place of `environment_data`.
fn read_response_schema(environment_data: Value) -> Value {
    let mut schema = FaultDetail::schema();
    // Without `jsonschema`, the response schema has no properties.
    if let Some(properties) = schema.as_object_mut().and_then(|object| {
        object
            .entry("properties")
            .or_insert_with(|| serde_json::json!({}))
            .as_object_mut()
    }) {
        properties.insert("environment_data".into(), normalize(environment_data));
    }
    schema
}

/// GET /{entity_path}/faults - List faults for an entity.
///
/// Returns the entity's faults matching the supplied filters.
async fn fault_list(
    State(topology): State<Topology>,
    entity: Entity,
    Query(query): Query<FaultListQuery>,
    Query(StatusPredicates(status)): Query<StatusPredicates>,
) -> Result<Json<FaultList>> {
    let provider = entity.fault_provider(&topology.read().await)?;
    let faults = provider
        .fault_list(FaultFilter {
            status,
            severity: query.severity,
            scope: query.scope,
            tags: query.tags.unwrap_or_default(),
        })
        .await?
        .into_iter()
        .map(model_fault)
        .collect();

    Ok(Json(FaultList {
        items: faults,
        schema: query.include_schema.then(FaultList::schema),
    }))
}

/// GET /{entity_path}/faults/{fault_code} - Read one fault and its environment data.
///
/// Returns the fault and any available environment data, with a schema when requested.
async fn fault_read(
    State(topology): State<Topology>,
    entity: Entity,
    Path(FaultCode { fault_code }): Path<FaultCode>,
    Query(query): Query<FaultDetailQuery>,
) -> Result<Json<FaultDetail>> {
    let provider = entity.fault_provider(&topology.read().await)?;
    let detail = provider
        .fault_read(&fault_code, query.include_schema)
        .await?;
    let schema = query
        .include_schema
        .then(|| read_response_schema(detail.schema.unwrap_or(Value::Bool(true))));

    Ok(Json(FaultDetail {
        item: model_fault(detail.fault),
        environment_data: detail.environment_data,
        errors: detail.errors.into_iter().map(model_error).collect(),
        schema,
    }))
}

/// DELETE /{entity_path}/faults - Delete all faults for an entity.
///
/// Returns 501 for an entity with a fault provider because deletion is not implemented.
async fn fault_delete_all(State(topology): State<Topology>, entity: Entity) -> Result<StatusCode> {
    entity.fault_provider(&topology.read().await)?;
    Err(FaultError::NotSupported("deletion".into()).into())
}

/// DELETE /{entity_path}/faults/{fault_code} - Delete one fault.
///
/// Returns 404 for an unknown fault and 501 for a known fault because deletion is not implemented.
async fn fault_delete(
    State(topology): State<Topology>,
    entity: Entity,
    Path(FaultCode { fault_code }): Path<FaultCode>,
) -> Result<StatusCode> {
    let provider = entity.fault_provider(&topology.read().await)?;
    provider.fault_read(&fault_code, false).await?;
    Err(FaultError::NotSupported("deletion".into()).into())
}

#[cfg(all(test, feature = "jsonschema"))]
mod tests {
    use super::*;

    #[test]
    fn read_response_schema_puts_environment_data_schema_in_place_of_value() {
        let schema = read_response_schema(serde_json::json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "title": "Temperature",
            "type": "integer",
        }));
        assert_eq!(
            schema.pointer("/properties/environment_data"),
            Some(&serde_json::json!({ "type": "integer" }))
        );
        assert!(schema.pointer("/properties/item").is_some());
    }
}
