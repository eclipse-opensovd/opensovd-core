// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Shared CLI utilities for `OpenSOVD` binaries.

pub mod trace;

use std::fmt;
use std::path::Path;

use tracing_subscriber::fmt::{
    FmtContext, FormattedFields,
    format::{self, FormatEvent, FormatFields},
};
use tracing_subscriber::registry::LookupSpan;

struct CompactFormat;

impl<S, N> FormatEvent<S, N> for CompactFormat
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        mut writer: format::Writer<'_>,
        event: &tracing::Event<'_>,
    ) -> fmt::Result {
        let level = match *event.metadata().level() {
            tracing::Level::ERROR => "E",
            tracing::Level::WARN => "W",
            tracing::Level::INFO => "I",
            tracing::Level::DEBUG => "D",
            tracing::Level::TRACE => "T",
        };

        let now = chrono::Utc::now();
        let target = event.metadata().target();
        write!(
            writer,
            "{} {} {} ",
            now.format("%Y-%m-%dT%H:%M:%S%.6fZ"),
            level,
            target
        )?;

        if let Some(scope) = ctx.event_scope() {
            let spans: Vec<_> = scope.from_root().collect();
            for span in &spans {
                write!(writer, "{}: ", span.name())?;
            }
            ctx.field_format().format_fields(writer.by_ref(), event)?;
            for span in &spans {
                let ext = span.extensions();
                if let Some(fields) = ext.get::<FormattedFields<N>>()
                    && !fields.is_empty()
                {
                    write!(writer, " {fields}")?;
                }
            }
        } else {
            ctx.field_format().format_fields(writer.by_ref(), event)?;
        }

        writeln!(writer)
    }
}

const TARGET: &str = "srv";

/// Wait for a shutdown signal (`SIGINT` or `SIGTERM`).
///
/// On Unix the future resolves on whichever of `ctrl_c` / `SIGTERM`
/// arrives first.  On other platforms only `ctrl_c` is supported.
///
/// # Panics
///
/// Panics if the `SIGTERM` signal handler cannot be installed (Unix only).
#[allow(clippy::expect_used)] // Panic on signal handler failure is intentional.
pub async fn shutdown_signal() {
    #[cfg(unix)]
    let sigterm = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let sigterm = std::future::pending::<()>();

    tokio::select! {
        Ok(()) = tokio::signal::ctrl_c() => tracing::info!(target: TARGET, signal = %"SIGINT", "Shutdown signal"),
        () = sigterm => tracing::info!(target: TARGET, signal = %"SIGTERM", "Shutdown signal"),
    }
}

fn env_filter(default_filter: &str) -> tracing_subscriber::EnvFilter {
    tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| default_filter.into())
}

/// Initialize tracing with `CompactFormat` and `EnvFilter`.
///
/// Uses `RUST_LOG` if set, otherwise falls back to `default_filter`.
/// Output goes to stderr by default; pass a `log_file` path to write
/// to a file instead (useful for stdio-based protocols like MCP).
///
/// # Errors
///
/// Returns an error if the log file cannot be created.
pub fn init_tracing(default_filter: &str, log_file: Option<&Path>) -> std::io::Result<()> {
    if let Some(path) = log_file {
        let file = std::fs::File::create(path)?;
        tracing_subscriber::fmt()
            .with_ansi(false)
            .event_format(CompactFormat)
            .with_env_filter(env_filter(default_filter))
            .with_writer(file)
            .init();
    } else {
        tracing_subscriber::fmt()
            .event_format(CompactFormat)
            .with_env_filter(env_filter(default_filter))
            .init();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_tracing_fails_on_invalid_path() {
        let result = init_tracing("info", Some(Path::new("/nonexistent/dir/test.log")));
        assert!(result.is_err());
    }

    #[cfg(unix)]
    #[tokio::test]
    #[allow(unsafe_code)]
    async fn shutdown_signal_responds_to_sigterm() {
        use std::time::Duration;

        let handle = tokio::spawn(shutdown_signal());
        tokio::time::sleep(Duration::from_millis(10)).await;
        unsafe { libc::raise(libc::SIGTERM) };
        let result = tokio::time::timeout(Duration::from_secs(1), handle).await;
        assert!(result.is_ok(), "shutdown_signal should complete on SIGTERM");
    }

    #[cfg(unix)]
    #[tokio::test]
    #[allow(unsafe_code)]
    async fn shutdown_signal_responds_to_sigint() {
        use std::time::Duration;

        let handle = tokio::spawn(shutdown_signal());
        tokio::time::sleep(Duration::from_millis(10)).await;
        unsafe { libc::raise(libc::SIGINT) };
        let result = tokio::time::timeout(Duration::from_secs(1), handle).await;
        assert!(result.is_ok(), "shutdown_signal should complete on SIGINT");
    }
}
