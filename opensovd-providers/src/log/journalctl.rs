// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Read logs for a configured systemd unit through `journalctl`.
//!
//! Enable the `journalctl` feature and attach a provider to an app or component:
//!
//! ```ignore
//! let logs = JournalctlLogProvider::new("save-the-spoiler.service")?;
//! let app = App::new("save-the-spoiler", "Save the Spoiler").with_log_provider(logs);
//! ```

use std::process::Stdio;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::stream;
use opensovd_core::{
    LogConfiguration, LogContext, LogEntry, LogError, LogFilter, LogProvider, LogResult,
    LogSeverity, LogStream,
};
use serde_json::{Map, Value};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::{Child, Command},
};

const DEFAULT_MAX_ENTRIES: usize = 1000;

/// A read-only SOVD log provider for a single systemd unit.
///
/// The unit is supplied explicitly when creating the provider; it is passed
/// directly as a process argument and is never interpreted by a shell.
/// Snapshot reads return up to 1000 most recent entries by default. Live
/// streams return that initial snapshot and then follow new journal entries.
/// A stream filtered by `created-before` is finite and returns a snapshot only.
#[derive(Clone, Debug)]
pub struct JournalctlLogProvider {
    unit: String,
    max_entries: usize,
}

impl JournalctlLogProvider {
    /// Create a provider for a systemd unit, with a default snapshot limit of
    /// 1000 entries.
    ///
    /// # Errors
    ///
    /// Returns an error if `unit` is empty.
    pub fn new(unit: impl Into<String>) -> LogResult<Self> {
        let unit = unit.into();
        if unit.is_empty() {
            return Err(LogError::InvalidRequest(
                "systemd unit name must not be empty".into(),
            ));
        }

        Ok(Self {
            unit,
            max_entries: DEFAULT_MAX_ENTRIES,
        })
    }

    /// Set the maximum number of entries returned by a snapshot or included
    /// as the initial entries of a live stream.
    ///
    /// # Errors
    ///
    /// Returns an error if `max_entries` is zero.
    pub fn with_max_entries(mut self, max_entries: usize) -> LogResult<Self> {
        if max_entries == 0 {
            return Err(LogError::InvalidRequest(
                "maximum log entries must be greater than zero".into(),
            ));
        }
        self.max_entries = max_entries;
        Ok(self)
    }

    fn command_args(&self, filter: &LogFilter, follow: bool) -> Vec<String> {
        let mut args = vec![
            "--no-pager".into(),
            "--output=json".into(),
            format!("--unit={}", self.unit),
            format!("--lines={}", self.max_entries),
        ];

        if let Some(severity) = filter.severity {
            args.push(format!("--priority=0..{}", max_priority(severity)));
        }
        if let Some(created_after) = filter.created_after {
            args.push(format!("--since={}", created_after.to_rfc3339()));
        }
        if let Some(created_before) = filter.created_before {
            args.push(format!("--until={}", created_before.to_rfc3339()));
        }
        if follow {
            args.push("--follow".into());
        }

        args
    }

    async fn read_entries(&self, filter: &LogFilter) -> LogResult<Vec<LogEntry>> {
        let output = Command::new("journalctl")
            .args(self.command_args(filter, false))
            .output()
            .await
            .map_err(|error| {
                LogError::Internal(format!("failed to start journalctl: {error}"))
            })?;

        if !output.status.success() {
            return Err(LogError::Internal(format!(
                "journalctl exited with status {:?}",
                output.status
            )));
        }

        parse_entries(&output.stdout, filter)
    }

    async fn live_stream(&self, filter: LogFilter) -> LogResult<LogStream> {
        let mut child = Command::new("journalctl")
            .args(self.command_args(&filter, true))
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|error| {
                LogError::Internal(format!("failed to start journalctl: {error}"))
            })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            LogError::Internal("journalctl did not provide an output stream".into())
        })?;
        let lines = BufReader::new(stdout).lines();

        let entries = stream::unfold(
            Some((child, lines, filter)),
            |state| async move {
                let (mut child, mut lines, filter) = state?;
                loop {
                    match lines.next_line().await {
                        Ok(Some(line)) if line.trim().is_empty() => {}
                        Ok(Some(line)) => match parse_entry_line(&line) {
                            Ok(entry) if matches_filter(&entry, &filter) => {
                                return Some((Ok(entry), Some((child, lines, filter))));
                            }
                            Ok(_) => {}
                            Err(error) => {
                                return Some((Err(error), Some((child, lines, filter))));
                            }
                        },
                        Ok(None) => {
                            return match child.wait().await {
                                Ok(status) if status.success() => None,
                                Ok(status) => Some((
                                    Err(LogError::Internal(format!(
                                        "journalctl exited with status {status:?}"
                                    ))),
                                    None,
                                )),
                                Err(error) => Some((
                                    Err(LogError::Internal(format!(
                                        "failed to wait for journalctl: {error}"
                                    ))),
                                    None,
                                )),
                            };
                        }
                        Err(error) => {
                            return Some((
                                Err(LogError::Internal(format!(
                                    "failed to read journalctl output: {error}"
                                ))),
                                None,
                            ));
                        }
                    }
                }
            },
        );

        Ok(Box::pin(entries))
    }
}

