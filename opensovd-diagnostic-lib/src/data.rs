// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Data structures for diagnostic information

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Data category following SOVD standard
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum DataCategory {
    /// Current/live data
    CurrentData,
    /// Identification data
    IdentData,
    /// Configuration data
    ConfigData,
    /// Fault/DTC data
    FaultData,
    /// System information
    SysInfo,
    /// Custom category
    Custom(String),
}

impl DataCategory {
    pub fn as_str(&self) -> &str {
        match self {
            DataCategory::CurrentData => "currentData",
            DataCategory::IdentData => "identData",
            DataCategory::ConfigData => "configData",
            DataCategory::FaultData => "faultData",
            DataCategory::SysInfo => "sysInfo",
            DataCategory::Custom(s) => s,
        }
    }
}

/// Metadata for a diagnostic data item
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DataItem {
    /// Unique identifier for the data item
    pub id: String,

    /// Human-readable name
    pub name: String,

    /// Data category
    pub category: DataCategory,

    /// Optional translation ID for i18n
    #[serde(skip_serializing_if = "Option::is_none")]
    pub translation_id: Option<String>,

    /// Groups this data belongs to
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<String>,

    /// Tags for filtering and organization
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,

    /// JSON schema for the data value
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<serde_json::Value>,

    /// Whether the data can be read
    pub is_readable: bool,

    /// Whether the data can be written
    pub is_writable: bool,
}

/// A data value with optional schema
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct DataValue {
    /// The actual data value
    pub value: serde_json::Value,

    /// Optional JSON schema for validation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<serde_json::Value>,
}

/// Request body for writing data
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct WriteDataRequest {
    /// The value to write
    pub value: serde_json::Value,
}

/// Health check response
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct HealthResponse {
    pub status: String,
    pub timestamp: u64,
}

/// API information response
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ApiInfo {
    pub name: String,
    pub version: String,
    pub description: String,
}
