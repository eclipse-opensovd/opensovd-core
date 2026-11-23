// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Fault provider trait and types.

use async_trait::async_trait;
use serde_json::{Map, Value};

/// A fault reported by an entity.
#[derive(Debug, Clone)]
pub struct Fault {
    /// Fault code in the native representation of the entity.
    pub code: String,
    /// Scope of this fault.
    pub scope: Option<String>,
    /// Display code for this fault.
    pub display_code: Option<String>,
    /// Name or description of the fault.
    pub fault_name: String,
    /// Optional identifier for translating the fault name.
    pub fault_translation_id: Option<String>,
    /// Severity, where a lower number is more severe.
    pub severity: Option<u32>,
    /// OEM-specific status attributes.
    pub status: Option<Map<String, Value>>,
    /// Symptom associated with this fault.
    pub symptom: Option<String>,
    /// Optional identifier for translating the symptom.
    pub symptom_translation_id: Option<String>,
    /// Tags assigned to this fault.
    pub tags: Vec<String>,
}

/// A fault and its detail-only environment data.
#[derive(Debug, Clone)]
pub struct FaultDetail {
    pub fault: Fault,
    /// Environment information, when the fault provides it.
    pub environment_data: Option<Value>,
    /// Errors of `environment_data` when that value represents an error.
    pub errors: Vec<FaultDataError>,
    /// JSON Schema 2020-12 of `environment_data`, not of the detail response.
    /// The server puts it in place of `environment_data` in the response
    /// schema, so it must not use `$ref`.
    pub schema: Option<Value>,
}

/// An error of one element of a fault's environment data.
#[derive(Debug, Clone)]
pub struct FaultDataError {
    /// JSON pointer to the erroneous element within the environment data,
    /// empty for the whole value.
    pub path: String,
    /// What went wrong.
    pub message: String,
}

/// Filters accepted when listing faults.
#[derive(Debug, Clone, Default)]
pub struct FaultFilter {
    /// Pairs of a status attribute and the value it must have, OR-combined.
    /// A string attribute must equal the value. Any other attribute is
    /// compared as compact JSON, so the number `1` matches `1` and not `1.0`.
    pub status: Vec<(String, String)>,
    /// Include faults at this severity and all more severe levels. Faults
    /// without a severity are excluded.
    pub severity: Option<u32>,
    /// Include faults whose scope exactly matches this value. Faults without
    /// a scope are excluded.
    pub scope: Option<String>,
    /// Include faults with any of these tags.
    pub tags: Vec<String>,
}

/// Errors that can occur when accessing faults.
#[derive(Debug, Clone, thiserror::Error)]
pub enum FaultError {
    /// The requested fault does not exist.
    #[error("fault not found: {0}")]
    NotFound(String),
    /// The provider does not support this operation.
    #[error("fault provider does not support this operation: {0}")]
    NotSupported(String),
    /// The provider failed while accessing a fault.
    #[error("internal error: {0}")]
    Internal(String),
}

/// A `Result` alias where the `Err` variant is [`FaultError`].
pub type Result<T> = std::result::Result<T, FaultError>;

/// Provides faults for one entity.
#[async_trait]
pub trait FaultProvider: Send + Sync + 'static {
    /// Returns faults matching the supplied filter.
    async fn fault_list(&self, filter: FaultFilter) -> Result<Vec<Fault>>;

    /// Returns one fault, its optional environment data, and its schema when requested.
    async fn fault_read(&self, fault_code: &str, include_schema: bool) -> Result<FaultDetail>;
}
