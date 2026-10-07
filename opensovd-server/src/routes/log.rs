// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0
//
// This file was created with the assistance of generative AI.

//! SOVD logging resource endpoints.

use axum::{
    Json, Router,
    extract::{Path, State},
    response::sse::{Event, KeepAlive, Sse},
    routing::get,
};
use axum_extra::extract::{Query, WithRejection};
use futures::StreamExt;
use http::{StatusCode, request::Parts};
use opensovd_core::{
    LogConfiguration as CoreLogConfiguration, LogContext as CoreLogContext,
    LogEntry as CoreLogEntry, LogFilter, LogProvider, LogSeverity as CoreLogSeverity, Topology,
};
use opensovd_models::{
    Response,
    log::{
        EventEnvelope, LogConfiguration, LogConfigurationQuery, LogConfigurationRequest,
        LogConfigurationResponse, LogContext, LogEntries, LogEntriesQuery, LogEntry, LogResources,
        LogResourcesQuery, LogSeverity, LogStreamQuery,
    },
};
use serde_json::{Map, Value};

use super::{
    AppState,
    entities::encode_path_segment,
    error::{Error, Result},
    versioned_uri,
};
use crate::schema::JsonSchema;

pub fn routes<V>() -> Router<AppState<V>>
where
    V: Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/{entity_collection}/{entity_id}/logs", get(log_resources))
        .route(
            "/{entity_collection}/{entity_id}/logs/entries",
            get(log_entries),
        )
        .route(
            "/{entity_collection}/{entity_id}/logs/entries/stream",
            get(log_entry_stream),
        )
        .route(
            "/{entity_collection}/{entity_id}/logs/config",
            get(log_configuration)
                .put(update_log_configuration)
                .delete(reset_log_configuration),
        )
}

async fn log_resources(
    State(topology): State<Topology>,
    Path((entity_collection, entity_id)): Path<(String, String)>,
    WithRejection(Query(query), _): WithRejection<Query<LogResourcesQuery>, Error>,
    parts: Parts,
) -> Result<Json<Response<LogResources>>> {
    get_provider(&topology, &entity_collection, &entity_id).await?;
    let base = versioned_uri(&parts);
    Ok(Json(Response {
        data: LogResources {
            entries: format!(
                "{base}/{entity_collection}/{}/logs/entries",
                encode_path_segment(&entity_id)
            )
            .into(),
            config: format!(
                "{base}/{entity_collection}/{}/logs/config",
                encode_path_segment(&entity_id)
            )
            .into(),
            live_entries: Some(
                format!(
                    "{base}/{entity_collection}/{}/logs/entries/stream",
                    encode_path_segment(&entity_id)
                )
                .into(),
            ),
        },
        schema: query.include_schema.then(LogResources::schema),
    }))
}

async fn log_entries(
    State(topology): State<Topology>,
    Path((entity_collection, entity_id)): Path<(String, String)>,
    WithRejection(Query(query), _): WithRejection<Query<LogEntriesQuery>, Error>,
) -> Result<Json<Response<LogEntries>>> {
    let provider = get_provider(&topology, &entity_collection, &entity_id).await?;
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

    Ok(Json(Response {
        data: LogEntries { items: entries },
        schema: include_schema.then(LogEntries::schema),
    }))
}

async fn log_entry_stream(
    State(topology): State<Topology>,
    Path((entity_collection, entity_id)): Path<(String, String)>,
    WithRejection(Query(query), _): WithRejection<Query<LogStreamQuery>, Error>,
) -> Result<
    Sse<impl futures_core::Stream<Item = std::result::Result<Event, std::convert::Infallible>>>,
> {
    let provider = get_provider(&topology, &entity_collection, &entity_id).await?;
    let stream = provider
        .stream(LogFilter {
            severity: query.severity.map(core_severity),
            created_after: query.created_after,
            created_before: query.created_before,
        })
        .await?;

    let events = stream.filter_map(|item| async move {
        let Ok(entry) = item else {
            return None;
        };
        let timestamp = entry.timestamp;
        let envelope = EventEnvelope {
            timestamp,
            payload: Some(model_entry(entry)),
            error: None,
        };
        Some(Ok(Event::default().data(
            serde_json::to_string(&envelope).unwrap_or_else(|_| {
                "{\"error\":{\"message\":\"failed to encode log event\"}}".into()
            }),
        )))
    });

    Ok(Sse::new(events).keep_alive(KeepAlive::default()))
}

async fn log_configuration(
    State(topology): State<Topology>,
    Path((entity_collection, entity_id)): Path<(String, String)>,
    WithRejection(Query(query), _): WithRejection<Query<LogConfigurationQuery>, Error>,
) -> Result<Json<Response<LogConfigurationResponse>>> {
    let provider = get_provider(&topology, &entity_collection, &entity_id).await?;
    let contexts = provider
        .configuration()
        .await?
        .into_iter()
        .map(model_configuration)
        .collect();
    Ok(Json(Response {
        data: LogConfigurationResponse { contexts },
        schema: query.include_schema.then(LogConfigurationResponse::schema),
    }))
}

