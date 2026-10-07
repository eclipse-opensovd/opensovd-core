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
    #[serde(rename = "DLT_FATAL")]
    DltFatal,
    #[serde(rename = "DLT_ERROR")]
    DltError,
    #[serde(rename = "DLT_WARN")]
    DltWarn,
    #[serde(rename = "DLT_INFO")]
    DltInfo,
    #[serde(rename = "DLT_DEBUG")]
    DltDebug,
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

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use chrono::{TimeZone, Utc};
    use serde_json::json;

    use super::*;

    #[test]
    fn dlt_severities_use_the_autosar_wire_names() {
        for (severity, expected) in [
            (LogSeverity::DltFatal, "DLT_FATAL"),
            (LogSeverity::DltError, "DLT_ERROR"),
            (LogSeverity::DltWarn, "DLT_WARN"),
            (LogSeverity::DltInfo, "DLT_INFO"),
            (LogSeverity::DltDebug, "DLT_DEBUG"),
        ] {
            assert_eq!(serde_json::to_value(severity).unwrap(), json!(expected));
            assert_eq!(
                serde_json::from_value::<LogSeverity>(json!(expected)).unwrap(),
                severity
            );
        }
    }

    #[test]
    fn generic_and_dlt_severities_are_distinguishable() {
        assert_eq!(
            serde_json::to_value(LogSeverity::Info).unwrap(),
            json!("info")
        );
        assert_ne!(LogSeverity::Info, LogSeverity::DltInfo);
        assert!(serde_json::from_str::<LogSeverity>(r#""DLT_TRACE""#).is_err());
    }

    #[test]
    fn event_envelope_contains_timestamp_and_payload() {
        let timestamp = Utc.with_ymd_and_hms(2026, 10, 7, 8, 0, 0).unwrap();
        let envelope = EventEnvelope {
            timestamp,
            payload: Some(LogEntry {
                timestamp,
                context: LogContext {
                    context_type: "AUTOSAR_DLT".into(),
                    attributes: [
                        ("application_id".into(), json!("TRAC")),
                        ("context_id".into(), json!("Main")),
                    ]
                    .into_iter()
                    .collect(),
                },
                severity: LogSeverity::DltInfo,
                msg: "heartbeat".into(),
                href: None,
            }),
            error: None,
        };

        let value = serde_json::to_value(envelope).unwrap();
        assert_eq!(value["timestamp"], json!("2026-10-07T08:00:00Z"));
        assert_eq!(value["payload"]["severity"], json!("DLT_INFO"));
        assert!(value.get("error").is_none());
    }

    #[test]
    fn log_resources_advertise_the_live_entries_extension() {
        let resources = LogResources {
            entries: "/sovd/v1/apps/diag-app/logs/entries".to_string().into(),
            config: "/sovd/v1/apps/diag-app/logs/config".to_string().into(),
            live_entries: Some(
                "/sovd/v1/apps/diag-app/logs/entries/stream"
                    .to_string()
                    .into(),
            ),
        };

        let value = serde_json::to_value(resources).unwrap();
        assert_eq!(
            value["x-opensovd-live-entries"],
            json!("/sovd/v1/apps/diag-app/logs/entries/stream")
        );
    }
}
