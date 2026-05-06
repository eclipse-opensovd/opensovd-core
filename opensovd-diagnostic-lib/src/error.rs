// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Error types for the diagnostic library

use thiserror::Error;

/// Errors that can occur in diagnostic operations
#[derive(Error, Debug)]
pub enum DiagnosticError {
    /// Data item not found
    #[error("Data item not found: {0}")]
    NotFound(String),

    /// Data item is read-only
    #[error("Data item is read-only: {0}")]
    ReadOnly(String),

    /// Invalid data format
    #[error("Invalid data format: {0}")]
    InvalidData(String),

    /// Internal error
    #[error("Internal error: {0}")]
    Internal(String),

    /// HTTP server error
    #[error("HTTP server error: {0}")]
    ServerError(String),

    /// Serialization error
    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
}

impl warp::reject::Reject for DiagnosticError {}
