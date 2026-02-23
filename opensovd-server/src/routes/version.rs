// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Version information endpoint.
//!
//! Provides routes for:
//! - GET /version-info - Get SOVD server version and vendor information

use axum::{
    Router,
    extract::{Query, State},
    http::request::Parts,
    response::Json,
    routing::get,
};
use axum_extra::extract::WithRejection;
use opensovd_models::Response;
use opensovd_models::version::{SovdInfo, VersionInfo, VersionInfoQuery};
use serde::Serialize;

use super::AppState;
use super::error::Error;
use crate::schema::JsonSchema;

pub fn routes<V>() -> Router<AppState<V>>
where
    V: Serialize + Clone + Send + Sync + 'static,
    VersionInfo<V>: JsonSchema,
{
    Router::new().route("/version-info", get(version_info::<V>))
}

/// GET /version-info - Get SOVD server version information.
///
/// Returns the SOVD API version, base URI, and optional vendor-specific information.
async fn version_info<V>(
    State(state): State<AppState<V>>,
    parts: Parts,
    WithRejection(Query(query), _): WithRejection<Query<VersionInfoQuery>, Error>,
) -> Json<Response<VersionInfo<V>>>
where
    V: Serialize + Clone + Send + Sync + 'static,
    VersionInfo<V>: JsonSchema,
{
    let base_uri = super::versioned_uri(&parts);

    Json(Response {
        data: VersionInfo {
            sovd_info: vec![SovdInfo {
                version: super::SOVD_VERSION.into(),
                base_uri: base_uri.into(),
                vendor_info: state.vendor_info,
            }],
        },
        schema: query.include_schema.then(VersionInfo::<V>::schema),
    })
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
#[allow(clippy::string_slice)]
mod tests {
    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use opensovd_core::Topology;
    use opensovd_models::version::VendorInfo;
    use tower::ServiceExt;

    use super::*;

    #[tokio::test]
    async fn test_version_info() {
        let state = AppState::<VendorInfo> {
            vendor_info: None,
            topology: Topology::default(),
        };
        let app = routes::<VendorInfo>().with_state(state);

        let request = Request::builder()
            .uri("/version-info")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert!(response.status().is_success());

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["sovd_info"][0]["version"], crate::routes::SOVD_VERSION);
        assert!(json["sovd_info"][0]["vendor_info"].is_null());
        assert!(json.get("schema").is_none());
    }

    #[tokio::test]
    async fn test_version_info_with_schema() {
        let state = AppState::<VendorInfo> {
            vendor_info: None,
            topology: Topology::default(),
        };
        let app = routes::<VendorInfo>().with_state(state);

        let request = Request::builder()
            .uri("/version-info?include-schema=true")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert!(response.status().is_success());

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert!(json["schema"].is_object());
    }

    #[tokio::test]
    async fn test_version_info_invalid_query_param() {
        let state = AppState::<VendorInfo> {
            vendor_info: None,
            topology: Topology::default(),
        };
        let app = routes::<VendorInfo>().with_state(state);

        let request = Request::builder()
            .uri("/version-info?include-schema=bad")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), 400);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["error_code"], "incomplete-request");
        assert_eq!(json["message"], "Bad request");
    }

    #[cfg(feature = "jsonschema")]
    #[tokio::test]
    async fn test_version_info_with_custom_vendor_schema() {
        #[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
        struct CustomVendor {
            product: String,
            build: u32,
        }

        let state = AppState {
            vendor_info: Some(CustomVendor {
                product: "SOVD".into(),
                build: 42,
            }),
            topology: Topology::default(),
        };
        let app = routes::<CustomVendor>().with_state(state);

        let request = Request::builder()
            .uri("/version-info?include-schema=true")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert!(response.status().is_success());

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        // Verify data
        assert_eq!(json["sovd_info"][0]["vendor_info"]["product"], "SOVD");
        assert_eq!(json["sovd_info"][0]["vendor_info"]["build"], 42);

        // Verify schema structure (schemars 1.0 uses JSON Schema 2020-12)
        let schema = &json["schema"];
        assert_eq!(
            schema["$schema"],
            "https://json-schema.org/draft/2020-12/schema"
        );
        assert_eq!(schema["title"], "VersionInfo");

        // Verify CustomVendor is in $defs with correct properties (schemars 1.0 uses $defs)
        let vendor_def = &schema["$defs"]["CustomVendor"];
        assert_eq!(vendor_def["type"], "object");
        assert_eq!(vendor_def["properties"]["product"]["type"], "string");
        assert_eq!(vendor_def["properties"]["build"]["type"], "integer");
        assert_eq!(vendor_def["properties"]["build"]["format"], "uint32");
    }
}
