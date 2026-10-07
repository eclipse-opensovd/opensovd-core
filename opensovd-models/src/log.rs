// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0
//
// This file was created with the assistance of generative AI.

//! SOVD logging API models.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{UriReference, error::GenericError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum LogSeverity {
    Fatal,
    Error,
    Warn,
    Info,
    Debug,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
pub struct LogContext {
    #[serde(rename = "type")]
    pub context_type: String,
    #[serde(flatten)]
    pub attributes: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
pub struct LogEntry {
    pub timestamp: DateTime<Utc>,
    pub context: LogContext,
    pub severity: LogSeverity,
    pub msg: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub href: Option<UriReference>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
pub struct LogResources {
    pub entries: UriReference,
    pub config: UriReference,
    #[serde(
        rename = "x-opensovd-live-entries",
        skip_serializing_if = "Option::is_none"
    )]
    pub live_entries: Option<UriReference>,
}

/// Event envelope used by the OpenSOVD live-log SSE extension.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
pub struct EventEnvelope<T> {
    pub timestamp: DateTime<Utc>,
    pub payload: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<GenericError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
pub struct LogEntries {
    pub items: Vec<LogEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
pub struct LogConfiguration {
    pub context: LogContext,
    pub severity: LogSeverity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
pub struct LogConfigurationRequest {
    pub items: Vec<LogConfiguration>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
pub struct LogConfigurationResponse {
    pub contexts: Vec<LogConfiguration>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct LogEntriesQuery {
    pub severity: Option<LogSeverity>,
    #[serde(rename = "created-after")]
    pub created_after: Option<DateTime<Utc>>,
    #[serde(rename = "created-before")]
    pub created_before: Option<DateTime<Utc>>,
    #[serde(default, rename = "include-schema")]
    pub include_schema: bool,
}
