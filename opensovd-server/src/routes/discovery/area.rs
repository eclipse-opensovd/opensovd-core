// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

use axum::{extract::State, http::request::Parts};
use opensovd_core::Topology;
use opensovd_models::discovery::{
    Entities, EntitiesQuery, EntityCapabilities, EntityCapabilitiesQuery, EntityReference,
};

use super::super::entity::Entity;
use super::super::error::Result;
use super::super::extract::{Json, Query};
use super::encode_path_segment;
use crate::schema::JsonSchema;

/// GET /areas - Discover areas.
///
/// Returns the list of areas known to the SOVD server.
pub(crate) async fn area_list(
    State(topology): State<Topology>,
    parts: Parts,
    Query(query): Query<EntitiesQuery>,
) -> Result<Json<Entities>> {
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

    Ok(Json(Entities {
        items,
        schema: query.include_schema.then(Entities::schema),
    }))
}

/// GET /areas/{area_id} - Query for capabilities of an area.
///
/// Returns the capabilities of a specific area.
pub(crate) async fn area_capabilities(
    State(topology): State<Topology>,
    entity: Entity,
    parts: Parts,
    Query(query): Query<EntityCapabilitiesQuery>,
) -> Result<Json<EntityCapabilities>> {
    let topo = topology.read().await;
    let area = entity.area(&topo)?;
    let area_id = area.id();

    let variant = super::variant(area.metadata());
    let translation_id = area.translation_id().map(String::from);

    let base = super::super::versioned_uri(&parts);
    // Relation links are only advertised when the relation is not empty.
    let has_members = topo.components_of_area(area_id).next().is_some()
        || topo.apps_of_area(area_id).next().is_some();
    let contains = has_members.then(|| format!("{base}/{}/contains", entity.path()).into());

    Ok(Json(EntityCapabilities {
        id: area_id.to_owned(),
        name: area.name().to_string(),
        translation_id,
        variant,
        contains,
        schema: query.include_schema.then(EntityCapabilities::schema),
        ..Default::default()
    }))
}

/// GET /areas/{area_id}/contains - Query for the entities in an area.
///
/// Returns the list of components and apps contained in a specific area.
pub(crate) async fn area_contains(
    State(topology): State<Topology>,
    entity: Entity,
    parts: Parts,
    Query(query): Query<EntitiesQuery>,
) -> Result<Json<Entities>> {
    // Verify area exists and get contained entities atomically
    let topo = topology.read().await;
    let area_id = entity.area(&topo)?.id();

    let base = super::super::versioned_uri(&parts);
    let mut items = Vec::new();

    // Add contained components
    let components = topo.components_of_area(area_id);
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
    let apps = topo.apps_of_area(area_id);
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

    Ok(Json(Entities {
        items,
        schema: query.include_schema.then(Entities::schema),
    }))
}

#[cfg(test)]
mod tests {
    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use opensovd_mocks::create_mock_topology;
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_area_capabilities_powertrain() {
        let app = crate::routes::test_router(create_mock_topology().await);

        let request = Request::builder()
            .uri("/sovd/v1/areas/powertrain")
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
    async fn test_area_capabilities_omit_empty_contains() {
        let topology = create_mock_topology().await;
        topology
            .write()
            .await
            .add_area(opensovd_core::Area::new("body", "Body Domain"));
        let app = crate::routes::test_router(topology);

        let request = Request::builder()
            .uri("/sovd/v1/areas/body")
            .body(Body::empty())
            .unwrap();

        let response = app.clone().oneshot(request).await.unwrap();
        assert!(response.status().is_success());

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["id"], "body");
        assert!(json.get("contains").is_none());

        // Not advertised, but still served as an empty list.
        let request = Request::builder()
            .uri("/sovd/v1/areas/body/contains")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), http::StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["items"], serde_json::json!([]));
    }

    #[tokio::test]
    async fn test_area_list() {
        let app = crate::routes::test_router(create_mock_topology().await);

        let request = Request::builder()
            .uri("/sovd/v1/areas")
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
