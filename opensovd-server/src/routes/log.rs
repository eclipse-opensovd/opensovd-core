// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0
//
// AI Disclosure: This file was created with the assistance of OpenAI Codex.
// Assisted-by: OpenAI Codex

//! Log resource endpoints.
//!
//! Provides routes below the path of an entity, `{entity_path}`, which is
//! empty for the server:
//! - GET /{entity_path}/logs - Retrieve log resources
//! - GET /{entity_path}/logs/entries - Read log entries
//! - GET /{entity_path}/logs/x-opensovd-log-streams - Stream log entries
//! - GET /{entity_path}/logs/config - Retrieve the log configuration
//! - PUT /{entity_path}/logs/config - Configure the log
//! - DELETE /{entity_path}/logs/config - Reset the log configuration to default

use axum::{
    Json, Router,
    extract::State,
    response::sse::{Event, KeepAlive, Sse},
    routing::get,
};
use axum_extra::extract::{Query, WithRejection};
use futures::StreamExt;
use http::{StatusCode, request::Parts};
use opensovd_core::{
    LogConfiguration as CoreLogConfiguration, LogContext as CoreLogContext,
    LogEntry as CoreLogEntry, LogError, LogFilter, LogSeverity as CoreLogSeverity, Topology,
};
use opensovd_models::{
    error::{ErrorCode, GenericError},
    log::{
        EventEnvelope, LogConfiguration, LogConfigurationQuery, LogConfigurationRequest,
        LogConfigurationResponse, LogContext, LogEntries, LogEntriesQuery, LogEntry, LogResources,
        LogResourcesQuery, LogSeverity, LogStreamQuery,
    },
};

use super::{
    entity::Entity,
    error::{Error, Result},
    versioned_uri,
};
use crate::schema::JsonSchema;

pub fn routes() -> Router<Topology> {
    Router::new()
        .route("/logs", get(log_resources))
        .route("/logs/entries", get(log_entries))
        .route("/logs/x-opensovd-log-streams", get(log_entry_stream))
        .route(
            "/logs/config",
            get(log_configuration)
                .put(log_configuration_write)
                .delete(log_configuration_reset),
        )
}

/// GET /{entity_path}/logs - Retrieve log resources.
///
/// Returns the links to the entries and the configuration of an entity, and
/// to the live stream if the provider supports it.
async fn log_resources(
    State(topology): State<Topology>,
    entity: Entity,
    parts: Parts,
    WithRejection(Query(query), _): WithRejection<Query<LogResourcesQuery>, Error>,
) -> Result<Json<LogResources>> {
    let provider = entity.log_provider(&topology.read().await)?;
    let logs = format!("{}/{}/logs", versioned_uri(&parts), entity.path());
    let stream = provider
        .supports_streaming()
        .then(|| format!("{logs}/x-opensovd-log-streams").into());
    Ok(Json(LogResources {
        entries: format!("{logs}/entries").into(),
        config: format!("{logs}/config").into(),
        stream,
        schema: query.include_schema.then(LogResources::schema),
    }))
}

/// GET /{entity_path}/logs/entries - Read log entries.
///
/// Returns the log entries of an entity, optionally filtered by severity or
/// creation time.
async fn log_entries(
    State(topology): State<Topology>,
    entity: Entity,
    WithRejection(Query(query), _): WithRejection<Query<LogEntriesQuery>, Error>,
) -> Result<Json<LogEntries>> {
    validate_entries_filter(&query)?;
    let provider = entity.log_provider(&topology.read().await)?;
    let include_schema = query.include_schema;
    let entries = provider
        .entries(LogFilter {
            severity: query.severity.map(core_severity),
            created_after: query.created_after,
            created_before: query.created_before,
        })
        .await?
        .into_iter()
        .map(model_entry)
        .collect();

    Ok(Json(LogEntries {
        items: entries,
        schema: include_schema.then(LogEntries::schema),
    }))
}

/// GET /{entity_path}/logs/x-opensovd-log-streams - Stream log entries.
///
/// Sends the log entries as server-sent events, filtered like the entries.
async fn log_entry_stream(
    State(topology): State<Topology>,
    entity: Entity,
    WithRejection(Query(query), _): WithRejection<Query<LogStreamQuery>, Error>,
) -> Result<
    Sse<impl futures_core::Stream<Item = std::result::Result<Event, std::convert::Infallible>>>,
> {
    validate_stream_filter(&query)?;
    let provider = entity.log_provider(&topology.read().await)?;
    let stream = provider
        .stream(LogFilter {
            severity: query.severity.map(core_severity),
            created_after: query.created_after,
            created_before: query.created_before,
        })
        .await?;

    let events = stream.map(|item| {
        let envelope = match item {
            Ok(entry) => EventEnvelope {
                timestamp: entry.timestamp,
                payload: Some(model_entry(entry)),
                error: None,
            },
            Err(error) => EventEnvelope {
                // Provider errors do not carry an entry timestamp. The epoch
                // makes that absence deterministic and keeps the envelope valid.
                timestamp: chrono::DateTime::<chrono::Utc>::UNIX_EPOCH,
                payload: None,
                error: Some(stream_error(&error)),
            },
        };
        Ok(Event::default().data(
            serde_json::to_string(&envelope).unwrap_or_else(|_| {
                "{\"error\":{\"message\":\"failed to encode log event\"}}".into()
            }),
        ))
    });

    Ok(Sse::new(events).keep_alive(KeepAlive::default()))
}

