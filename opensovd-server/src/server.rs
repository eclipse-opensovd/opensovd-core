// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Server builder and listener types.

use std::future::Future;
use std::pin::Pin;

use axum::Router;
use futures::future::{FutureExt, Shared};
use futures::stream::StreamExt;
use opensovd_core::{DiscoveryProvider, EntityKind, Topology};
use serde::Serialize;
use thiserror::Error;
use tokio::net::TcpListener;
#[cfg(unix)]
use tokio::net::UnixListener;
use tower::Service as TowerService;
use tower::ServiceExt;
use tower::layer::util::{Identity, Stack};
use tower::util::BoxCloneSyncService;

use crate::auth::{
    AllowAll, AuthenticationLayer, Authenticator, AuthorizationLayer, Authorizer, NoAuth,
};
use crate::connect_info::ConnectInfo;
use crate::routes::VendorInfo;

pub(crate) type Service = BoxCloneSyncService<
    http::Request<axum::body::Body>,
    http::Response<axum::body::Body>,
    std::convert::Infallible,
>;

fn normalize_base(base: &str) -> Option<String> {
    match base.as_bytes() {
        b"" | b"/" => None,
        [b'/', ..] => Some(base.to_string()),
        _ => Some(format!("/{base}")),
    }
}

#[derive(Debug, Error)]
pub enum BuilderError {
    #[error("listener is required")]
    NoListener,
    #[error("invalid base URI")]
    InvalidUri,
}

pub enum Listener {
    Tcp(TcpListener),
    #[cfg(unix)]
    Unix(UnixListener),
}

impl Listener {
    /// Returns the local address and transport type
    ///
    /// # Errors
    ///
    /// Returns an error if the local address cannot be determined.
    pub fn local_addr(&self) -> std::io::Result<(String, &'static str)> {
        match self {
            Listener::Tcp(l) => Ok((l.local_addr()?.to_string(), "tcp")),
            #[cfg(unix)]
            Listener::Unix(l) => {
                let addr = l.local_addr()?;
                if let Some(path) = addr.as_pathname() {
                    Ok((path.display().to_string(), "unix"))
                } else {
                    #[cfg(target_os = "linux")]
                    if let Some(name) = addr.as_abstract_name() {
                        return Ok((String::from_utf8_lossy(name).to_string(), "abstract"));
                    }
                    Ok(("(unnamed)".to_string(), "unix"))
                }
            }
        }
    }
}

impl From<TcpListener> for Listener {
    fn from(listener: TcpListener) -> Self {
        Listener::Tcp(listener)
    }
}

#[cfg(unix)]
impl From<UnixListener> for Listener {
    fn from(listener: UnixListener) -> Self {
        Listener::Unix(listener)
    }
}

#[allow(clippy::too_many_arguments)]
fn build_router<Vendor, Authn, Authz, Layer>(
    base: Option<&str>,
    vendor_info: Option<Vendor>,
    authenticator: Authn,
    authorizer: Authz,
    topology: Topology,
    layer: Layer,
    services: Vec<(String, Service)>,
) -> Router
where
    Vendor: Serialize + Clone + Send + Sync + 'static,
    crate::routes::VersionInfo<Vendor>: crate::schema::JsonSchema,
    Authn: Authenticator,
    Authz: Authorizer<Authn::Identity>,
    Layer: tower::Layer<axum::routing::Route> + Clone + Send + Sync + 'static,
    Layer::Service: TowerService<http::Request<axum::body::Body>> + Clone + Send + Sync + 'static,
    <Layer::Service as TowerService<http::Request<axum::body::Body>>>::Response:
        axum::response::IntoResponse + 'static,
    <Layer::Service as TowerService<http::Request<axum::body::Body>>>::Error:
        Into<std::convert::Infallible> + 'static,
    <Layer::Service as TowerService<http::Request<axum::body::Body>>>::Future: Send + 'static,
{
    let inner = crate::routes::router(vendor_info, topology);
    let mut router = match base {
        Some(path) => Router::new().nest(path, inner),
        None => Router::new().merge(inner),
    };

    for (path, svc) in services {
        router = router.nest_service(&path, svc);
    }

    // Apply layers (order matters - last applied is outermost)
    // Request flow: UserLayer -> AuthN -> AuthZ -> Handler
    let router = router
        .layer(AuthorizationLayer::<Authz, Authn::Identity>::new(
            authorizer,
        ))
        .layer(AuthenticationLayer::new(authenticator));

    // User layers are outermost
    router.layer(layer)
}

type ShutdownFuture = Shared<Pin<Box<dyn Future<Output = ()> + Send>>>;