#[async_trait]
impl LogProvider for JournalctlLogProvider {
    async fn entries(&self, filter: LogFilter) -> LogResult<Vec<LogEntry>> {
        self.read_entries(&filter).await
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    async fn stream(&self, filter: LogFilter) -> LogResult<LogStream> {
        if filter.created_before.is_some() {
            return Ok(Box::pin(stream::iter(
                self.read_entries(&filter)
                    .await?
                    .into_iter()
                    .map(Ok),
            )));
        }

        self.live_stream(filter).await
    }

    async fn configuration(&self) -> LogResult<Vec<LogConfiguration>> {
        Ok(Vec::new())
    }

    async fn configure(&self, _configuration: Vec<LogConfiguration>) -> LogResult<()> {
        Err(LogError::NotSupported(
            "journalctl log configuration is read-only".into(),
        ))
    }

    async fn reset_configuration(&self) -> LogResult<()> {
        Err(LogError::NotSupported(
            "journalctl log configuration is read-only".into(),
        ))
    }
}

fn parse_entries(output: &[u8], filter: &LogFilter) -> LogResult<Vec<LogEntry>> {
    output
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.iter().all(u8::is_ascii_whitespace))
        .map(parse_entry_line)
        .filter_map(|result| match result {
            Ok(entry) if matches_filter(&entry, filter) => Some(Ok(entry)),
            Ok(_) => None,
            Err(error) => Some(Err(error)),
        })
        .collect()
}

fn parse_entry_line(line: &str) -> LogResult<LogEntry> {
    let value: Value = serde_json::from_str(line)
        .map_err(|error| LogError::Internal(format!("invalid journalctl JSON entry: {error}")))?;
    let fields = value
        .as_object()
        .ok_or_else(|| LogError::Internal("journalctl entry is not a JSON object".into()))?;

    let timestamp_micros = fields
        .get("__REALTIME_TIMESTAMP")
        .and_then(field_as_string)
        .and_then(|value| value.parse::<i64>().ok())
        .ok_or_else(|| LogError::Internal("journalctl entry has no valid timestamp".into()))?;
    let timestamp = DateTime::<Utc>::from_timestamp_micros(timestamp_micros)
        .ok_or_else(|| LogError::Internal("journalctl timestamp is out of range".into()))?;
    let msg = fields
        .get("MESSAGE")
        .and_then(Value::as_str)
        .ok_or_else(|| LogError::Internal("journalctl entry has no string message".into()))?
        .to_owned();
    let priority = match fields.get("PRIORITY") {
        None => 6,
        Some(value) => field_as_string(value)
            .and_then(|value| value.parse::<u8>().ok())
            .filter(|priority| *priority <= 7)
            .ok_or_else(|| LogError::Internal("journalctl entry has invalid priority".into()))?,
    };

    let mut attributes = Map::new();
    for (field, name) in [
        ("_HOSTNAME", "hostname"),
        ("_SYSTEMD_UNIT", "unit"),
        ("SYSLOG_IDENTIFIER", "identifier"),
        ("_COMM", "process"),
        ("_PID", "pid"),
        ("SYSLOG_FACILITY", "facility"),
        ("PRIORITY", "priority"),
    ] {
        if let Some(value) = fields.get(field) {
            attributes.insert(name.into(), value.clone());
        }
    }

    Ok(LogEntry {
        timestamp,
        context: LogContext {
            context_type: "systemd-journal".into(),
            attributes,
        },
        severity: severity(priority),
        msg,
        href: None,
    })
}

