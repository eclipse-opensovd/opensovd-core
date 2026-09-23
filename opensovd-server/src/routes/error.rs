// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Error response handling.
//!
//! Defines error types that convert to SOVD-compliant HTTP error responses.

use axum::{
    extract::rejection::JsonRejection,
    http::{StatusCode, header},
    response::{IntoResponse, Json, Response},
};
use axum_extra::{extract::QueryRejection, typed_header::TypedHeaderRejection};
use opensovd_core::{BulkDataError, DataError, TopologyError};
use opensovd_models::{ErrorCode, ErrorDetails, GenericError, JsonPointer};

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
    #[error(transparent)]
    BulkData(#[from] BulkDataError),
    #[error("{0}")]
    BadQuery(#[from] QueryRejection),
    #[error("{0}")]
    InvalidHeader(#[from] TypedHeaderRejection),
    #[error("{0}")]
    BadBody(#[from] JsonRejection),
    #[error(transparent)]
    Topology(#[from] TopologyError),
}

/// A `DataError` naming the erroneous element of the request body.
fn data_error(path: impl Into<String>, message: impl Into<String>) -> ErrorDetails {
    opensovd_models::DataError {
        path: path.into().into(),
        error: Some(GenericError::new(ErrorCode::IncompleteRequest, message)),
    }
    .into()
}

/// The JSON pointer to the element of the request body that failed to
/// deserialize, or the whole body if it is unknown.
fn rejection_path(rejection: &JsonRejection) -> String {
    let mut source = std::error::Error::source(rejection);
    while let Some(error) = source {
        if let Some(error) = error.downcast_ref::<serde_path_to_error::Error<serde_json::Error>>() {
            return JsonPointer::from(error).0;
        }
        source = error.source();
    }
    String::new()
}

/// A malformed body names the erroneous element; other rejections keep
/// their status.
fn body_rejection(rejection: &JsonRejection) -> (StatusCode, ErrorDetails) {
    let message = rejection.body_text();
    match rejection {
        JsonRejection::JsonDataError(_) => (
            StatusCode::BAD_REQUEST,
            data_error(rejection_path(rejection), message),
        ),
        JsonRejection::JsonSyntaxError(_) => (StatusCode::BAD_REQUEST, data_error("", message)),
        _ => (
            rejection.status(),
            GenericError::new(ErrorCode::IncompleteRequest, message).into(),
        ),
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let (status, details): (_, ErrorDetails) = match &self {
            Self::EntityNotFound(id) => (
                StatusCode::NOT_FOUND,
                GenericError::with_vendor_code(
                    "entity-not-found",
                    format!("Entity not found: {id}"),
                )
                .into(),
            ),
            Self::ProviderNotAvailable(provider) => (
                StatusCode::NOT_FOUND,
                GenericError::with_vendor_code(
                    "provider-not-available",
                    format!("Component has no {provider}"),
                )
                .into(),
            ),
            Self::Data(e) => {
                let status = match e {
                    DataError::NotFound(_) => StatusCode::NOT_FOUND,
                    DataError::ReadOnly => {
                        return (
                            StatusCode::METHOD_NOT_ALLOWED,
                            [(header::ALLOW, "GET")],
                            Json(GenericError::with_vendor_code("read-only", e.to_string())),
                        )
                            .into_response();
                    }
                    DataError::InvalidValue { path, message } => {
                        let details = data_error(format!("/data{path}"), message);
                        return (StatusCode::BAD_REQUEST, Json(details)).into_response();
                    }
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

                (
                    status,
                    GenericError::new(ErrorCode::ErrorResponse, message).into(),
                )
            }
            Self::BulkData(e) => {
                let status = match e {
                    BulkDataError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
                    BulkDataError::DeletionFailed(_) => StatusCode::CONFLICT,
                    BulkDataError::InvalidRequest(_) => StatusCode::BAD_REQUEST,
                    BulkDataError::NotFound(_) => StatusCode::NOT_FOUND,
                };

                // Sanitize internal errors - log details, return generic message
                let message = match e {
                    BulkDataError::Internal(msg) => {
                        tracing::error!(target: "srv", error = %msg, "Internal error");
                        "An internal error occurred".to_string()
                    }
                    _ => e.to_string(),
                };

                (
                    status,
                    GenericError::new(ErrorCode::ErrorResponse, message).into(),
                )
            }
            Self::BadQuery(_) => (
                StatusCode::BAD_REQUEST,
                GenericError::new(ErrorCode::IncompleteRequest, "Bad request").into(),
            ),
            Self::InvalidHeader(_) => (
                StatusCode::BAD_REQUEST,
                GenericError::new(ErrorCode::IncompleteRequest, "Bad request headers").into(),
            ),
            Self::BadBody(rejection) => body_rejection(rejection),
            Self::Topology(e) => {
                let status = match e {
                    TopologyError::NotFound(_) => StatusCode::NOT_FOUND,
                };
                tracing::error!(target: "srv", error = %e, "Topology error");
                let message = e.to_string();
                (
                    status,
                    GenericError::new(ErrorCode::ErrorResponse, message).into(),
                )
            }
        };
        (status, Json(details)).into_response()
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

        assert_eq!(response.status(), 405);
        assert_eq!(response.headers()[header::ALLOW], "GET");

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error_code"], "vendor-specific");
        assert_eq!(json["vendor_code"], "read-only");
    }

    #[tokio::test]
    async fn test_error_data_invalid_value() {
        let error = Error::Data(DataError::InvalidValue {
            path: "/level".into(),
            message: "expected u8".into(),
        });
        let response = error.into_response();

        assert_eq!(response.status(), 400);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["path"], "/data/level");
        assert_eq!(json["error"]["error_code"], "incomplete-request");
        assert_eq!(json["error"]["message"], "expected u8");
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
        use axum::{Router, body::Body, http::Request, routing::get};
        use axum_extra::extract::{Query, WithRejection};
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

    async fn put_body(
        content_type: Option<&'static str>,
        body: &'static str,
    ) -> (StatusCode, serde_json::Value) {
        use axum::{Router, body::Body, http::Request, routing::put};
        use axum_extra::extract::WithRejection;
        use opensovd_models::data::WriteRequest;
        use tower::ServiceExt;

        async fn handler(
            WithRejection(Json(_body), _): WithRejection<Json<WriteRequest>, Error>,
        ) -> StatusCode {
            StatusCode::NO_CONTENT
        }

        let app = Router::new().route("/test", put(handler));
        let mut request = Request::builder().method("PUT").uri("/test");
        if let Some(content_type) = content_type {
            request = request.header("content-type", content_type);
        }
        let request = request.body(Body::from(body)).unwrap();
        let response = app.oneshot(request).await.unwrap();
        let status = response.status();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        (status, serde_json::from_slice(&body).unwrap())
    }

    #[tokio::test]
    async fn test_error_body_missing_data() {
        let (status, json) = put_body(Some("application/json"), r#"{"value": 42}"#).await;
        assert_eq!(status, 400);
        assert_eq!(json["path"], "/data");
        assert_eq!(json["error"]["error_code"], "incomplete-request");
    }

    #[tokio::test]
    async fn test_error_body_wrong_signature_type() {
        let (status, json) =
            put_body(Some("application/json"), r#"{"data": 1, "signature": 5}"#).await;
        assert_eq!(status, 400);
        assert_eq!(json["path"], "/signature");
    }

    #[tokio::test]
    async fn test_error_body_not_object() {
        let (status, json) = put_body(Some("application/json"), "[]").await;
        assert_eq!(status, 400);
        assert_eq!(json["path"], "");
    }

    #[tokio::test]
    async fn test_error_body_not_json() {
        let (status, json) = put_body(Some("application/json"), "{").await;
        assert_eq!(status, 400);
        assert_eq!(json["path"], "");
        assert_eq!(json["error"]["error_code"], "incomplete-request");
    }

    #[tokio::test]
    async fn test_error_body_missing_content_type() {
        let (status, json) = put_body(None, r#"{"data": 42}"#).await;
        assert_eq!(status, 415);
        assert_eq!(json["error_code"], "incomplete-request");
    }
}
