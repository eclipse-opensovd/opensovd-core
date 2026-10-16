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

/// GET /apps - Discover apps.
///
/// Returns the list of apps known to the SOVD server.
pub(crate) async fn app_list(
    State(topology): State<Topology>,
    parts: Parts,
    Query(query): Query<EntitiesQuery>,
) -> Result<Json<Entities>> {
    let base = super::super::versioned_uri(&parts);
    let items = topology
        .read()
        .await
        .apps()
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

/// GET /apps/{app_id} - Query for capabilities of an app.
///
/// Returns the capabilities of a specific app.
pub(crate) async fn app_capabilities(
    State(topology): State<Topology>,
    entity: Entity,
    parts: Parts,
    Query(query): Query<EntityCapabilitiesQuery>,
) -> Result<Json<EntityCapabilities>> {
    let topo = topology.read().await;
    let app = entity.app(&topo)?;
    let app_id = app.id();

    let variant = super::variant(app.metadata());
    let translation_id = app.translation_id().map(String::from);

    let base = super::super::versioned_uri(&parts);
    let own = format!("{base}/{}", entity.path());
    // Direct references, advertised only when the target exists.
    let located_on = topo
        .component_of_app(app_id)
        .ok()
        .flatten()
        .map(|c| format!("{base}/components/{}", encode_path_segment(c.id())).into());

    let belongs_to = topo
        .area_of_app(app_id)
        .ok()
        .flatten()
        .map(|area| format!("{base}/areas/{}", encode_path_segment(area.id())).into());

    let data = app.data_provider().map(|_| format!("{own}/data").into());

    let bulk_data = app
        .bulkdata_provider()
        .map(|_| format!("{own}/bulk-data").into());

    let logs = app.log_provider().map(|_| format!("{own}/logs").into());

    Ok(Json(EntityCapabilities {
        id: app_id.to_owned(),
        name: app.name().to_string(),
        translation_id,
        variant,
        located_on,
        belongs_to,
        data,
        bulk_data,
        logs,
        schema: query.include_schema.then(EntityCapabilities::schema),
        ..Default::default()
    }))
}

#[cfg(test)]
mod tests {
    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use opensovd_mocks::create_mock_topology;
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_app_capabilities_engine_control() {
        let app = crate::routes::test_router(create_mock_topology().await);

        let request = Request::builder()
            .uri("/sovd/v1/apps/engine_control")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert!(response.status().is_success());

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["id"], "engine_control");
        assert_eq!(json["name"], "Engine Control Application");
        assert_eq!(
            json["is-located-on"],
            "http://localhost/sovd/v1/components/ecu"
        );
        assert_eq!(
            json["belongs-to"],
            "http://localhost/sovd/v1/areas/powertrain"
        );
        assert_eq!(
            json["data"],
            "http://localhost/sovd/v1/apps/engine_control/data"
        );
    }

    #[tokio::test]
    async fn test_app_without_area_omits_belongs_to() {
        let app = crate::routes::test_router(create_mock_topology().await);

        let request = Request::builder()
            .uri("/sovd/v1/apps/ota_manager")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(request).await.unwrap();
        assert!(response.status().is_success());
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json.get("belongs-to").is_none());
    }

    #[tokio::test]
    async fn test_app_capabilities_skip_removed_relations() {
        let topology = create_mock_topology().await;
        {
            let mut topo = topology.write().await;
            topo.remove_component("ecu");
            topo.remove_area("powertrain");
        }
        let app = crate::routes::test_router(topology);

        let request = Request::builder()
            .uri("/sovd/v1/apps/engine_control")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(request).await.unwrap();
        assert!(response.status().is_success());
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json.get("is-located-on").is_none());
        assert!(json.get("belongs-to").is_none());
    }

    #[tokio::test]
    async fn test_app_list() {
        let app = crate::routes::test_router(create_mock_topology().await);

        let request = Request::builder()
            .uri("/sovd/v1/apps")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert!(response.status().is_success());

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        let items = json["items"].as_array().expect("items should be an array");
        assert_eq!(items.len(), 3);
        assert_eq!(items[0]["id"], "engine_control");
        assert_eq!(items[0]["name"], "Engine Control Application");
        assert_eq!(
            items[0]["href"],
            "http://localhost/sovd/v1/apps/engine_control"
        );
    }
}