fn field_as_string(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::to_owned)
        .or_else(|| value.as_u64().map(|number| number.to_string()))
}

fn severity(priority: u8) -> LogSeverity {
    match priority {
        0..=2 => LogSeverity::Fatal,
        3 => LogSeverity::Error,
        4 => LogSeverity::Warn,
        5..=6 => LogSeverity::Info,
        _ => LogSeverity::Debug,
    }
}

fn max_priority(severity: LogSeverity) -> u8 {
    match severity {
        LogSeverity::Fatal => 2,
        LogSeverity::Error => 3,
        LogSeverity::Warn => 4,
        LogSeverity::Info => 6,
        LogSeverity::Debug => 7,
    }
}

fn matches_filter(entry: &LogEntry, filter: &LogFilter) -> bool {
    filter
        .severity
        .is_none_or(|threshold| entry.severity.rank() <= threshold.rank())
        && filter
            .created_after
            .is_none_or(|after| entry.timestamp > after)
        && filter
            .created_before
            .is_none_or(|before| entry.timestamp < before)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_journal_entries_and_maps_syslog_severity() {
        let entry = parse_entry_line(
            r#"{"__REALTIME_TIMESTAMP":"1794506400123456","MESSAGE":"service ready","PRIORITY":"3","_HOSTNAME":"ecu-1","_SYSTEMD_UNIT":"save-the-spoiler.service","SYSLOG_IDENTIFIER":"spoiler","_COMM":"spoiler","_PID":"42"}"#,
        )
        .unwrap();

        assert_eq!(entry.timestamp.timestamp_micros(), 1_794_506_400_123_456);
        assert_eq!(entry.msg, "service ready");
        assert_eq!(entry.severity, LogSeverity::Error);
        assert_eq!(entry.context.context_type, "systemd-journal");
        assert_eq!(
            entry.context.attributes.get("unit"),
            Some(&Value::String("save-the-spoiler.service".into()))
        );
    }

    #[test]
    fn severity_filter_maps_to_journalctl_priority_range() {
        let provider = JournalctlLogProvider::new("save-the-spoiler.service").unwrap();
        let args = provider.command_args(
            &LogFilter {
                severity: Some(LogSeverity::Warn),
                ..LogFilter::default()
            },
            false,
        );

        assert!(args.contains(&"--priority=0..4".to_owned()));
        assert!(args.contains(&"--unit=save-the-spoiler.service".to_owned()));
    }

    #[test]
    fn live_stream_uses_the_configured_initial_entry_limit() {
        let provider = JournalctlLogProvider::new("save-the-spoiler.service")
            .unwrap()
            .with_max_entries(17)
            .unwrap();
        let args = provider.command_args(&LogFilter::default(), true);

        assert!(args.contains(&"--lines=17".to_owned()));
        assert!(args.contains(&"--follow".to_owned()));
    }

    #[test]
    fn unit_is_passed_as_a_single_argument() {
        let provider = JournalctlLogProvider::new("app;touch /tmp/not-run").unwrap();
        let args = provider.command_args(&LogFilter::default(), false);

        assert!(args.contains(&"--unit=app;touch /tmp/not-run".to_owned()));
        assert_eq!(args.len(), 4);
    }

    #[test]
    fn parses_numeric_journal_fields() {
        let entry = parse_entry_line(
            r#"{"__REALTIME_TIMESTAMP":1794506400000000,"MESSAGE":"debug","PRIORITY":7,"_PID":42}"#,
        )
        .unwrap();

        assert_eq!(entry.severity, LogSeverity::Debug);
        assert_eq!(
            entry.context.attributes.get("pid"),
            Some(&Value::Number(42.into()))
        );
    }

    #[test]
    fn rejects_empty_units_and_zero_entry_limits() {
        assert!(JournalctlLogProvider::new("").is_err());
        assert!(
            JournalctlLogProvider::new("service.service")
                .unwrap()
                .with_max_entries(0)
                .is_err()
        );
    }

    #[tokio::test]
    async fn provider_does_not_claim_to_configure_the_journal() {
        let provider = JournalctlLogProvider::new("service.service").unwrap();
        assert!(provider.configuration().await.unwrap().is_empty());
        assert!(matches!(
            provider.configure(Vec::new()).await,
            Err(LogError::NotSupported(_))
        ));
        assert!(matches!(
            provider.reset_configuration().await,
            Err(LogError::NotSupported(_))
        ));
    }
}
