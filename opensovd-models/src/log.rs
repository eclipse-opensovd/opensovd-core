// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0
//
// AI Disclosure: This file was created with the assistance of OpenAI Codex.
// Assisted-by: OpenAI Codex

//! SOVD logging API models.

use serde::{Deserialize, Serialize};

use crate::{Collection, DateTime, UriReference, error::GenericError};

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
    pub timestamp: DateTime,
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
        rename = "x-opensovd-log-streams",
        skip_serializing_if = "Option::is_none"
    )]
    pub stream: Option<UriReference>,
    /// Schema of the response, sent for `include-schema=true`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<serde_json::Value>,
}

/// Event envelope used by the OpenSOVD live-log SSE extension.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
pub struct EventEnvelope<T> {
    pub timestamp: DateTime,
    pub payload: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<GenericError>,
}

pub type LogEntries = Collection<LogEntry>;

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
    /// Schema of the response, sent for `include-schema=true`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct LogEntriesQuery {
    pub severity: Option<LogSeverity>,
    #[serde(rename = "created-after")]
    pub created_after: Option<DateTime>,
    #[serde(rename = "created-before")]
    pub created_before: Option<DateTime>,
    #[serde(default, rename = "include-schema")]
    pub include_schema: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct LogResourcesQuery {
    #[serde(default, rename = "include-schema")]
    pub include_schema: bool,
}

pub type LogConfigurationQuery = LogResourcesQuery;

#[derive(Debug, Clone, Default, Deserialize)]
/// Filters accepted by the live stream. Schema negotiation is not part of the
/// SSE query because every event uses the fixed `EventEnvelope<LogEntry>` shape.
pub struct LogStreamQuery {
    pub severity: Option<LogSeverity>,
    #[serde(rename = "created-after")]
    pub created_after: Option<DateTime>,
    #[serde(rename = "created-before")]
    pub created_before: Option<DateTime>,
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use serde_json::json;

    use super::*;

    #[test]
    fn severities_use_generic_wire_names() {
        assert_eq!(
            serde_json::to_value(LogSeverity::Info).unwrap(),
            json!("info")
        );
    }

    #[test]
    fn event_envelope_contains_timestamp_and_payload() {
        let timestamp = Utc.with_ymd_and_hms(2026, 10, 7, 8, 0, 0).unwrap();
        let envelope = EventEnvelope {
            timestamp,
            payload: Some(LogEntry {
                timestamp,
                context: LogContext {
                    context_type: "custom".into(),
                    attributes: [
                        ("application_id".into(), json!("TRAC")),
                        ("context_id".into(), json!("Main")),
                    ]
                    .into_iter()
                    .collect(),
                },
                severity: LogSeverity::Info,
                msg: "heartbeat".into(),
                href: None,
            }),
            error: None,
        };

        let value = serde_json::to_value(envelope).unwrap();
        assert_eq!(value["timestamp"], json!("2026-10-07T08:00:00Z"));
        assert_eq!(value["payload"]["severity"], json!("info"));
        assert!(value.get("error").is_none());
    }

    #[test]
    fn log_resources_advertise_the_stream_extension() {
        let resources = LogResources {
            entries: "/sovd/v1/apps/diag-app/logs/entries".to_string().into(),
            config: "/sovd/v1/apps/diag-app/logs/config".to_string().into(),
            stream: Some(
                "/sovd/v1/apps/diag-app/logs/x-opensovd-log-streams"
                    .to_string()
                    .into(),
            ),
            schema: None,
        };

        let value = serde_json::to_value(resources).unwrap();
        assert_eq!(
            value["x-opensovd-log-streams"],
            json!("/sovd/v1/apps/diag-app/logs/x-opensovd-log-streams")
        );
    }
}
