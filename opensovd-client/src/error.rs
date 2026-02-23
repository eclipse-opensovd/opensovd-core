// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

use opensovd_models::GenericError;

/// Client error type.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Server returned a non-success status code.
    #[error("server error {status}: {error:?}")]
    ApiError {
        /// HTTP status code.
        status: http::StatusCode,
        /// Parsed SOVD error body, if available.
        error: Option<GenericError>,
    },

    /// HTTP request construction error.
    #[error("http error: {0}")]
    Http(#[from] http::Error),

    /// Hyper protocol error.
    #[error("hyper error: {0}")]
    Hyper(#[from] hyper::Error),

    /// Service error (transport, timeout, middleware).
    #[error("service error: {source}")]
    Service {
        /// The underlying error.
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// JSON serialization/deserialization error.
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    /// Invalid URI.
    #[error("invalid uri: {0}")]
    InvalidUri(#[from] http::uri::InvalidUri),
}

/// A `Result` alias where the `Err` variant is [`Error`].
pub type Result<T> = std::result::Result<T, Error>;
