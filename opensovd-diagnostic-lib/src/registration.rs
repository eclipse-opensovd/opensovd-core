// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Generic registration API for announcing this app to a SOVD server.
//!
//! Apps can use any IPC mechanism to register: REST, D-Bus, iceoryx2, SOME/IP, etc.
//! Implement [`AppRegistrar`] for your transport and pass it to
//! [`DiagnosticServer::with_registration`].
//!
//! The lib ships [`HttpRegistrar`] as the default REST-based implementation.

use async_trait::async_trait;

/// Information sent to the SOVD server when this app registers.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AppEndpoint {
    /// Unique app identifier (e.g. `"APP01"`)
    pub app_id: String,
    /// Human-readable name (e.g. `"APP01"`)
    pub app_name: String,
    /// Port on which the diagnostic HTTP server is listening
    pub port: u16,
    /// Component that hosts this app (e.g. `"HPC"`)
    pub hosted_on: String,
}

/// Trait for registering and deregistering this app with a SOVD server.
///
/// Any IPC mechanism can back this: REST, D-Bus, iceoryx2, SOME/IP, AUTOSAR COM, etc.
/// The lib calls [`register`] on startup with exponential backoff, and [`deregister`]
/// on clean shutdown.
#[async_trait]
pub trait AppRegistrar: Send + Sync {
    /// Announce this app endpoint to the SOVD server.
    ///
    /// Return `Ok(())` on success. Any error will trigger a retry.
    async fn register(&self, endpoint: &AppEndpoint) -> Result<(), String>;

    /// Remove this app from the SOVD server topology on clean shutdown.
    ///
    /// Default implementation is a no-op — override for transports that support it.
    async fn deregister(&self, endpoint: &AppEndpoint) -> Result<(), String> {
        let _ = endpoint;
        Ok(())
    }
}

/// REST-based registrar — POSTs [`AppEndpoint`] as JSON to register, DELETE to deregister.
pub struct HttpRegistrar {
    url: String,
}

impl HttpRegistrar {
    /// Create a registrar that POSTs to `url` on register and sends DELETE to
    /// `{url}/{app_id}` on deregister.
    pub fn new(url: &str) -> Self {
        Self { url: url.to_string() }
    }
}

#[async_trait]
impl AppRegistrar for HttpRegistrar {
    async fn register(&self, endpoint: &AppEndpoint) -> Result<(), String> {
        let client = reqwest::Client::new();
        let resp = client
            .post(&self.url)
            .json(endpoint)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        if resp.status().is_success() {
            Ok(())
        } else {
            Err(format!("HTTP {}", resp.status()))
        }
    }

    async fn deregister(&self, endpoint: &AppEndpoint) -> Result<(), String> {
        let url = format!("{}/{}", self.url, endpoint.app_id);
        let client = reqwest::Client::new();
        let resp = client
            .delete(&url)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        if resp.status().is_success() {
            Ok(())
        } else {
            Err(format!("HTTP {}", resp.status()))
        }
    }
}
