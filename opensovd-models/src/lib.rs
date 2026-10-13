// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

pub mod bulkdata;
pub mod data;
pub mod discovery;
pub mod error;
pub mod log;
pub mod types;
pub mod version;

pub use error::{DataError, ErrorCode, ErrorDetails, GenericError};
use serde::{Deserialize, Serialize};
pub use types::{DateTime, JsonPointer, UriReference};

/// SOVD tag definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
pub struct Tag {
    /// Name of the tag (identifier)
    pub name: String,
    /// Optional description of the tag
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Identifier for translating the name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub translation_id: Option<String>,
}

/// A collection of items.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
pub struct Collection<T> {
    pub items: Vec<T>,
    /// Schema of the response, sent for `include-schema=true`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<serde_json::Value>,
}