#[must_use]
pub struct ServerBuilder<Vendor = VendorInfo, Authn = NoAuth, Authz = AllowAll, Layer = Identity> {
    listener: Option<Listener>,
    base: String,
    shutdown: ShutdownFuture,
    vendor_info: Option<Vendor>,
    authenticator: Authn,
    authorizer: Authz,
    topology: Topology,
    discovery_providers: Vec<Box<dyn DiscoveryProvider>>,
    layer: Layer,
    services: Vec<(String, Service)>,
    #[cfg(feature = "tls")]
    tls_config: Option<crate::tls::TlsConfig>,
}

pub struct Server<Vendor = VendorInfo, Authn = NoAuth, Authz = AllowAll, Layer = Identity> {
    listener: Listener,
    base: String,
    shutdown: ShutdownFuture,
    vendor_info: Option<Vendor>,
    authenticator: Authn,
    authorizer: Authz,
    topology: Topology,
    discovery_providers: Vec<Box<dyn DiscoveryProvider>>,
    layer: Layer,
    services: Vec<(String, Service)>,
    #[cfg(feature = "tls")]
    tls_config: Option<crate::tls::TlsConfig>,
}

#[allow(clippy::expect_used)] // Panic on signal handler failure is intentional
async fn default_shutdown_signal() {
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
        Ok(()) = tokio::signal::ctrl_c() => tracing::info!(target: "srv", signal = %"SIGINT", "Shutdown signal"),
        () = sigterm => tracing::info!(target: "srv", signal = %"SIGTERM", "Shutdown signal"),
    }
}

impl ServerBuilder<VendorInfo, NoAuth, AllowAll, Identity> {
    fn new() -> Self {
        let shutdown: Pin<Box<dyn Future<Output = ()> + Send>> =
            Box::pin(default_shutdown_signal());
        Self {
            listener: None,
            base: String::new(),
            shutdown: shutdown.shared(),
            vendor_info: Some(VendorInfo {
                version: env!("CARGO_PKG_VERSION").into(),
                name: "OpenSOVD".into(),
            }),
            authenticator: NoAuth,
            authorizer: AllowAll,
            topology: Topology::default(),
            discovery_providers: Vec::new(),
            layer: Identity::new(),
            services: Vec::new(),
            #[cfg(feature = "tls")]
            tls_config: None,
        }
    }
}

impl<Vendor, Authn, Authz, Layer> ServerBuilder<Vendor, Authn, Authz, Layer> {
    pub fn listener(mut self, listener: impl Into<Listener>) -> Self {
        self.listener = Some(listener.into());
        self
    }

    /// Set the base URI path for all SOVD routes.
    ///
    /// # Errors
    ///
    /// Returns an error if the URI is malformed.
    pub fn base_uri(
        mut self,
        uri: impl TryInto<http::Uri>,
    ) -> std::result::Result<Self, BuilderError> {
        let uri: http::Uri = uri.try_into().map_err(|_| BuilderError::InvalidUri)?;
        self.base = uri.path().to_string();
        Ok(self)
    }

