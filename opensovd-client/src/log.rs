// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0
//
// This file was created with the assistance of generative AI.

//! Client helpers for the SOVD logging resource.

use chrono::{DateTime, Utc};
use opensovd_models::{
    Response,
    log::{LogConfigurationResponse, LogEntries, LogSeverity},
};

use crate::{
    client::{Client, schema_query},
    error::Result,
};

pub struct LogRequest<'a> {
    pub(crate) client: &'a Client,
    pub(crate) path: String,
    pub(crate) severity: Option<LogSeverity>,
    pub(crate) created_after: Option<DateTime<Utc>>,
    pub(crate) created_before: Option<DateTime<Utc>>,
    pub(crate) schema: bool,
}

impl LogRequest<'_> {
    #[must_use]
    pub fn severity(mut self, severity: LogSeverity) -> Self {
        self.severity = Some(severity);
        self
    }

    #[must_use]
    pub fn created_after(mut self, timestamp: DateTime<Utc>) -> Self {
        self.created_after = Some(timestamp);
        self
    }

    #[must_use]
    pub fn created_before(mut self, timestamp: DateTime<Utc>) -> Self {
        self.created_before = Some(timestamp);
        self
    }

    #[must_use]
    pub fn schema(mut self, include: bool) -> Self {
        self.schema = include;
        self
    }

    pub async fn send(&self) -> Result<Response<LogEntries>> {
        let mut query = Vec::new();
        if let Some(severity) = self.severity {
            query.push(("severity", severity_wire_name(severity)));
        }
        let after;
        if let Some(timestamp) = self.created_after {
            after = serde_json::to_string(&timestamp)?;
            query.push(("created-after", after.trim_matches('"')));
        }
        let before;
        if let Some(timestamp) = self.created_before {
            before = serde_json::to_string(&timestamp)?;
            query.push(("created-before", before.trim_matches('"')));
        }
        query.extend_from_slice(schema_query(self.schema));
        self.client.get(&self.path, &query).await
    }
}

pub(crate) async fn configuration(
    client: &Client,
    path: &str,
) -> Result<Response<LogConfigurationResponse>> {
    client.get(path, &[]).await
}

fn severity_wire_name(value: LogSeverity) -> &'static str {
    match value {
        LogSeverity::Fatal => "fatal",
        LogSeverity::Error => "error",
        LogSeverity::Warn => "warn",
        LogSeverity::Info => "info",
        LogSeverity::Debug => "debug",
        LogSeverity::DltFatal => "DLT_FATAL",
        LogSeverity::DltError => "DLT_ERROR",
        LogSeverity::DltWarn => "DLT_WARN",
        LogSeverity::DltInfo => "DLT_INFO",
        LogSeverity::DltDebug => "DLT_DEBUG",
    }
}
