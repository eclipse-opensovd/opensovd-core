// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Reusable OpenSOVD gateway runner.
//!
//! An embedding application constructs a [`Topology`] with its own providers,
//! parses the ordinary gateway options, and runs the same gateway as the binary:
//!
//! ```no_run
//! # async fn example() -> anyhow::Result<()> {
//! let topology = opensovd_core::Topology::default();
//! // Register components, apps and providers in the topology here.
//! let options = opensovd_gateway::cli::try_parse_from(std::env::args_os())?;
//! opensovd_gateway::run(options, topology).await
//! # }
//! ```
//!
//! The gateway's `SOVD_*` environment variables and command-line options apply
//! equally to an embedded gateway. The caller owns tracing initialization.
//! [`run_with_shutdown`] permits the host to control graceful shutdown.
//!
//! The gateway builds its authentication from `--auth-*`. An application with
//! its own [`Auth`] passes it to [`run_with_auth`] or
//! [`run_with_auth_and_shutdown`] instead, and may not combine it with `--auth-jwks`.

pub mod cli;
mod upstream;

use opensovd_core::Topology;
use opensovd_extra::{Federation, FederationLayer};
#[cfg(feature = "mock")]
use opensovd_mocks::create_mock_topology;
use opensovd_server::{Auth, Server};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
struct OpenSovdInfo {
    version: &'static str,
    sha1: &'static str,
    build_date: &'static str,
    name: &'static str,
}

const TARGET: &str = "gw";

const VENDOR_INFO: OpenSovdInfo = OpenSovdInfo {
    version: env!("VERSION"),
    sha1: env!("COMMIT_SHA"),
    build_date: env!("BUILD_DATE"),
    name: "OpenSOVD",
};

/// Runs the gateway with its usual empty or mock topology and signal handling.
///
/// # Errors
///
/// Returns an error if configuration, startup or serving fails.
pub async fn run_default(cli: cli::Cli) -> anyhow::Result<()> {
    let topology = configure_topology(&cli).await;
    serve(cli, None, topology, libcli::shutdown_signal()).await
}

/// Runs the gateway with a caller-provided topology and signal handling.
///
/// The caller initializes tracing if required. `--mock` cannot be combined
/// with a supplied topology; use [`run_default`] for the mock topology.
///
/// # Errors
///
/// Returns an error if configuration, startup or serving fails.
pub async fn run(cli: cli::Cli, topology: Topology) -> anyhow::Result<()> {
    run_with_shutdown(cli, topology, libcli::shutdown_signal()).await
}