fn stream_error(error: &LogError) -> GenericError {
    let (error_code, message) = match error {
        LogError::InvalidRequest(_) => (
            ErrorCode::ErrorResponse,
            "the log provider rejected a stream item",
        ),
        LogError::NotFound(_) => (
            ErrorCode::ErrorResponse,
            "the log stream resource was not found",
        ),
        LogError::NotSupported(_) => (
            ErrorCode::ErrorResponse,
            "the log provider does not support live streaming",
        ),
        LogError::Internal(_) => (
            ErrorCode::SovdServerFailure,
            "the log provider failed to produce a stream item",
        ),
    };
    GenericError::new(error_code, message)
}

/// GET /{entity_path}/logs/config - Retrieve the log configuration.
///
/// Returns the severity configured for each log context of an entity.
async fn log_configuration(
    State(topology): State<Topology>,
    entity: Entity,
    WithRejection(Query(query), _): WithRejection<Query<LogConfigurationQuery>, Error>,
) -> Result<Json<LogConfigurationResponse>> {
    let provider = entity.log_provider(&topology.read().await)?;
    let contexts = provider
        .configuration()
        .await?
        .into_iter()
        .map(model_configuration)
        .collect();
    Ok(Json(LogConfigurationResponse {
        contexts,
        schema: query.include_schema.then(LogConfigurationResponse::schema),
    }))
}

/// PUT /{entity_path}/logs/config - Configure the log.
///
/// Sets the severity of the given log contexts.
async fn log_configuration_write(
    State(topology): State<Topology>,
    entity: Entity,
    WithRejection(Json(body), _): WithRejection<Json<LogConfigurationRequest>, Error>,
) -> Result<StatusCode> {
    let provider = entity.log_provider(&topology.read().await)?;
    provider
        .configure(body.items.into_iter().map(core_configuration).collect())
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// DELETE /{entity_path}/logs/config - Reset the log configuration to default.
///
/// Restores the default configuration of the provider.
async fn log_configuration_reset(
    State(topology): State<Topology>,
    entity: Entity,
) -> Result<StatusCode> {
    let provider = entity.log_provider(&topology.read().await)?;
    provider.reset_configuration().await?;
    Ok(StatusCode::NO_CONTENT)
}

fn core_severity(value: LogSeverity) -> CoreLogSeverity {
    match value {
        LogSeverity::Fatal => CoreLogSeverity::Fatal,
        LogSeverity::Error => CoreLogSeverity::Error,
        LogSeverity::Warn => CoreLogSeverity::Warn,
        LogSeverity::Info => CoreLogSeverity::Info,
        LogSeverity::Debug => CoreLogSeverity::Debug,
    }
}

fn model_severity(value: CoreLogSeverity) -> LogSeverity {
    match value {
        CoreLogSeverity::Fatal => LogSeverity::Fatal,
        CoreLogSeverity::Error => LogSeverity::Error,
        CoreLogSeverity::Warn => LogSeverity::Warn,
        CoreLogSeverity::Info => LogSeverity::Info,
        CoreLogSeverity::Debug => LogSeverity::Debug,
    }
}

fn core_context(value: LogContext) -> CoreLogContext {
    CoreLogContext {
        context_type: value.context_type,
        attributes: value.attributes,
    }
}

fn model_context(value: CoreLogContext) -> LogContext {
    LogContext {
        context_type: value.context_type,
        attributes: value.attributes,
    }
}

fn validate_entries_filter(query: &LogEntriesQuery) -> Result<()> {
    validate_time_range(query.created_after, query.created_before)
}

fn validate_stream_filter(query: &LogStreamQuery) -> Result<()> {
    validate_time_range(query.created_after, query.created_before)
}

fn validate_time_range(
    created_after: Option<chrono::DateTime<chrono::Utc>>,
    created_before: Option<chrono::DateTime<chrono::Utc>>,
) -> Result<()> {
    if let (Some(after), Some(before)) = (created_after, created_before)
        && after >= before
    {
        return Err(LogError::InvalidRequest(
            "created-after must be earlier than created-before".into(),
        )
        .into());
    }
    Ok(())
}

fn core_configuration(value: LogConfiguration) -> CoreLogConfiguration {
    CoreLogConfiguration {
        context: core_context(value.context),
        severity: core_severity(value.severity),
    }
}

fn model_configuration(value: CoreLogConfiguration) -> LogConfiguration {
    LogConfiguration {
        context: model_context(value.context),
        severity: model_severity(value.severity),
    }
}

fn model_entry(value: CoreLogEntry) -> LogEntry {
    LogEntry {
        timestamp: value.timestamp,
        context: model_context(value.context),
        severity: model_severity(value.severity),
        msg: value.msg,
        href: value.href.map(Into::into),
    }
}
