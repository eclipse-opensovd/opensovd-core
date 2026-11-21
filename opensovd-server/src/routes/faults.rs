// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Fault resource endpoints.
//!
//! Provides routes below the path of an entity, `{entity_path}`, which is
//! empty for the server:
//! - GET /{entity_path}/faults - Query for faults
//! - GET /{entity_path}/faults/{fault_code} - Read a fault
//!
//! Returns faults matching the supplied filters and, for detail reads, the
//! fault with any available environment data.

use axum::{Router, extract::State, routing::get};
use opensovd_core::{Fault as CoreFault, FaultFilter, Topology};
use opensovd_models::faults::{Fault, FaultDetail, FaultDetailQuery, FaultList, FaultListQuery};
use serde::Deserialize;

use super::{
    entity::Entity,
    error::Result,
    extract::{Json, Path, Query},
};
use crate::schema::JsonSchema;

pub fn routes() -> Router<Topology> {
    Router::new()
        .route("/faults", get(fault_list))
        .route("/faults/{fault_code}", get(fault_read))
}

#[derive(Deserialize)]
struct FaultCode {
    fault_code: String,
}

/// Extracts status predicates from the query pairs without grouping repeated keys.
fn status_predicates(pairs: Vec<(String, String)>) -> Vec<(String, String)> {
    pairs
        .into_iter()
        .filter_map(|(key, value)| {
            let key = key.strip_prefix("status[")?.strip_suffix(']')?;
            Some((key.to_owned(), value))
        })
        .collect()
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

/// GET /{entity_path}/faults - List faults for an entity.
async fn fault_list(
    State(topology): State<Topology>,
    entity: Entity,
    Query(query): Query<FaultListQuery>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Result<Json<FaultList>> {
    let topo = topology.read().await;
    let provider = entity.fault_provider(&topo)?;
    let faults = provider
        .fault_list(FaultFilter {
            status: status_predicates(pairs),
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
async fn fault_read(
    State(topology): State<Topology>,
    entity: Entity,
    Path(FaultCode { fault_code }): Path<FaultCode>,
    Query(query): Query<FaultDetailQuery>,
) -> Result<Json<FaultDetail>> {
    let topo = topology.read().await;
    let provider = entity.fault_provider(&topo)?;
    let detail = provider.fault_read(&fault_code).await?;

    Ok(Json(FaultDetail {
        item: model_fault(detail.fault),
        environment_data: detail.environment_data,
        schema: query.include_schema.then(FaultDetail::schema),
    }))
}
