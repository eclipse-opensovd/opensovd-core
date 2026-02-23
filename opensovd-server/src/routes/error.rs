// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Error response handling.
//!
//! Defines error types that convert to SOVD-compliant HTTP error responses.

use axum::{
    extract::rejection::QueryRejection,
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};
use opensovd_core::{DataError, TopologyError};
use opensovd_models::{ErrorCode, GenericError};

/// A `Result` alias where the `Err` variant is [`Error`].
pub type Result<T> = std::result::Result<T, Error>;

/// Handler error that converts to HTTP responses.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("entity not found: {0}")]
    EntityNotFound(String),
    #[error("provider not available: {0}")]
    ProviderNotAvailable(String),
    #[error(transparent)]
    Data(#[from] DataError),
    #[error("{0}")]
    BadQuery(#[from] QueryRejection),
    #[error(transparent)]
    Topology(#[from] TopologyError),
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let (status, error) = match &self {
            Self::EntityNotFound(id) => (
                StatusCode::NOT_FOUND,
                GenericError::with_vendor_code(
                    "entity-not-found",
                    format!("Entity not found: {id}"),
                ),
            ),
            Self::ProviderNotAvailable(provider) => (
                StatusCode::NOT_FOUND,
                GenericError::with_vendor_code(
                    "provider-not-available",
                    format!("Component has no {provider}"),
                ),
            ),
            Self::Data(e) => {
                let status = match e {
                    DataError::NotFound(_) => StatusCode::NOT_FOUND,
                    DataError::ReadOnly => StatusCode::BAD_REQUEST,
                    DataError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
                };

                // Sanitize internal errors - log details, return generic message
                let message = match e {
                    DataError::Internal(msg) => {
                        tracing::error!(target: "srv", error = %msg, "Internal error");
                        "An internal error occurred".to_string()
                    }
                    _ => e.to_string(),
                };

                (status, GenericError::new(ErrorCode::ErrorResponse, message))
            }
            Self::BadQuery(_) => (
                StatusCode::BAD_REQUEST,
                GenericError::new(ErrorCode::IncompleteRequest, "Bad request"),
            ),
            Self::Topology(e) => {
                let status = match e {
                    TopologyError::NotFound(_) => StatusCode::NOT_FOUND,
                };
                tracing::error!(target: "srv", error = %e, "Topology error");
                let message = e.to_string();
                (status, GenericError::new(ErrorCode::ErrorResponse, message))
            }
        };
        (status, Json(error)).into_response()
    }
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use http_body_util::BodyExt;
    use opensovd_core::EntityRef;

    use super::*;

    #[tokio::test]
    async fn test_error_entity_not_found() {
        let error = Error::EntityNotFound("test-component".into());
        let response = error.into_response();

        assert_eq!(response.status(), 404);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["vendor_code"], "entity-not-found");
        assert!(json["message"].as_str().unwrap().contains("test-component"));
    }

    #[tokio::test]
    async fn test_error_provider_not_available() {
        let error = Error::ProviderNotAvailable("data".into());
        let response = error.into_response();

        assert_eq!(response.status(), 404);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["vendor_code"], "provider-not-available");
    }

    #[tokio::test]
    async fn test_error_data_not_found() {
        let error = Error::Data(DataError::NotFound("voltage".into()));
        let response = error.into_response();

        assert_eq!(response.status(), 404);
    }

    #[tokio::test]
    async fn test_error_data_read_only() {
        let error = Error::Data(DataError::ReadOnly);
        let response = error.into_response();

        assert_eq!(response.status(), 400);
    }

    #[tokio::test]
    async fn test_error_data_internal() {
        let error = Error::Data(DataError::Internal("lock poisoned".into()));
        let response = error.into_response();

        assert_eq!(response.status(), 500);

        // Verify internal details are sanitized - client receives generic message
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["message"], "An internal error occurred");
        // Ensure the actual error details are NOT leaked
        assert!(!json["message"].as_str().unwrap().contains("lock poisoned"));
    }

    #[tokio::test]
    async fn test_error_from_data_error() {
        let data_error = DataError::NotFound("voltage".into());
        let error: Error = data_error.into();
        let response = error.into_response();

        assert_eq!(response.status(), 404);
    }

    #[tokio::test]
    async fn test_error_topology_not_found() {
        let error = Error::Topology(TopologyError::NotFound(EntityRef::area("area-1")));
        let response = error.into_response();

        assert_eq!(response.status(), 404);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["message"].as_str().unwrap().contains("area-1"));
    }

    #[tokio::test]
    async fn test_error_bad_query() {
        use axum::{Router, body::Body, extract::Query, http::Request, routing::get};
        use axum_extra::extract::WithRejection;
        use serde::Deserialize;
        use tower::ServiceExt;

        #[derive(Deserialize)]
        struct TestQuery {
            #[allow(dead_code)]
            flag: bool,
        }

        async fn handler(
            WithRejection(Query(_q), _): WithRejection<Query<TestQuery>, Error>,
        ) -> &'static str {
            "ok"
        }

        let app = Router::new().route("/test", get(handler));

        let request = Request::builder()
            .uri("/test?flag=not_a_bool")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), 400);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error_code"], "incomplete-request");
        assert_eq!(json["message"], "Bad request");
    }
}
