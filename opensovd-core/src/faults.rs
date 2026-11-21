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
}

/// Filters accepted when listing faults.
#[derive(Debug, Clone, Default)]
pub struct FaultFilter {
    /// Status predicates are OR-combined.
    pub status: Vec<(String, String)>,
    /// Include faults at this severity and all more severe levels.
    pub severity: Option<u32>,
    /// Include faults with this scope.
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

    /// Returns one fault and its optional environment data.
    async fn fault_read(&self, fault_code: &str) -> Result<FaultDetail>;
}
