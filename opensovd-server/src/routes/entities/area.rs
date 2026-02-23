// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

use axum::{
    Router,
    extract::{Path, Query, State},
    http::request::Parts,
    response::Json,
    routing::get,
};
use axum_extra::extract::WithRejection;
use opensovd_core::Topology;
use opensovd_models::Response;
use opensovd_models::discovery::{
    Entities, EntitiesQuery, EntityCapabilities, EntityCapabilitiesQuery, EntityReference,
};

use super::super::AppState;
use super::super::error::{Error, Result};
use super::encode_path_segment;
use crate::schema::JsonSchema;

pub(super) fn routes<V>() -> Router<AppState<V>>
where
    V: Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/areas", get(area_list))
        .route("/areas/{area_id}", get(area_capabilities))
        .route("/areas/{area_id}/contains", get(area_contains))
}

/// GET /areas - Discover areas.
///
/// Returns the list of areas known to the SOVD server.
pub(super) async fn area_list(
    State(topology): State<Topology>,
    parts: Parts,
    axum_extra::extract::Query(query): axum_extra::extract::Query<EntitiesQuery>,
) -> Result<Json<Response<Entities>>> {
    let base = super::super::versioned_uri(&parts);
    let items = topology
        .read()
        .await
        .areas()
        .filter(|e| {
            let tags = e.tags();
            query.tags.is_empty() || query.tags.iter().any(|t| tags.contains(t))
        })
        .map(|e| {
            let tags = e.tags();
            let translation_id = e.translation_id().map(String::from);
            EntityReference {
                id: e.id().to_string(),
                name: e.name().to_string(),
                translation_id,
                href: format!("{base}/areas/{}", encode_path_segment(e.id())).into(),
                tags: (!tags.is_empty()).then_some(tags.to_vec()),
            }
        })
        .collect();

    Ok(Json(Response {
        data: Entities { items },
        schema: query.include_schema.then(Entities::schema),
    }))
}

/// GET `/areas/:area_id` - Query capabilities of an area.
///
/// Returns the capabilities of a specific area.
pub(super) async fn area_capabilities(
    State(topology): State<Topology>,
    Path(area_id): Path<String>,
    parts: Parts,
    WithRejection(Query(query), _): WithRejection<Query<EntityCapabilitiesQuery>, Error>,
) -> Result<Json<Response<EntityCapabilities>>> {
    let topo = topology.read().await;
    let entity = topo
        .get_area(&area_id)
        .map_err(|_| Error::EntityNotFound(area_id.clone()))?;

    let variant = (!entity.metadata().is_empty()).then(|| entity.metadata().clone());
    let translation_id = entity.translation_id().map(String::from);

    let base = super::super::versioned_uri(&parts);
    let contains = Some(format!("{base}/areas/{}/contains", encode_path_segment(&area_id)).into());

    Ok(Json(Response {
        data: EntityCapabilities {
            id: area_id,
            name: entity.name().to_string(),
            translation_id,
            variant,
            contains,
            ..Default::default()
        },
        schema: query.include_schema.then(EntityCapabilities::schema),
    }))
}

/// GET `/areas/:area_id/contains` - List entities contained in an area.
///
/// Returns the list of components and apps contained in a specific area.
pub(super) async fn area_contains(
    State(topology): State<Topology>,
    Path(area_id): Path<String>,
    parts: Parts,
    axum_extra::extract::Query(query): axum_extra::extract::Query<EntitiesQuery>,
) -> Result<Json<Response<Entities>>> {
    // Verify area exists and get contained entities atomically
    let topo = topology.read().await;
    topo.get_area(&area_id)
        .map_err(|_| Error::EntityNotFound(area_id.clone()))?;

    let base = super::super::versioned_uri(&parts);
    let mut items = Vec::new();

    // Add contained components
    let components = topo.components_of_area(&area_id);
    items.extend(
        components
            .filter(|e| {
                let tags = e.tags();
                query.tags.is_empty() || query.tags.iter().any(|t| tags.contains(t))
            })
            .map(|e| {
                let tags = e.tags();
                let translation_id = e.translation_id().map(String::from);
                EntityReference {
                    id: e.id().to_string(),
                    name: e.name().to_string(),
                    translation_id,
                    href: format!("{base}/components/{}", encode_path_segment(e.id())).into(),
                    tags: (!tags.is_empty()).then_some(tags.to_vec()),
                }
            }),
    );

    // Add contained apps
    let apps = topo.apps_of_area(&area_id);
    items.extend(
        apps.filter(|e| {
            let tags = e.tags();
            query.tags.is_empty() || query.tags.iter().any(|t| tags.contains(t))
        })
        .map(|e| {
            let tags = e.tags();
            let translation_id = e.translation_id().map(String::from);
            EntityReference {
                id: e.id().to_string(),
                name: e.name().to_string(),
                translation_id,
                href: format!("{base}/apps/{}", encode_path_segment(e.id())).into(),
                tags: (!tags.is_empty()).then_some(tags.to_vec()),
            }
        }),
    );

    Ok(Json(Response {
        data: Entities { items },
        schema: query.include_schema.then(Entities::schema),
    }))
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use opensovd_mocks::create_mock_topology;
    use tower::ServiceExt;

    use super::*;

    #[tokio::test]
    async fn test_area_capabilities_powertrain() {
        let state = AppState::<()> {
            vendor_info: None,
            topology: create_mock_topology().await,
        };
        let app = routes::<()>().with_state(state);

        let request = Request::builder()
            .uri("/areas/powertrain")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert!(response.status().is_success());

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["id"], "powertrain");
        assert_eq!(json["name"], "Powertrain Domain");
        assert_eq!(
            json["contains"],
            "http://localhost/sovd/v1/areas/powertrain/contains"
        );
    }

    #[tokio::test]
    async fn test_area_list() {
        let state = AppState::<()> {
            vendor_info: None,
            topology: create_mock_topology().await,
        };
        let app = routes::<()>().with_state(state);

        let request = Request::builder()
            .uri("/areas")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert!(response.status().is_success());

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        let items = json["items"].as_array().expect("items should be an array");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0]["id"], "powertrain");
        assert_eq!(items[0]["name"], "Powertrain Domain");
        assert_eq!(
            items[0]["href"],
            "http://localhost/sovd/v1/areas/powertrain"
        );
    }
}
