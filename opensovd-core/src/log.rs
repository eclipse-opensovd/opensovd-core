// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0
//
// This file was created with the assistance of generative AI.

//! SOVD logging provider trait and types.
//!
//! A [`LogProvider`] supplies log entries for an application or component.
//! The provider owns log acquisition and filtering; the server passes the
//! requested [`LogFilter`] to the provider without imposing a storage model.
//! Providers can read from an in-memory buffer, a file, a DLT client, a
//! realtime unit, or another backend.
//!
//! # Entity integration
//!
//! Attach a provider to an entity with [`crate::App::with_log_provider`] or
//! [`crate::Component::with_log_provider`]. The server then exposes the
//! corresponding SOVD log resources under the generic entity URI:
//!
//! ```text
//! /sovd/v1/{entity-collection}/{entity-id}/logs
//! /sovd/v1/{entity-collection}/{entity-id}/logs/entries
//! /sovd/v1/{entity-collection}/{entity-id}/logs/config
//! ```
//!
//! # Filtering
//!
//! [`LogFilter::severity`] is a threshold. Providers should return entries at
//! or above the requested severity. Use [`LogSeverity::rank`] rather than enum
//! declaration order when comparing levels: lower ranks are more severe, so an
//! `Info` threshold includes fatal, error, warning, and info entries.
//! Generic and AUTOSAR DLT spellings with the same level share a rank while
//! remaining distinct wire values.
//!
//! # Live streaming
//!
//! The optional [`LogProvider::stream`] method supports the OpenSOVD live-log
//! SSE extension. The server advertises it through the
//! `x-opensovd-live-entries` discovery link and serves events at
//! `/logs/entries/stream`. Events use a fixed `EventEnvelope<LogEntry>` shape;
//! schema negotiation is therefore not part of the stream query.

use std::pin::Pin;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures_core::Stream;

/// SOVD log severity levels.
///
/// The `Dlt*` variants preserve AUTOSAR DLT vocabulary when an entry uses an
/// `AUTOSAR_DLT` context. Generic severities remain available for other
/// contexts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogSeverity {
    Fatal,
    Error,
    Warn,
    Info,
    Debug,
    DltFatal,
    DltError,
    DltWarn,
    DltInfo,
    DltDebug,
}

impl LogSeverity {
    /// Returns the severity rank used for threshold filtering.
    ///
    /// Lower values represent more severe messages. Generic and AUTOSAR DLT
    /// spellings intentionally share ranks so providers can filter either
    /// representation without relying on enum declaration order.
    #[must_use]
    pub const fn rank(self) -> u8 {
        match self {
            Self::Fatal | Self::DltFatal => 0,
            Self::Error | Self::DltError => 1,
            Self::Warn | Self::DltWarn => 2,
            Self::Info | Self::DltInfo => 3,
            Self::Debug | Self::DltDebug => 4,
        }
    }
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
    async fn entries(&self, filter: LogFilter) -> LogResult<Vec<LogEntry>>;

    /// Opens a live log stream for the OpenSOVD SSE extension.
    async fn stream(&self, _filter: LogFilter) -> LogResult<LogStream> {
        Err(LogError::Internal(
            "live log streaming is not supported by this provider".into(),
        ))
    }

    async fn configuration(&self) -> LogResult<Vec<LogConfiguration>>;

    async fn configure(&self, configuration: Vec<LogConfiguration>) -> LogResult<()>;

    async fn reset_configuration(&self) -> LogResult<()>;
}

/// A result returned by a [`LogProvider`].
pub type LogResult<T> = Result<T, LogError>;

/// A stream of log entries for the OpenSOVD live-log extension.
pub type LogStream = Pin<Box<dyn Stream<Item = LogResult<LogEntry>> + Send + 'static>>;

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use async_trait::async_trait;

    use super::*;

    struct Provider;

    #[async_trait]
    impl LogProvider for Provider {
        async fn entries(&self, _filter: LogFilter) -> LogResult<Vec<LogEntry>> {
            Ok(Vec::new())
        }

        async fn configuration(&self) -> LogResult<Vec<LogConfiguration>> {
            Ok(Vec::new())
        }

        async fn configure(&self, _configuration: Vec<LogConfiguration>) -> LogResult<()> {
            Ok(())
        }

        async fn reset_configuration(&self) -> LogResult<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn providers_without_stream_support_return_a_clear_error() {
        let Err(error) = Provider.stream(LogFilter::default()).await else {
            panic!("default stream implementation unexpectedly succeeded")
        };
        assert!(matches!(error, LogError::Internal(message) if message.contains("not supported")));
    }

    #[test]
    fn severity_rank_is_independent_of_enum_declaration_order() {
        assert!(LogSeverity::DltFatal.rank() < LogSeverity::DltInfo.rank());
        assert_eq!(LogSeverity::Info.rank(), LogSeverity::DltInfo.rank());
        assert_eq!(LogSeverity::Debug.rank(), LogSeverity::DltDebug.rank());
    }
}
