// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

#![cfg_attr(coverage_nightly, feature(coverage_attribute))]

pub mod data;
pub mod discovery;
pub mod error;
pub mod types;
pub mod version;

pub use error::{ErrorCode, GenericError};
use serde::{Deserialize, Serialize};
pub use types::{JsonPointer, UriReference};

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

/// Generic response wrapper with optional schema
#[derive(Debug, Serialize, Deserialize)]
pub struct Response<T> {
    #[serde(flatten)]
    pub data: T,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<serde_json::Value>,
}

/// Generic response for collections with items
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
pub struct Items<T> {
    pub items: Vec<T>,
}
