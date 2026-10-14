// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0
//
// AI Disclosure: This file was created with the assistance of OpenAI Codex.
// Assisted-by: OpenAI Codex

//! In-memory log provider for tests and examples.

use std::sync::{Arc, PoisonError, RwLock};

use async_trait::async_trait;
use futures::{StreamExt, stream};
use opensovd_core::{LogConfiguration, LogEntry, LogFilter, LogProvider, LogResult, LogStream};
use tokio::sync::broadcast;

#[derive(Clone, Debug)]
pub struct InMemoryLogProvider {
    entries: Arc<RwLock<Vec<LogEntry>>>,
    updates: broadcast::Sender<LogEntry>,
}

impl Default for InMemoryLogProvider {
    fn default() -> Self {
        let (updates, _) = broadcast::channel(256);
        Self {
            entries: Arc::new(RwLock::new(Vec::new())),
            updates,
        }
    }
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
            .push(entry.clone());
        // A provider may have no subscribers. In that case there is nothing
        // to deliver, and the snapshot above still retains the entry.
        let _ = self.updates.send(entry);
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

    fn supports_streaming(&self) -> bool {
        true
    }

    async fn stream(&self, filter: LogFilter) -> LogResult<LogStream> {
        let receiver = self.updates.subscribe();
        let entries = self
            .entries
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let initial = stream::iter(filtered_entries(entries, &filter).into_iter().map(Ok));
        let live_filter = filter.clone();
        let live = stream::unfold(receiver, |mut receiver| async move {
            let entry = loop {
                match receiver.recv().await {
                    Ok(entry) => break entry,
                    // A slow mock subscriber may miss entries. Continue with
                    // the next available entry rather than ending the stream.
                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => return None,
                }
            };
            Some((entry, receiver))
        })
        .filter_map(move |entry| {
            let filter = live_filter.clone();
            async move { matches_filter(&entry, &filter).then_some(Ok(entry)) }
        });

        Ok(Box::pin(initial.chain(live)))
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
        .filter(|entry| matches_filter(entry, filter))
        .collect()
}

fn matches_filter(entry: &LogEntry, filter: &LogFilter) -> bool {
    filter
        .severity
        .is_none_or(|severity| entry.severity.rank() <= severity.rank())
        && filter
            .created_after
            .is_none_or(|after| entry.timestamp > after)
        && filter
            .created_before
            .is_none_or(|before| entry.timestamp < before)
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use opensovd_core::{LogContext, LogSeverity};

    use super::*;

    fn entry(second: u32, severity: LogSeverity, msg: &str) -> LogEntry {
        LogEntry {
            timestamp: Utc.with_ymd_and_hms(2026, 10, 12, 12, 0, second).unwrap(),
            context: LogContext {
                context_type: "RFC5424".into(),
                attributes: [
                    ("host".into(), "test-host".into()),
                    ("process".into(), "test-process".into()),
                    ("pid".into(), 1.into()),
                ]
                .into_iter()
                .collect(),
            },
            severity,
            msg: msg.into(),
            href: None,
        }
    }

    #[tokio::test]
    async fn stream_remains_open_for_entries_pushed_after_subscription() {
        let initial = entry(0, LogSeverity::Info, "initial");
        let provider = InMemoryLogProvider::default().with_entries([initial.clone()]);
        let mut stream = provider.stream(LogFilter::default()).await.unwrap();

        assert_eq!(stream.next().await.unwrap().unwrap(), initial);

        let pushed = entry(1, LogSeverity::Info, "pushed");
        provider.push(pushed.clone());

        assert_eq!(stream.next().await.unwrap().unwrap(), pushed);
    }

    #[tokio::test]
    async fn live_stream_applies_severity_filter() {
        let provider = InMemoryLogProvider::default();
        let mut stream = provider
            .stream(LogFilter {
                severity: Some(LogSeverity::Info),
                ..LogFilter::default()
            })
            .await
            .unwrap();

        provider.push(entry(1, LogSeverity::Debug, "filtered"));
        provider.push(entry(2, LogSeverity::Info, "included"));

        assert_eq!(stream.next().await.unwrap().unwrap().msg, "included");
    }
}
