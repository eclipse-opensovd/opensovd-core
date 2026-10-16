// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0
//
// AI Disclosure: This file was created with the assistance of OpenAI Codex.
// Assisted-by: OpenAI Codex

//! SOVD logging provider trait and types.
//!
//! A [`LogProvider`] supplies log entries for an application or component.
//! The provider owns log acquisition and filtering; the server passes the
//! requested [`LogFilter`] to the provider without imposing a storage model.
//! Providers can read from an in-memory buffer, a file, a realtime unit, or
//! another backend.
//!
//! # Entity integration
//!
//! Attach a provider to an entity with [`crate::App::with_log_provider`] or
//! [`crate::Component::with_log_provider`]. The server then serves the SOVD
//! log resources relative to the entity, such as:
//!
//! ```text
//! /components/{component_id}/logs
//! /components/{component_id}/logs/entries
//! /components/{component_id}/logs/config
//! ```
//!
//! # Filtering
//!
//! [`LogFilter::severity`] is a threshold. Providers should return entries at
//! or above the requested severity. Use [`LogSeverity::rank`] rather than enum
//! declaration order when comparing levels: lower ranks are more severe, so an
//! `Info` threshold includes fatal, error, warning, and info entries.
//! Backend-specific levels must be normalized by the provider before they are
//! exposed through this API.
//!
//! # Live streaming
//!
//! The optional [`LogProvider::stream`] method supports the OpenSOVD live-log
//! SSE extension. Providers that implement it must also return `true` from
//! [`LogProvider::supports_streaming`]. The server advertises the extension
//! through the `x-opensovd-log-streams` discovery link only for those
//! providers and serves events at `/logs/x-opensovd-log-streams`. Events use
//! a fixed `EventEnvelope<LogEntry>` shape; schema negotiation is therefore
//! not part of the stream query.

use std::pin::Pin;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures_core::Stream;

/// SOVD log severity levels.
///
/// Providers map backend-specific levels to these common severity values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogSeverity {
    Fatal,
    Error,
    Warn,
    Info,
    Debug,
}

impl LogSeverity {
    /// Returns the severity rank used for threshold filtering.
    ///
    /// Lower values represent more severe messages.
    #[must_use]
    pub const fn rank(self) -> u8 {
        match self {
            Self::Fatal => 0,
            Self::Error => 1,
            Self::Warn => 2,
            Self::Info => 3,
            Self::Debug => 4,
        }
    }
}

/// Opaque context identifying the source and format of a log entry.
///
/// Context formats and their attributes are provider-specific. The common
/// logging interface transports the type and attributes without interpreting
/// them; providers are responsible for matching and filtering contexts.
#[derive(Debug, Clone, PartialEq)]
pub struct LogContext {
    pub context_type: String,
    pub attributes: serde_json::Map<String, serde_json::Value>,
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
    #[error("log provider does not support this operation: {0}")]
    NotSupported(String),
    #[error("internal log provider error: {0}")]
    Internal(String),
}

/// A provider for an entity's SOVD log resources.
#[async_trait]
pub trait LogProvider: Send + Sync + 'static {
    async fn entries(&self, filter: LogFilter) -> LogResult<Vec<LogEntry>>;

    /// Returns whether this provider supports live log streaming.
    ///
    /// Providers that implement [`Self::stream`] must override this method to
    /// return `true`. The default is `false`, so snapshot-only providers do not
    /// advertise the live-stream extension.
    fn supports_streaming(&self) -> bool {
        false
    }

    /// Opens a live log stream for the OpenSOVD SSE extension.
    async fn stream(&self, _filter: LogFilter) -> LogResult<LogStream> {
        Err(LogError::NotSupported(
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
        assert!(
            matches!(error, LogError::NotSupported(message) if message.contains("not supported"))
        );
    }
}
