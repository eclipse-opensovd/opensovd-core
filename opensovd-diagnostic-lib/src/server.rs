// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! HTTP server for exposing diagnostic data

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tracing::{debug, error, info};
use warp::Filter;

use crate::data::{ApiInfo, HealthResponse, WriteDataRequest};
use crate::registration::{AppEndpoint, AppRegistrar};
use crate::{DataProvider, DiagnosticError};

/// Waits for SIGINT or SIGTERM.
async fn shutdown_signal() {
    #[cfg(unix)]
    let sigterm = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let sigterm = std::future::pending::<()>();

    tokio::select! {
        Ok(()) = tokio::signal::ctrl_c() => {},
        () = sigterm => {},
    }
}

/// Diagnostic HTTP server
///
/// Provides a standardized HTTP API for applications to expose their diagnostic data.
/// The server handles routing, error handling, and JSON serialization.
pub struct DiagnosticServer<P: DataProvider> {
    provider: Arc<P>,
    port: u16,
    registration: Option<(Arc<dyn AppRegistrar>, AppEndpoint)>,
    heartbeat_interval: Option<Duration>,
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
            heartbeat_interval: None,
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
        self.registration = Some((Arc::new(registrar), endpoint));
        self
    }

    /// Send a registration heartbeat to the SOVD server every `interval`.
    ///
    /// Requires [`with_registration`](Self::with_registration) to be configured.
    /// The heartbeat re-sends the registration so the server can detect stale apps via TTL.
    pub fn with_heartbeat(mut self, interval: Duration) -> Self {
        self.heartbeat_interval = Some(interval);
        self
    }

    /// Start the HTTP server.
    ///
    /// Blocks until a shutdown signal (SIGINT or SIGTERM) is received, then
    /// drains in-flight requests, calls `deregister` if configured, and returns.
    pub async fn serve(self) -> Result<(), Box<dyn std::error::Error>> {
        let provider = self.provider.clone();

        // Spawn registration, optional heartbeat, and handle graceful shutdown
        let heartbeat_interval = self.heartbeat_interval;
        let deregister = self
            .registration
            .as_ref()
            .map(|(r, ep)| (Arc::clone(r), ep.clone()));
        if let Some((registrar, endpoint)) = self.registration {
            // Initial registration with exponential backoff
            let reg = Arc::clone(&registrar);
            let ep = endpoint.clone();
            tokio::spawn(async move {
                let mut delay = Duration::from_millis(500);
                loop {
                    match reg.register(&ep).await {
                        Ok(()) => {
                            info!(
                                "Registered '{}' with SOVD server (hosted on '{}')",
                                ep.app_id, ep.hosted_on
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

            // Heartbeat task — re-registers on the configured interval
            if let Some(interval) = heartbeat_interval {
                let reg = Arc::clone(&registrar);
                let ep = endpoint.clone();
                tokio::spawn(async move {
                    loop {
                        tokio::time::sleep(interval).await;
                        if let Err(e) = reg.register(&ep).await {
                            debug!("Heartbeat failed: {}", e);
                        }
                    }
                });
            }
        }

        // Health check endpoint
        let health = warp::path!("health").and(warp::get()).map(|| {
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
                                crate::DiagnosticError::NotFound(_) => {
                                    debug!("Data not found: {}", data_id)
                                }
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
                                crate::DiagnosticError::NotFound(_)
                                | crate::DiagnosticError::ReadOnly(_) => {
                                    debug!("Write rejected for {}: {}", data_id, e)
                                }
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
                    let data_ids: Vec<String> = query
                        .data_ids
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
                                provider
                                    .read_data(&data_id)
                                    .await
                                    .map(|v| v.value)
                                    .map_err(|e| e.to_string())
                            }
                        },
                    )
                    .await;

                    let rx = tx.subscribe();
                    let sse_stream = crate::streaming::SseStream::new(rx);

                    Ok::<_, warp::Rejection>(warp::sse::reply(
                        warp::sse::keep_alive().stream(sse_stream),
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
        info!("GET  /health");
        info!("GET  /api/info");
        info!("GET  /api/data");
        info!("GET  /api/data/{{id}}");
        info!("PUT  /api/data/{{id}}");
        info!("GET  /api/stream?data_ids=id1,id2&interval_ms=100");

        let (_, server) =
            warp::serve(routes).bind_with_graceful_shutdown(([127, 0, 0, 1], self.port), async {
                shutdown_signal().await;
            });

        server.await;

        // Deregister after the server has stopped accepting connections
        if let Some((registrar, endpoint)) = deregister {
            info!("Shutting down — deregistering '{}'", endpoint.app_id);
            if let Err(e) = registrar.deregister(&endpoint).await {
                debug!("Deregistration failed: {}", e);
            }
        }

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

        Ok(warp::reply::with_status(
            json,
            warp::http::StatusCode::from_u16(code).unwrap(),
        ))
    } else {
        Err(err)
    }
}