    pub fn shutdown<F>(mut self, signal: F) -> Self
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let shutdown: Pin<Box<dyn Future<Output = ()> + Send>> = Box::pin(signal);
        self.shutdown = shutdown.shared();
        self
    }

    pub fn vendor_info<NewVendor>(
        self,
        vendor_info: NewVendor,
    ) -> ServerBuilder<NewVendor, Authn, Authz, Layer> {
        ServerBuilder {
            listener: self.listener,
            base: self.base,
            shutdown: self.shutdown,
            vendor_info: Some(vendor_info),
            authenticator: self.authenticator,
            authorizer: self.authorizer,
            topology: self.topology,
            discovery_providers: self.discovery_providers,
            layer: self.layer,
            services: self.services,
            #[cfg(feature = "tls")]
            tls_config: self.tls_config,
        }
    }

    /// Set the authenticator for request authentication.
    pub fn authenticator<NewAuthn: Authenticator>(
        self,
        authenticator: NewAuthn,
    ) -> ServerBuilder<Vendor, NewAuthn, Authz, Layer> {
        ServerBuilder {
            listener: self.listener,
            base: self.base,
            shutdown: self.shutdown,
            vendor_info: self.vendor_info,
            authenticator,
            authorizer: self.authorizer,
            topology: self.topology,
            discovery_providers: self.discovery_providers,
            layer: self.layer,
            services: self.services,
            #[cfg(feature = "tls")]
            tls_config: self.tls_config,
        }
    }

    /// Set the authorizer for request authorization.
    pub fn authorizer<NewAuthz>(
        self,
        authorizer: NewAuthz,
    ) -> ServerBuilder<Vendor, Authn, NewAuthz, Layer>
    where
        NewAuthz: Authorizer<Authn::Identity>,
        Authn: Authenticator,
    {
        ServerBuilder {
            listener: self.listener,
            base: self.base,
            shutdown: self.shutdown,
            vendor_info: self.vendor_info,
            authenticator: self.authenticator,
            authorizer,
            topology: self.topology,
            discovery_providers: self.discovery_providers,
            layer: self.layer,
            services: self.services,
            #[cfg(feature = "tls")]
            tls_config: self.tls_config,
        }
    }

    /// Add a Tower layer to the server middleware stack.
    ///
    /// Layers are applied outside auth. The first layer added is the outermost.
    pub fn layer<NewLayer>(
        self,
        layer: NewLayer,
    ) -> ServerBuilder<Vendor, Authn, Authz, Stack<NewLayer, Layer>> {
        ServerBuilder {
            listener: self.listener,
            base: self.base,
            shutdown: self.shutdown,
            vendor_info: self.vendor_info,
            authenticator: self.authenticator,
            authorizer: self.authorizer,
            topology: self.topology,
            discovery_providers: self.discovery_providers,
            layer: Stack::new(layer, self.layer),
            services: self.services,
            #[cfg(feature = "tls")]
            tls_config: self.tls_config,
        }
    }

    pub fn topology(mut self, topology: Topology) -> Self {
        self.topology = topology;
        self
    }

    /// Register a tower service at the given path.
    pub fn service<S, ResBody>(mut self, path: &str, svc: S) -> Self
    where
        S: TowerService<
                http::Request<crate::Body>,
                Response = http::Response<ResBody>,
                Error = std::convert::Infallible,
            > + Clone
            + Send
            + Sync
            + 'static,
        S::Future: Send + 'static,
        ResBody: http_body::Body<Data = bytes::Bytes> + Send + 'static,
        ResBody::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    {
        let svc = svc
            .map_request(|req: http::Request<axum::body::Body>| req.map(crate::Body::wrap))
            .map_response(|resp: http::Response<ResBody>| resp.map(axum::body::Body::new));
        self.services
            .push((path.to_owned(), BoxCloneSyncService::new(svc)));
        self
    }

    /// Register a discovery provider for streaming entity discovery.
    ///
    /// Multiple providers can be registered; their streams are merged at
    /// runtime and events are applied to the shared topology.
    pub fn discovery(mut self, provider: Box<dyn DiscoveryProvider>) -> Self {
        self.discovery_providers.push(provider);
        self
    }

    // wrap the TCP listener with TLS.
    #[cfg(feature = "tls")]
    pub fn tls(mut self, config: crate::tls::TlsConfig) -> Self {
        self.tls_config = Some(config);
        self
    }

    /// Builds the server.
    ///
    /// # Errors
    ///
    /// Returns an error if the listener has not been set.
    pub fn build(self) -> Result<Server<Vendor, Authn, Authz, Layer>, BuilderError> {
        Ok(Server {
            listener: self.listener.ok_or(BuilderError::NoListener)?,
            base: self.base,
            shutdown: self.shutdown,
            vendor_info: self.vendor_info,
            authenticator: self.authenticator,
            authorizer: self.authorizer,
            topology: self.topology,
            discovery_providers: self.discovery_providers,
            layer: self.layer,
            services: self.services,
            #[cfg(feature = "tls")]
            tls_config: self.tls_config,
        })
    }
}

impl Server {
    pub fn builder() -> ServerBuilder {
        ServerBuilder::new()
    }
}

async fn run_discovery(
    topology: Topology,
    providers: Vec<Box<dyn DiscoveryProvider>>,
    shutdown: ShutdownFuture,
) {
    let mut streams = Vec::new();
    for provider in providers {
        match provider.discover().await {
            Ok(stream) => streams.push(stream),
            Err(e) => {
                tracing::error!(target: "discovery", error = %e, "Failed to start discovery provider");
            }
        }
    }
    if streams.is_empty() {
        return;
    }

    let mut merged = futures::stream::select_all(streams);
    let process_events = async {
        while let Some(event) = merged.next().await {
            match event {
                Ok((remove, add)) => {
                    let mut t = topology.write().await;
                    for r in &remove {
                        match r.kind() {
                            EntityKind::Component => t.remove_component(r.id()),
                            EntityKind::App => t.remove_app(r.id()),
                            EntityKind::Area => t.remove_area(r.id()),
                        }
                    }
                    for c in add.components {
                        t.add_component(c);
                    }
                    for a in add.apps {
                        t.add_app(a);
                    }
                    for a in add.areas {
                        t.add_area(a);
                    }
                }
                Err(e) => {
                    tracing::error!(target: "discovery", error = %e, "Discovery stream error");
                }
            }
        }
    };
    tokio::select! {
        () = process_events => {
            tracing::debug!(target: "discovery", "All discovery streams ended");
        }
        () = shutdown => {
            tracing::debug!(target: "discovery", "Shutting down");
        }
    }
}

