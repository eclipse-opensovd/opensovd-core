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
        .route("/components", get(component_list))
        .route("/components/{component_id}", get(component_capabilities))
        .route("/components/{component_id}/hosts", get(component_hosts))
        .route(
            "/components/{component_id}/belongs-to",
            get(component_belongs_to),
        )
}

/// GET /components - Discover contained components.
///
/// Returns the list of components known to the SOVD server.
pub(super) async fn component_list(
    State(topology): State<Topology>,
    parts: Parts,
    axum_extra::extract::Query(query): axum_extra::extract::Query<EntitiesQuery>,
) -> Result<Json<Response<Entities>>> {
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

    Ok(Json(Response {
        data: Entities { items },
        schema: query.include_schema.then(Entities::schema),
    }))
}

/// GET `/components/:component_id` - Query capabilities of a component.
///
/// Returns the capabilities of a specific component.
pub(super) async fn component_capabilities(
    State(topology): State<Topology>,
    Path(component_id): Path<String>,
    parts: Parts,
    WithRejection(Query(query), _): WithRejection<Query<EntityCapabilitiesQuery>, Error>,
) -> Result<Json<Response<EntityCapabilities>>> {
    let topo = topology.read().await;
    let entity = topo
        .get_component(&component_id)
        .map_err(|_| Error::EntityNotFound(component_id.clone()))?;

    let variant = (!entity.metadata().is_empty()).then(|| entity.metadata().clone());
    let translation_id = entity.translation_id().map(String::from);

    let base = super::super::versioned_uri(&parts);
    let hosts = Some(
        format!(
            "{base}/components/{}/hosts",
            encode_path_segment(&component_id)
        )
        .into(),
    );

    let belongs_to = entity.area_id().map(|_| {
        format!(
            "{base}/components/{}/belongs-to",
            encode_path_segment(&component_id)
        )
        .into()
    });

    let data = entity.data_provider().map(|_| {
        format!(
            "{base}/components/{}/data",
            encode_path_segment(&component_id)
        )
        .into()
    });

    Ok(Json(Response {
        data: EntityCapabilities {
            id: component_id,
            name: entity.name().to_string(),
            translation_id,
            variant,
            hosts,
            belongs_to,
            data,
            ..Default::default()
        },
        schema: query.include_schema.then(EntityCapabilities::schema),
    }))
}

/// GET `/components/:component_id/hosts` - List apps hosted on a component.
///
/// Returns the list of apps running on a specific component.
pub(super) async fn component_hosts(
    State(topology): State<Topology>,
    Path(component_id): Path<String>,
    parts: Parts,
    axum_extra::extract::Query(query): axum_extra::extract::Query<EntitiesQuery>,
) -> Result<Json<Response<Entities>>> {
    // Verify component exists and get hosted apps atomically
    let topo = topology.read().await;
    topo.get_component(&component_id)
        .map_err(|_| Error::EntityNotFound(component_id.clone()))?;

    let apps = topo.apps_of_component(&component_id);

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

    Ok(Json(Response {
        data: Entities { items },
        schema: query.include_schema.then(Entities::schema),
    }))
}

/// GET `/components/:component_id/belongs-to` - Get area containing the component.
///
/// Returns the area that contains a specific component.
pub(super) async fn component_belongs_to(
    State(topology): State<Topology>,
    Path(component_id): Path<String>,
    parts: Parts,
    axum_extra::extract::Query(query): axum_extra::extract::Query<EntitiesQuery>,
) -> Result<Json<Response<Entities>>> {
    let topo = topology.read().await;
    let mut items = Vec::new();

    if let Some(area) = topo
        .area_of_component(&component_id)
        .map_err(|_| Error::EntityNotFound(component_id.clone()))?
    {
        let tags = area.tags();
        let translation_id = area.translation_id().map(String::from);

        // Check tag filter if provided
        let matches_tags = query.tags.is_empty()
            || (!tags.is_empty() && query.tags.iter().any(|t| tags.contains(t)));

        if matches_tags {
            let base = super::super::versioned_uri(&parts);
            items.push(EntityReference {
                id: area.id().to_string(),
                name: area.name().to_string(),
                translation_id,
                href: format!("{base}/areas/{}", encode_path_segment(area.id())).into(),
                tags: (!tags.is_empty()).then_some(tags.to_vec()),
            });
        }
    }

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
    async fn test_component_capabilities_ecu() {
        let state = AppState::<()> {
            vendor_info: None,
            topology: create_mock_topology().await,
        };
        let app = routes::<()>().with_state(state);

        let request = Request::builder()
            .uri("/components/ecu")
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
            "http://localhost/sovd/v1/components/ecu/belongs-to"
        );
        assert_eq!(json["data"], "http://localhost/sovd/v1/components/ecu/data");
    }

    #[tokio::test]
    async fn test_component_list() {
        let state = AppState::<()> {
            vendor_info: None,
            topology: create_mock_topology().await,
        };
        let app = routes::<()>().with_state(state);

        let request = Request::builder()
            .uri("/components")
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