async fn update_log_configuration(
    State(topology): State<Topology>,
    Path((entity_collection, entity_id)): Path<(String, String)>,
    WithRejection(Json(body), _): WithRejection<Json<LogConfigurationRequest>, Error>,
) -> Result<StatusCode> {
    let provider = get_provider(&topology, &entity_collection, &entity_id).await?;
    provider
        .configure(body.items.into_iter().map(core_configuration).collect())
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn reset_log_configuration(
    State(topology): State<Topology>,
    Path((entity_collection, entity_id)): Path<(String, String)>,
) -> Result<StatusCode> {
    let provider = get_provider(&topology, &entity_collection, &entity_id).await?;
    provider.reset_configuration().await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_provider(
    topology: &Topology,
    entity_collection: &str,
    entity_id: &str,
) -> Result<std::sync::Arc<dyn LogProvider>> {
    let topo = topology.read().await;
    let provider = match entity_collection {
        "apps" => topo
            .get_app(entity_id)
            .map_err(|_| Error::EntityNotFound(entity_id.to_string()))?
            .log_provider(),
        "components" => topo
            .get_component(entity_id)
            .map_err(|_| Error::EntityNotFound(entity_id.to_string()))?
            .log_provider(),
        _ => return Err(Error::EntityNotFound(entity_collection.to_string())),
    };
    provider.ok_or_else(|| Error::ProviderNotAvailable("logs".into()))
}

fn core_severity(value: LogSeverity) -> CoreLogSeverity {
    match value {
        LogSeverity::Fatal => CoreLogSeverity::Fatal,
        LogSeverity::Error => CoreLogSeverity::Error,
        LogSeverity::Warn => CoreLogSeverity::Warn,
        LogSeverity::Info => CoreLogSeverity::Info,
        LogSeverity::Debug => CoreLogSeverity::Debug,
        LogSeverity::DltFatal => CoreLogSeverity::DltFatal,
        LogSeverity::DltError => CoreLogSeverity::DltError,
        LogSeverity::DltWarn => CoreLogSeverity::DltWarn,
        LogSeverity::DltInfo => CoreLogSeverity::DltInfo,
        LogSeverity::DltDebug => CoreLogSeverity::DltDebug,
    }
}

fn model_severity(value: CoreLogSeverity) -> LogSeverity {
    match value {
        CoreLogSeverity::Fatal => LogSeverity::Fatal,
        CoreLogSeverity::Error => LogSeverity::Error,
        CoreLogSeverity::Warn => LogSeverity::Warn,
        CoreLogSeverity::Info => LogSeverity::Info,
        CoreLogSeverity::Debug => LogSeverity::Debug,
        CoreLogSeverity::DltFatal => LogSeverity::DltFatal,
        CoreLogSeverity::DltError => LogSeverity::DltError,
        CoreLogSeverity::DltWarn => LogSeverity::DltWarn,
        CoreLogSeverity::DltInfo => LogSeverity::DltInfo,
        CoreLogSeverity::DltDebug => LogSeverity::DltDebug,
    }
}

fn core_context(value: LogContext) -> CoreLogContext {
    let LogContext {
        context_type,
        mut attributes,
    } = value;
    match context_type.as_str() {
        "RFC5424" => CoreLogContext::Rfc5424 {
            host: take_string(&mut attributes, "host"),
            process: take_string(&mut attributes, "process"),
            pid: take_i64(&mut attributes, "pid"),
        },
        "AUTOSAR_DLT" => CoreLogContext::AutosarDlt {
            session: take_string(&mut attributes, "session"),
            session_id: take_string(&mut attributes, "session_id"),
            application_id: take_string(&mut attributes, "application_id"),
            context_id: take_string(&mut attributes, "context_id"),
            message_id: take_string(&mut attributes, "message_id"),
        },
        _ => CoreLogContext::Custom {
            context_type,
            attributes,
        },
    }
}

fn model_context(value: CoreLogContext) -> LogContext {
    match value {
        CoreLogContext::Rfc5424 { host, process, pid } => LogContext {
            context_type: "RFC5424".into(),
            attributes: attributes([
                ("host", host.map(Value::from)),
                ("process", process.map(Value::from)),
                ("pid", pid.map(Value::from)),
            ]),
        },
        CoreLogContext::AutosarDlt {
            session,
            session_id,
            application_id,
            context_id,
            message_id,
        } => LogContext {
            context_type: "AUTOSAR_DLT".into(),
            attributes: attributes([
                ("session", session.map(Value::from)),
                ("session_id", session_id.map(Value::from)),
                ("application_id", application_id.map(Value::from)),
                ("context_id", context_id.map(Value::from)),
                ("message_id", message_id.map(Value::from)),
            ]),
        },
        CoreLogContext::Custom {
            context_type,
            attributes,
        } => LogContext {
            context_type,
            attributes,
        },
    }
}

fn attributes(
    items: impl IntoIterator<Item = (&'static str, Option<Value>)>,
) -> Map<String, Value> {
    items
        .into_iter()
        .filter_map(|(key, value)| value.map(|value| (key.to_string(), value)))
        .collect()
}

fn take_string(attributes: &mut Map<String, Value>, key: &str) -> Option<String> {
    attributes
        .remove(key)
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
}

fn take_i64(attributes: &mut Map<String, Value>, key: &str) -> Option<i64> {
    attributes.remove(key).and_then(|value| value.as_i64())
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
