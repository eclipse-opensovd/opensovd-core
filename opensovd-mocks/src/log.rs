// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0
//
// This file was created with the assistance of generative AI.

//! In-memory log provider for tests and examples.

use std::sync::{Arc, PoisonError, RwLock};

use async_trait::async_trait;
use futures::stream;
use opensovd_core::{LogConfiguration, LogEntry, LogFilter, LogProvider, LogResult, LogStream};

#[derive(Clone, Default, Debug)]
pub struct InMemoryLogProvider {
    entries: Arc<RwLock<Vec<LogEntry>>>,
}

impl InMemoryLogProvider {
    #[must_use]
    pub fn with_entries(self, entries: impl IntoIterator<Item = LogEntry>) -> Self {
        *self.entries.write().unwrap_or_else(PoisonError::into_inner) =
            entries.into_iter().collect();
        self
    }

    pub fn push(&self, entry: LogEntry) {
        self.entries
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .push(entry);
    }
}

#[async_trait]
impl LogProvider for InMemoryLogProvider {
    async fn entries(&self, filter: LogFilter) -> LogResult<Vec<LogEntry>> {
        Ok(filtered_entries(
            self.entries
                .read()
                .unwrap_or_else(PoisonError::into_inner)
                .clone(),
            &filter,
        ))
    }

    async fn stream(&self, filter: LogFilter) -> LogResult<LogStream> {
        let entries = self
            .entries
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        Ok(Box::pin(stream::iter(
            filtered_entries(entries, &filter).into_iter().map(Ok),
        )))
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

fn filtered_entries(entries: Vec<LogEntry>, filter: &LogFilter) -> Vec<LogEntry> {
    entries
        .into_iter()
        .filter(|entry| {
            filter
                .severity
                .is_none_or(|severity| entry.severity.rank() <= severity.rank())
        })
        .filter(|entry| {
            filter
                .created_after
                .is_none_or(|after| entry.timestamp > after)
        })
        .filter(|entry| {
            filter
                .created_before
                .is_none_or(|before| entry.timestamp < before)
        })
        .collect()
}
