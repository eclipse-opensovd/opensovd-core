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
use super::{encode_path, encode_path_segment};
use crate::schema::JsonSchema;

/// GET /components - Discover components.
///
/// Returns the list of components known to the SOVD server.
pub(crate) async fn component_list(
    State(topology): State<Topology>,
    parts: Parts,
    Query(query): Query<EntitiesQuery>,
) -> Result<Json<Entities>> {
    let base = super::super::versioned_uri(&parts);
    let items = topology
        .read()
        .await
        .components()
        .filter(|e| {
            let tags = e.tags();
            query.tags.is_empty() || query.tags.iter().any(|t| tags.contains(t))
        })
        .map(|e| {
            let tags = e.tags();
            EntityReference {
                id: e.id().to_string(),
                name: e.name().to_string(),
                translation_id: e.translation_id().map(String::from),
                href: format!("{base}/components/{}", encode_path_segment(e.id())).into(),
                tags: (!tags.is_empty()).then_some(tags.to_vec()),
            }
        })
        .collect();

    Ok(Json(Entities {
        items,
        schema: query.include_schema.then(Entities::schema),
    }))
}

/// GET /components/{component_id} - Query for capabilities of a component.
///
/// Returns the capabilities of a specific component.
pub(crate) async fn component_capabilities(
    State(topology): State<Topology>,
    entity: Entity,
    parts: Parts,
    Query(query): Query<EntityCapabilitiesQuery>,
) -> Result<Json<EntityCapabilities>> {
    let topo = topology.read().await;
    let component = entity.component(&topo)?;
    let component_id = component.id();

    let variant = super::variant(component.metadata());
    let translation_id = component.translation_id().map(String::from);

    let base = super::super::versioned_uri(&parts);
    let own = format!("{base}/{}", encode_path(entity.path()));
    // Relation links are only advertised when the relation is not empty.
    let hosts = topo
        .apps_of_component(component_id)
        .next()
        .is_some()
        .then(|| format!("{own}/hosts").into());

    // A direct reference, advertised only when the area exists.
    let belongs_to = topo
        .area_of_component(component_id)
        .ok()
        .flatten()
        .map(|area| format!("{base}/areas/{}", encode_path_segment(area.id())).into());

    let data = component
        .data_provider()
        .map(|_| format!("{own}/data").into());

    let bulk_data = component
        .bulkdata_provider()
        .map(|_| format!("{own}/bulk-data").into());

    let logs = component
        .log_provider()
        .map(|_| format!("{own}/logs").into());

    Ok(Json(EntityCapabilities {
        id: component_id.to_owned(),
        name: component.name().to_string(),
        translation_id,
        variant,
        hosts,
        belongs_to,
        data,
        bulk_data,
        logs,
        schema: query.include_schema.then(EntityCapabilities::schema),
        ..Default::default()
    }))
}

/// GET /components/{component_id}/hosts - Query for the apps on a component.
///
/// Returns the list of apps running on a specific component.
pub(crate) async fn component_hosts(
    State(topology): State<Topology>,
    entity: Entity,
    parts: Parts,
    Query(query): Query<EntitiesQuery>,
) -> Result<Json<Entities>> {
    // Verify component exists and get hosted apps atomically
    let topo = topology.read().await;
    let component_id = entity.component(&topo)?.id();

    let apps = topo.apps_of_component(component_id);

    let base = super::super::versioned_uri(&parts);
    let items: Vec<EntityReference> = apps
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
                href: format!("{base}/apps/{}", encode_path_segment(e.id())).into(),
                tags: (!tags.is_empty()).then_some(tags.to_vec()),
            }
        })
        .collect();

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
    async fn test_component_capabilities_ecu() {
        let app = crate::routes::test_router(create_mock_topology().await);

        let request = Request::builder()
            .uri("/sovd/v1/components/ecu")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert!(response.status().is_success());

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["id"], "ecu");
        assert_eq!(json["name"], "Engine Control Unit");
        assert_eq!(
            json["hosts"],
            "http://localhost/sovd/v1/components/ecu/hosts"
        );
        assert_eq!(
            json["belongs-to"],
            "http://localhost/sovd/v1/areas/powertrain"
        );
        assert_eq!(json["data"], "http://localhost/sovd/v1/components/ecu/data");
    }

    #[tokio::test]
    async fn test_component_capabilities_omit_empty_relations() {
        let topology = create_mock_topology().await;
        topology
            .write()
            .await
            .add_component(opensovd_core::Component::new("spare", "Spare ECU"));
        let app = crate::routes::test_router(topology);

        let request = Request::builder()
            .uri("/sovd/v1/components/spare")
            .body(Body::empty())
            .unwrap();

        let response = app.clone().oneshot(request).await.unwrap();
        assert!(response.status().is_success());

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["id"], "spare");
        assert!(json.get("hosts").is_none());
        assert!(json.get("belongs-to").is_none());

        // Not advertised, but still served as an empty list.
        let request = Request::builder()
            .uri("/sovd/v1/components/spare/hosts")
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), http::StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["items"], serde_json::json!([]));
    }

    #[tokio::test]
    async fn test_component_capabilities_skip_removed_area() {
        let topology = create_mock_topology().await;
        topology.write().await.remove_area("powertrain");
        let app = crate::routes::test_router(topology);

        let request = Request::builder()
            .uri("/sovd/v1/components/ecu")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(request).await.unwrap();
        assert!(response.status().is_success());
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json.get("belongs-to").is_none());
    }

    #[tokio::test]
    async fn test_component_list() {
        let app = crate::routes::test_router(create_mock_topology().await);

        let request = Request::builder()
            .uri("/sovd/v1/components")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert!(response.status().is_success());

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        let items = json["items"].as_array().expect("items should be an array");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0]["id"], "ecu");
        assert_eq!(items[0]["name"], "Engine Control Unit");
        assert_eq!(items[0]["href"], "http://localhost/sovd/v1/components/ecu");
    }
}
