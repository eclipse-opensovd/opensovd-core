// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! HTTP server for exposing diagnostic data

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use warp::Filter;
use tracing::{debug, error, info};

use crate::{DataProvider, DiagnosticError};
use crate::data::{HealthResponse, ApiInfo, WriteDataRequest};
use crate::registration::{AppEndpoint, AppRegistrar};

/// Diagnostic HTTP server
///
/// Provides a standardized HTTP API for applications to expose their diagnostic data.
/// The server handles routing, error handling, and JSON serialization.
pub struct DiagnosticServer<P: DataProvider> {
    provider: Arc<P>,
    port: u16,
    registration: Option<(Box<dyn AppRegistrar>, AppEndpoint)>,
}

impl<P: DataProvider + 'static> DiagnosticServer<P> {
    /// Create a new diagnostic server
    /// 
    /// # Arguments
    /// * `provider` - The data provider implementation
    /// * `port` - The port to listen on
    pub fn new(provider: P, port: u16) -> Self {
        Self {
            provider: Arc::new(provider),
            port,
            registration: None,
        }
    }

    /// Configure self-registration with a SOVD server.
    ///
    /// On startup the lib will call `registrar.register(&endpoint)` and retry
    /// with exponential backoff until it succeeds. The `registrar` can use any
    /// IPC transport — pass [`HttpRegistrar`](crate::HttpRegistrar) for REST.
    pub fn with_registration(
        mut self,
        registrar: impl AppRegistrar + 'static,
        endpoint: AppEndpoint,
    ) -> Self {
        self.registration = Some((Box::new(registrar), endpoint));
        self
    }

    /// Start the HTTP server
    ///
    /// This will block until the server is stopped.
    pub async fn serve(self) -> Result<(), Box<dyn std::error::Error>> {
        let provider = self.provider.clone();

        // Spawn self-registration task before the server blocks
        if let Some((registrar, endpoint)) = self.registration {
            tokio::spawn(async move {
                let mut delay = Duration::from_millis(500);
                loop {
                    match registrar.register(&endpoint).await {
                        Ok(()) => {
                            info!(
                                "Registered '{}' with SOVD server (hosted on '{}')",
                                endpoint.app_id, endpoint.hosted_on
                            );
                            return;
                        }
                        Err(e) => {
                            debug!(
                                "Registration attempt failed ({}), retrying in {:?}",
                                e, delay
                            );
                            tokio::time::sleep(delay).await;
                            delay = (delay * 2).min(Duration::from_secs(30));
                        }
                    }
                }
            });
        }

        // Health check endpoint
        let health = warp::path!("health")
            .and(warp::get())
            .map(|| {
                let timestamp = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                warp::reply::json(&HealthResponse {
                    status: "healthy".to_string(),
                    timestamp,
                })
            });

        // API info endpoint
        let provider_info = provider.clone();
        let api_info = warp::path!("api" / "info")
            .and(warp::get())
            .and_then(move || {
                let provider = provider_info.clone();
                async move {
                    let app_info = provider.get_app_info().await;
                    Ok::<_, warp::Rejection>(warp::reply::json(&ApiInfo {
                        name: app_info.name,
                        version: app_info.version,
                        description: app_info.description,
                    }))
                }
            });

        // List all data items
        let provider_list = provider.clone();
        let list_data = warp::path!("api" / "data")
            .and(warp::get())
            .and_then(move || {
                let provider = provider_list.clone();
                async move {
                    let items = provider.list_data().await;
                    Ok::<_, warp::Rejection>(warp::reply::json(&items))
                }
            });

        // Read specific data item
        let provider_read = provider.clone();
        let read_data = warp::path!("api" / "data" / String)
            .and(warp::get())
            .and_then(move |data_id: String| {
                let provider = provider_read.clone();
                async move {
                    match provider.read_data(&data_id).await {
                        Ok(value) => Ok(warp::reply::json(&value)),
                        Err(e) => {
                            match &e {
                                crate::DiagnosticError::NotFound(_) => debug!("Data not found: {}", data_id),
                                _ => error!("Error reading data {}: {}", data_id, e),
                            }
                            Err(warp::reject::custom(e))
                        }
                    }
                }
            });

        // Write to specific data item
        let provider_write = provider.clone();
        let write_data = warp::path!("api" / "data" / String)
            .and(warp::put())
            .and(warp::body::json())
            .and_then(move |data_id: String, req: WriteDataRequest| {
                let provider = provider_write.clone();
                async move {
                    match provider.write_data(&data_id, req.value).await {
                        Ok(()) => {
                            info!("Successfully wrote data to {}", data_id);
                            Ok(warp::reply::json(&serde_json::json!({
                                "status": "success",
                                "message": format!("Data written to {}", data_id)
                            })))
                        }
                        Err(e) => {
                            match &e {
                                crate::DiagnosticError::NotFound(_) | crate::DiagnosticError::ReadOnly(_) => debug!("Write rejected for {}: {}", data_id, e),
                                _ => error!("Error writing data to {}: {}", data_id, e),
                            }
                            Err(warp::reject::custom(e))
                        }
                    }
                }
            });

        // SSE streaming endpoint
        #[derive(serde::Deserialize)]
        struct StreamQuery {
            data_ids: String,
            interval_ms: Option<u64>,
        }

        let provider_stream = provider.clone();
        let stream_endpoint = warp::path!("api" / "stream")
            .and(warp::get())
            .and(warp::query::<StreamQuery>())
            .and_then(move |query: StreamQuery| {
                let provider = provider_stream.clone();
                async move {
                    let data_ids: Vec<String> = query.data_ids
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();

                    let interval_ms = query.interval_ms.unwrap_or(100);

                    let tx = crate::streaming::create_periodic_stream(
                        data_ids,
                        interval_ms,
                        move |data_id| {
                            let provider = provider.clone();
                            async move {
                                provider.read_data(&data_id)
                                    .await
                                    .map(|v| v.value)
                                    .map_err(|e| e.to_string())
                            }
                        },
                    ).await;

                    let rx = tx.subscribe();
                    let sse_stream = crate::streaming::SseStream::new(rx);

                    Ok::<_, warp::Rejection>(warp::sse::reply(
                        warp::sse::keep_alive().stream(sse_stream)
                    ))
                }
            });

        // Combine all routes
        let routes = health
            .or(api_info)
            .or(list_data)
            .or(read_data)
            .or(write_data)
            .or(stream_endpoint)
            .recover(handle_rejection);

        info!("Diagnostic API listening on http://127.0.0.1:{}", self.port);
        info!("Endpoints:");
        info!("GET  /health - Health check");
        info!("GET  /api/info - Application info");
        info!("GET  /api/data - List all data items");
        info!("GET  /api/data/{{id}} - Read data item");
        info!("PUT  /api/data/{{id}} - Write data item");
        info!("GET  /api/stream?data_ids=id1,id2&interval_ms=100 - Stream data (SSE)");

        warp::serve(routes)
            .run(([127, 0, 0, 1], self.port))
            .await;

        Ok(())
    }
}

/// Handle rejections and convert to HTTP responses
async fn handle_rejection(err: warp::Rejection) -> Result<impl warp::Reply, warp::Rejection> {
    if let Some(e) = err.find::<DiagnosticError>() {
        let (code, message) = match e {
            DiagnosticError::NotFound(msg) => (404, msg.clone()),
            DiagnosticError::ReadOnly(msg) => (403, msg.clone()),
            DiagnosticError::InvalidData(msg) => (400, msg.clone()),
            DiagnosticError::Internal(msg) => (500, msg.clone()),
            DiagnosticError::ServerError(msg) => (500, msg.clone()),
            DiagnosticError::SerializationError(e) => (400, e.to_string()),
        };

        let json = warp::reply::json(&serde_json::json!({
            "error": message,
            "code": code
        }));

        Ok(warp::reply::with_status(json, warp::http::StatusCode::from_u16(code).unwrap()))
    } else {
        Err(err)
    }
}