/// Runs the gateway until `shutdown` resolves, using a caller-provided topology.
///
/// On shutdown the server stops gracefully and any mDNS announcement is withdrawn.
/// The caller initializes tracing if required.
///
/// # Errors
///
/// Returns an error if `--mock` is selected, or configuration, startup or serving fails.
pub async fn run_with_shutdown(
    cli: cli::Cli,
    topology: Topology,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> anyhow::Result<()> {
    #[cfg(feature = "mock")]
    anyhow::ensure!(!cli.mock, "--mock cannot be used with a supplied topology");
    serve(cli, None, topology, shutdown).await
}

/// Runs the gateway with a caller-provided topology and [`Auth`], and signal handling.
///
/// The `--auth-*` options do not build the authentication here; see
/// [`run_with_auth_and_shutdown`].
///
/// # Errors
///
/// Returns an error if configuration, startup or serving fails.
pub async fn run_with_auth(cli: cli::Cli, topology: Topology, auth: Auth) -> anyhow::Result<()> {
    run_with_auth_and_shutdown(cli, topology, auth, libcli::shutdown_signal()).await
}

/// Runs the gateway with a caller-provided topology and [`Auth`] until `shutdown` resolves.
///
/// For an application that authenticates and authorizes requests itself. The
/// caller initializes tracing if required.
///
/// # Errors
///
/// Returns an error if `--mock` or `--auth-jwks` is selected, as the supplied
/// topology and `auth` replace them, or if configuration, startup or serving fails.
pub async fn run_with_auth_and_shutdown(
    cli: cli::Cli,
    topology: Topology,
    auth: Auth,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> anyhow::Result<()> {
    #[cfg(feature = "mock")]
    anyhow::ensure!(!cli.mock, "--mock cannot be used with a supplied topology");
    anyhow::ensure!(
        !cli.server.auth_enabled(),
        "--auth-jwks cannot be used with supplied auth"
    );
    serve(cli, Some(auth), topology, shutdown).await
}

/// Serves the gateway, with `auth` if supplied, or else the auth of `--auth-*`.
async fn serve(
    cli: cli::Cli,
    auth: Option<Auth>,
    topology: Topology,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> anyhow::Result<()> {
    tracing::info!(
        target: TARGET,
        version = %VENDOR_INFO.version,
        channel = %env!("RELEASE_CHANNEL"),
        sha1 = %VENDOR_INFO.sha1,
        build_date = %VENDOR_INFO.build_date,
        "{}", cli::ABOUT);
    let args = &cli.server;

    let mut builder = Server::builder();
    let auth = match auth {
        Some(auth) => Some(auth),
        None => args.auth()?,
    };
    if let Some(auth) = auth {
        builder = builder.auth(auth);
    }
    #[cfg(feature = "tls")]
    if let Some(tls) = args.tls()? {
        builder = builder.tls(tls);
    }

    let listener = args.listen().await?;
    #[cfg(feature = "mdns")]
    let mdns = args.mdns(&listener)?;
    builder = builder.listener(listener);
    builder = builder.topology(topology.clone());
    // after auth, so that forwarded requests are authorized like any other
    if let Some(federation) = create_federation(&cli.upstream, topology)? {
        builder = builder.inner_layer(federation);
    }

    let cors = args.cors();
    if cors.is_some() {
        tracing::info!(target: TARGET, "CORS enabled");
    }

    if let Some((path, dir)) = args.serve_dir()? {
        builder = builder.service(path, dir);
    }

    #[cfg(feature = "mdns")]
    let (announcer_tx, announcer_rx) =
        tokio::sync::oneshot::channel::<Option<opensovd_extra::MdnsAnnouncer>>();
    builder = builder.shutdown(async move {
        shutdown.await;
        #[cfg(feature = "mdns")]
        if let Ok(Some(announcer)) = announcer_rx.await {
            announcer.shutdown().await;
        }
    });

    let server = builder
        .layer(opensovd_extra::trace::server_layer())
        .layer(tower::util::option_layer(cors))
        .base_uri(args.url().clone())?
        .vendor_info(VENDOR_INFO)
        .build()?;

    #[cfg(feature = "mdns")]
    {
        use anyhow::Context;

        let announcer = match mdns {
            Some(mdns) => Some(
                mdns.announce()
                    .await
                    .context("failed to announce via mDNS")?,
            ),
            None => None,
        };
        let _ = announcer_tx.send(announcer);
    }

    libcli::notify_ready();
    server.serve().await?;
    tracing::info!(target: TARGET, "Shutdown complete");

    Ok(())
}

#[cfg_attr(not(feature = "mock"), expect(clippy::unused_async))]
async fn configure_topology(cli: &cli::Cli) -> Topology {
    #[cfg(feature = "mock")]
    let topology = if cli.mock {
        tracing::info!(target: TARGET, "Mock topology enabled");
        create_mock_topology().await
    } else {
        Topology::default()
    };

    #[cfg(not(feature = "mock"))]
    let topology = {
        let _ = cli;
        Topology::default()
    };

    topology
}

/// The layer that forwards to the upstreams, if there are any.
fn create_federation(
    upstream: &cli::UpstreamArgs,
    topology: Topology,
) -> anyhow::Result<Option<FederationLayer>> {
    let upstreams = upstream::configured(&upstream.config()?)?;
    if upstreams.is_empty() {
        return Ok(None);
    }
    let federation = Federation::new(upstreams, topology)?.into_layer();
    Ok(Some(federation))
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;
    use std::time::Duration;

    use opensovd_core::Component;
    use opensovd_models::data::DataCategory;
    use opensovd_providers::data::{Constant, DataProviderBuilder};
    use opensovd_server::Authenticated;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tokio::sync::oneshot;

    use super::*;

    /// Authenticates the token `ok` only.
    struct OkToken;

    impl opensovd_server::Authenticator for OkToken {
        fn authenticate(
            &self,
            parts: &http::request::Parts,
        ) -> Result<Option<opensovd_server::Principal>, opensovd_server::AuthError> {
            Ok(parts
                .headers
                .get(http::header::AUTHORIZATION)
                .filter(|value| *value == "Bearer ok")
                .map(|_| principal()))
        }
    }

    fn principal() -> opensovd_server::Principal {
        opensovd_server::Principal {
            subject: "user".to_owned(),
            roles: Vec::new(),
            scopes: Vec::new(),
            claims: serde_json::Map::new(),
        }
    }

    async fn get(address: SocketAddr, authorization: Option<&str>) -> String {
        let authorization = authorization
            .map(|value| format!("Authorization: {value}\r\n"))
            .unwrap_or_default();
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if let Ok(mut stream) = tokio::net::TcpStream::connect(address).await {
                    let request = format!(
                        "GET /sovd/version-info HTTP/1.1\r\nHost: localhost\r\n{authorization}Connection: close\r\n\r\n"
                    );
                    stream.write_all(request.as_bytes()).await.unwrap();
                    let mut response = Vec::new();
                    stream.read_to_end(&mut response).await.unwrap();
                    break String::from_utf8(response).unwrap();
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("gateway started")
    }

    #[tokio::test]
    async fn supplied_auth_is_enforced_and_shuts_down() {
        let reservation = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = reservation.local_addr().unwrap();
        let url = format!("http://{address}/sovd");
        let cli = cli::try_parse_from(["gateway", "--url", &url]).unwrap();
        let (stop, stopped) = oneshot::channel();
        drop(reservation);
        let task = tokio::spawn(run_with_auth_and_shutdown(
            cli,
            Topology::default(),
            Auth::new(OkToken, Authenticated),
            async {
                let _ = stopped.await;
            },
        ));
        let anonymous = get(address, None).await;
        assert!(anonymous.starts_with("HTTP/1.1 401"), "{anonymous}");
        let wrong = get(address, Some("Bearer no")).await;
        assert!(wrong.starts_with("HTTP/1.1 401"), "{wrong}");
        let valid = get(address, Some("Bearer ok")).await;
        assert!(valid.starts_with("HTTP/1.1 200"), "{valid}");
        stop.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(5), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn supplied_auth_cannot_be_combined_with_jwks() {
        let cli = cli::try_parse_from([
            "gateway",
            "--auth-jwks",
            "jwks.json",
            "--auth-issuer",
            "issuer",
        ])
        .unwrap();
        let error = run_with_auth_and_shutdown(
            cli,
            Topology::default(),
            Auth::new(OkToken, Authenticated),
            std::future::pending(),
        )
        .await
        .unwrap_err();
        assert!(error.to_string().contains("--auth-jwks"));
    }

    #[cfg(feature = "mock")]
    #[tokio::test]
    async fn supplied_auth_cannot_be_combined_with_mock() {
        let cli = cli::try_parse_from(["gateway", "--mock"]).unwrap();
        let error = run_with_auth_and_shutdown(
            cli,
            Topology::default(),
            Auth::new(OkToken, Authenticated),
            std::future::pending(),
        )
        .await
        .unwrap_err();
        assert!(error.to_string().contains("--mock"));
    }

    #[test]
    fn invalid_arguments_are_returned_to_the_caller() {
        assert!(cli::try_parse_from(["gateway", "--not-an-option"]).is_err());
        #[cfg(feature = "mdns")]
        assert!(cli::try_parse_from(["gateway", "--mdns", ""]).is_err());
    }

    #[cfg(feature = "mock")]
    #[tokio::test]
    async fn supplied_topology_cannot_be_combined_with_mock() {
        let cli = cli::try_parse_from(["gateway", "--mock"]).unwrap();
        let error = run_with_shutdown(cli, Topology::default(), std::future::pending())
            .await
            .unwrap_err();
        assert!(error.to_string().contains("--mock"));
    }

    #[test]
    fn invalid_cors_is_returned_not_exited() {
        let error = cli::try_parse_from(["gateway", "--cors-origin", "*", "--cors-credentials"])
            .err()
            .unwrap();
        assert_eq!(error.kind(), clap::error::ErrorKind::ValueValidation);
    }

    #[cfg(feature = "mdns")]
    #[tokio::test]
    async fn mdns_on_loopback_is_rejected() {
        let cli = cli::try_parse_from([
            "gateway",
            "--url",
            "http://127.0.0.1:0/sovd",
            "--mdns",
            "test-id",
        ])
        .unwrap();
        let error = run_with_shutdown(cli, Topology::default(), std::future::pending())
            .await
            .unwrap_err();
        assert!(error.to_string().contains("loopback"));
    }

    #[tokio::test]
    async fn supplied_topology_is_served_alongside_upstream_and_shuts_down() {
        let upstream_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_addr = upstream_listener.local_addr().unwrap();
        let upstream_topology = Topology::default();
        upstream_topology
            .write()
            .await
            .add_component(Component::new("remote", "Remote"));
        let (upstream_stop, upstream_stopped) = oneshot::channel();
        let upstream = Server::builder()
            .listener(upstream_listener)
            .base_uri(format!("http://{upstream_addr}/remote"))
            .unwrap()
            .topology(upstream_topology)
            .shutdown(async {
                let _ = upstream_stopped.await;
            })
            .build()
            .unwrap();
        let upstream_task = tokio::spawn(async move { upstream.serve().await });

        // The runner binds from its URL; reserve an ephemeral port until just before startup.
        let reservation = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = reservation.local_addr().unwrap();
        let url = format!("http://{address}/sovd");
        let remote = format!("http://{upstream_addr}/remote/v1");
        let cli =
            cli::try_parse_from(["gateway", "--url", &url, "--upstream-url", &remote]).unwrap();
        let provider = DataProviderBuilder::new()
            .read_data(
                "example.version",
                "Version",
                &DataCategory::IdentData,
                Constant::new("1.0.0").unwrap(),
            )
            .build()
            .unwrap();
        let topology = Topology::default();
        topology
            .write()
            .await
            .add_component(Component::new("local", "Local").with_data_provider(provider));
        let (stop, stopped) = oneshot::channel();
        drop(reservation);
        let task = tokio::spawn(run_with_shutdown(cli, topology, async {
            let _ = stopped.await;
        }));
        let response = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if let Ok(mut stream) = tokio::net::TcpStream::connect(address).await {
                    stream.write_all(b"GET /sovd/v1/components HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").await.unwrap();
                    let mut response = Vec::new();
                    stream.read_to_end(&mut response).await.unwrap();
                    break String::from_utf8(response).unwrap();
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("gateway started");
        assert!(response.starts_with("HTTP/1.1 200"), "{response}");
        assert!(response.contains("\"id\":\"local\""), "{response}");
        assert!(response.contains("\"id\":\"remote\""), "{response}");
        let mut stream = tokio::net::TcpStream::connect(address).await.unwrap();
        stream.write_all(b"GET /sovd/v1/components/local/data/example.version HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").await.unwrap();
        let mut response = Vec::new();
        stream.read_to_end(&mut response).await.unwrap();
        let response = String::from_utf8(response).unwrap();
        assert!(response.starts_with("HTTP/1.1 200"), "{response}");
        assert!(response.contains("1.0.0"), "{response}");
        stop.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(5), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        upstream_stop.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(5), upstream_task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }
}