impl<Vendor, Authn, Authz, Layer> Server<Vendor, Authn, Authz, Layer>
where
    Vendor: Serialize + Clone + Send + Sync + 'static,
    crate::routes::VersionInfo<Vendor>: crate::schema::JsonSchema,
    Authn: Authenticator,
    Authz: Authorizer<Authn::Identity>,
    Layer: tower::Layer<axum::routing::Route> + Clone + Send + Sync + 'static,
    Layer::Service: TowerService<http::Request<axum::body::Body>> + Clone + Send + Sync + 'static,
    <Layer::Service as TowerService<http::Request<axum::body::Body>>>::Response:
        axum::response::IntoResponse + 'static,
    <Layer::Service as TowerService<http::Request<axum::body::Body>>>::Error:
        Into<std::convert::Infallible> + 'static,
    <Layer::Service as TowerService<http::Request<axum::body::Body>>>::Future: Send + 'static,
{
    /// Starts the server.
    ///
    /// # Errors
    ///
    /// Returns an error if the server cannot be started.
    pub async fn serve(self) -> std::io::Result<()> {
        let base = normalize_base(&self.base);

        if !self.discovery_providers.is_empty() {
            let topology = self.topology.clone();
            let shutdown = self.shutdown.clone();
            tokio::spawn(run_discovery(topology, self.discovery_providers, shutdown));
        }

        let router = build_router(
            base.as_deref(),
            self.vendor_info,
            self.authenticator,
            self.authorizer,
            self.topology,
            self.layer,
            self.services,
        );

        let (addr, transport) = self.listener.local_addr()?;

        // if TLS is configured, override the transport label for logging
        #[cfg(feature = "tls")]
        let transport = match &self.tls_config {
            Some(cfg) if cfg.has_client_ca() => "mtls",
            Some(_) => "tls",
            None => transport,
        };

        tracing::info!(target: "srv", addr = %addr, r#type = %transport, base = %base.as_deref().unwrap_or("/"), "Listening");

        // TLS path: wrap the TCP listener and serve over TLS
        #[cfg(feature = "tls")]
        if let Some(tls_cfg) = self.tls_config {
            return match self.listener {
                Listener::Tcp(l) => {
                    let tls_listener = tls_cfg.build(l).map_err(std::io::Error::other)?;
                    axum::serve(
                        tls_listener,
                        router.into_make_service_with_connect_info::<ConnectInfo>(),
                    )
                    .with_graceful_shutdown(self.shutdown)
                    .await
                }
                #[cfg(unix)]
                _ => Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "TLS is only supported on TCP listeners",
                )),
            };
        }

        match self.listener {
            Listener::Tcp(l) => {
                axum::serve(
                    l,
                    router.into_make_service_with_connect_info::<ConnectInfo>(),
                )
                .with_graceful_shutdown(self.shutdown)
                .await
            }
            #[cfg(unix)]
            Listener::Unix(l) => {
                axum::serve(
                    l,
                    router.into_make_service_with_connect_info::<ConnectInfo>(),
                )
                .with_graceful_shutdown(self.shutdown)
                .await
            }
        }
    }
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
#[allow(clippy::string_slice)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_base() {
        assert_eq!(normalize_base(""), None);
        assert_eq!(normalize_base("/"), None);
        assert_eq!(normalize_base("/sovd"), Some("/sovd".to_string()));
        assert_eq!(normalize_base("sovd"), Some("/sovd".to_string()));
    }

    #[test]
    fn test_build_missing_listener() {
        let result = Server::builder().build();
        assert!(matches!(result, Err(BuilderError::NoListener)));
    }

    #[tokio::test]
    async fn test_immediate_shutdown() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();

        let server = Server::builder()
            .listener(listener)
            .base_uri("http://127.0.0.1:0/sovd")
            .unwrap()
            .shutdown(std::future::ready(()))
            .build()
            .unwrap();

        let result = server.serve().await;
        assert!(result.is_ok());
    }
}
