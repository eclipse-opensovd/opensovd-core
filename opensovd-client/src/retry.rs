// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use rand::RngExt as _;

#[derive(Debug, Clone)]
pub struct RetryPolicy {
    pub(crate) max_retries: u32,
    pub(crate) backoff: Duration,
    pub(crate) max_backoff: Duration,
}

impl RetryPolicy {
    const DEFAULT_BACKOFF: Duration = Duration::from_millis(100);
    const DEFAULT_MAX_BACKOFF: Duration = Duration::from_secs(30);

    /// Create a new [`RetryPolicy`] with `max_retries` retry attempts and a default base
    /// backoff of 100 ms.
    #[must_use]
    pub fn new(max_retries: u32) -> Self {
        Self {
            max_retries,
            backoff: Self::DEFAULT_BACKOFF,
            max_backoff: Self::DEFAULT_MAX_BACKOFF,
        }
    }

    /// Set the base backoff duration.
    #[must_use]
    pub fn backoff(mut self, duration: Duration) -> Self {
        self.backoff = duration;
        self
    }

    /// Set the maximum backoff duration (default: 30 s).
    #[must_use]
    pub fn max_backoff(mut self, duration: Duration) -> Self {
        self.max_backoff = duration;
        self
    }

    /// Compute the sleep duration before the next attempt using full jitter.
    pub(crate) fn delay(&self, attempt: u32) -> Duration {
        let base_ms = u64::try_from(self.backoff.as_millis()).unwrap_or(u64::MAX);
        let max_ms = u64::try_from(self.max_backoff.as_millis()).unwrap_or(u64::MAX);
        let factor = 2u64.saturating_pow(attempt);
        let ceiling = base_ms.saturating_mul(factor).min(max_ms);
        let delay_ms = if ceiling > 0 {
            rand::rng().random_range(0..ceiling)
        } else {
            0
        };
        Duration::from_millis(delay_ms)
    }
}
