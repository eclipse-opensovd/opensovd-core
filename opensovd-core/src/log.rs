// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0
//
// This file was created with the assistance of generative AI.

//! SOVD logging provider trait and types.

use std::pin::Pin;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures_core::Stream;

/// SOVD log severity, ordered from most to least severe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogSeverity {
    Fatal,
    Error,
    Warn,
    Info,
    Debug,
}

/// Context identifying the source and format of a log entry.
#[derive(Debug, Clone, PartialEq)]
pub enum LogContext {
    Rfc5424 {
        host: Option<String>,
        process: Option<String>,
        pid: Option<i64>,
    },
    AutosarDlt {
        session: Option<String>,
        session_id: Option<String>,
        application_id: Option<String>,
        context_id: Option<String>,
        message_id: Option<String>,
    },
    Custom {
        context_type: String,
        attributes: serde_json::Map<String, serde_json::Value>,
    },
}

/// A log entry exposed through the SOVD API.
#[derive(Debug, Clone, PartialEq)]
pub struct LogEntry {
    pub timestamp: DateTime<Utc>,
    pub context: LogContext,
    pub severity: LogSeverity,
    pub msg: String,
    pub href: Option<String>,
}

/// Filter for retrieving SOVD log entries.
#[derive(Debug, Clone, Default)]
pub struct LogFilter {
    pub severity: Option<LogSeverity>,
    pub created_after: Option<DateTime<Utc>>,
    pub created_before: Option<DateTime<Utc>>,
}

/// A logging configuration for one context.
#[derive(Debug, Clone, PartialEq)]
pub struct LogConfiguration {
    pub context: LogContext,
    pub severity: LogSeverity,
}

/// Errors returned by a [`LogProvider`].
#[derive(Debug, thiserror::Error)]
pub enum LogError {
    #[error("invalid log request: {0}")]
    InvalidRequest(String),
    #[error("log resource not found: {0}")]
    NotFound(String),
    #[error("internal log provider error: {0}")]
    Internal(String),
}

/// A provider for an entity's SOVD log resources.
#[async_trait]
pub trait LogProvider: Send + Sync + 'static {
    async fn entries(&self, filter: LogFilter) -> Result<Vec<LogEntry>>;

    /// Opens a live log stream for the OpenSOVD SSE extension.
    async fn stream(&self, _filter: LogFilter) -> Result<LogStream> {
        Err(LogError::Internal(
            "live log streaming is not supported by this provider".into(),
        ))
    }

    async fn configuration(&self) -> Result<Vec<LogConfiguration>>;

    async fn configure(&self, configuration: Vec<LogConfiguration>) -> Result<()>;

    async fn reset_configuration(&self) -> Result<()>;
}

/// A result returned by a [`LogProvider`].
pub type Result<T> = std::result::Result<T, LogError>;

/// A stream of log entries for the OpenSOVD live-log extension.
pub type LogStream = Pin<Box<dyn Stream<Item = Result<LogEntry>> + Send + 'static>>;
