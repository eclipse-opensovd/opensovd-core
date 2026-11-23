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
mod cors;
mod serve_dir;
mod upstream;

use std::net::SocketAddr;
use std::path::Path;

use anyhow::Context;
use opensovd_core::Topology;
use opensovd_extra::{Federation, FederationLayer, JwtAuthenticator, RegorusAuthorizer};
#[cfg(feature = "mock")]
use opensovd_mocks::create_mock_topology;
use opensovd_server::{Auth, Authenticated, Server};
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
    run_gateway(cli, topology, libcli::shutdown_signal()).await
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
    run_gateway(cli, topology, shutdown).await
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
        cli.auth.jwks.is_none(),
        "--auth-jwks cannot be used with supplied auth"
    );
    serve(cli, Some(auth), topology, shutdown).await
}

async fn run_gateway(
    mut cli: cli::Cli,
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
    let auth = create_auth(&mut cli.auth)?;
    serve(cli, auth, topology, shutdown).await
}

fn create_auth(args: &mut cli::AuthArgs) -> anyhow::Result<Option<Auth>> {
    let Some(jwks) = args.jwks.take() else {
        return Ok(None);
    };
    let jwt = create_jwt_authenticator(&jwks, args)?;
    Ok(Some(if args.policy.is_empty() {
        Auth::new(jwt, Authenticated)
    } else {
        Auth::new(jwt, create_rego_authorizer(args)?)
    }))
}

fn create_jwt_authenticator(
    jwks: &Path,
    auth: &mut cli::AuthArgs,
) -> anyhow::Result<JwtAuthenticator> {
    let text = std::fs::read_to_string(jwks)
        .with_context(|| format!("failed to read --auth-jwks {}", jwks.display()))?;
    let invalid = || format!("invalid --auth-jwks {}", jwks.display());
    let set: serde_json::Value = serde_json::from_str(&text).with_context(invalid)?;
    if let Some(kid) = private_key(&set) {
        anyhow::bail!("--auth-jwks {} holds the private key {kid}", jwks.display());
    }
    let set = serde_json::from_value(set).with_context(invalid)?;
    let issuer = auth
        .issuer
        .take()
        .context("--auth-jwks requires --auth-issuer")?;
    let audience = std::mem::take(&mut auth.audience);
    let authenticator = JwtAuthenticator::new(&set, &issuer, &audience)
        .map_err(anyhow::Error::from_boxed)
        .with_context(invalid)?;

    tracing::info!(target: TARGET, jwks = %jwks.display(), %issuer, ?audience, "JWT authentication enabled");
    Ok(authenticator)
}

/// The `kid` of the first key in `jwks` with private key material.
fn private_key(jwks: &serde_json::Value) -> Option<&str> {
    let keys = jwks.get("keys")?.as_array()?;
    let key = keys.iter().find(|key| key.get("d").is_some())?;
    Some(
        key.get("kid")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("without kid"),
    )
}

fn create_rego_authorizer(auth: &mut cli::AuthArgs) -> anyhow::Result<RegorusAuthorizer> {
    let policies = std::mem::take(&mut auth.policy);
    let policy_data = std::mem::take(&mut auth.policy_data);

    let authorizer = RegorusAuthorizer::from_paths(&policies, &policy_data)
        .map_err(anyhow::Error::from_boxed)
        .context("failed to load Rego authorization policies")?;
    tracing::info!(target: TARGET, count = policies.len(), "Rego policy authorization enabled");
    Ok(authorizer)
}

async fn serve(
    cli: cli::Cli,
    auth: Option<Auth>,
    topology: Topology,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> anyhow::Result<()> {
    let uri: http::Uri = cli
        .url
        .parse()
        .with_context(|| format!("invalid --url {:?}", cli.url))?;
    let authority = uri
        .authority()
        .ok_or_else(|| {
            anyhow::anyhow!("--url must include host:port (e.g., http://localhost:7690/sovd)")
        })?
        .as_str();

    let builder = Server::builder();
    let builder = match auth {
        Some(auth) => builder.auth(auth),
        None => builder,
    };

    #[cfg(feature = "tls")]
    let (builder, tls) = if let Some(tls) = cli.tls.config() {
        anyhow::ensure!(
            uri.scheme_str() == Some("https"),
            "--tls-cert requires an https:// --url"
        );
        let mtls = tls.is_mtls();
        let config = tls.build().context("failed to configure TLS")?;
        tracing::info!(target: TARGET, mtls, "TLS enabled");
        (builder.tls(config), true)
    } else {
        (builder, false)
    };
    #[cfg(not(feature = "tls"))]
    let tls = false;

    let (mut builder, local_addr) = configure_listener(builder, &cli, authority).await?;
    builder = builder.topology(topology.clone());
    // after auth, so that forwarded requests are authorized like any other
    if let Some(federation) = create_federation(&cli.upstream, topology)? {
        builder = builder.inner_layer(federation);
    }

    #[cfg(feature = "mdns")]
    let mdns = configure_mdns(cli.mdns, local_addr, &uri, tls)?;
    #[cfg(not(feature = "mdns"))]
    let _ = (local_addr.map(|b| (b.addr, b.v6_only, b.activated)), tls);

    let cors = cors::create_cors_layer(
        &cli.cors.origins,
        &cli.cors.methods,
        &cli.cors.headers,
        cli.cors.credentials,
        cli.cors.max_age,
    )
    .map_err(|e| {
        use clap::CommandFactory;
        cli::Cli::command().error(clap::error::ErrorKind::ValueValidation, e)
    })?;
    if cors.is_some() {
        tracing::info!(target: TARGET, "CORS enabled");
    }

    if let Some(ref serve_dir_arg) = cli.serve_dir {
        let (path, dir) = serve_dir_arg.split_once(':').ok_or_else(|| {
            anyhow::anyhow!("--serve-dir format: PATH:DIRECTORY (e.g., /ui:./webui/dist)")
        })?;
        let svc = serve_dir::create_serve_dir(dir);
        builder = builder.service(path, svc);
        tracing::info!(target: TARGET, path = %path, dir = %dir, "Serving static files");
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
        .base_uri(uri)?
        .vendor_info(VENDOR_INFO)
        .build()?;

    #[cfg(feature = "mdns")]
    {
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

    notify_readiness();
    server.serve().await?;
    tracing::info!(target: TARGET, "Shutdown complete");

    Ok(())
}

type Builder<Vendor, Layer> = opensovd_server::ServerBuilder<Vendor, Layer>;

#[derive(Clone, Copy)]
struct Bound {
    addr: SocketAddr,
    v6_only: bool,
    activated: bool,
}

impl Bound {
    fn of(listener: &tokio::net::TcpListener, activated: bool) -> std::io::Result<Self> {
        let addr = listener.local_addr()?;
        #[cfg(feature = "mdns")]
        let v6_only = addr.is_ipv6() && socket2::SockRef::from(listener).only_v6()?;
        #[cfg(not(feature = "mdns"))]
        let v6_only = false;
        Ok(Self {
            addr,
            v6_only,
            activated,
        })
    }
}

#[cfg(unix)]
async fn configure_listener<Vendor, Layer>(
    builder: Builder<Vendor, Layer>,
    cli: &cli::Cli,
    authority: &str,
) -> anyhow::Result<(Builder<Vendor, Layer>, Option<Bound>)> {
    #[cfg(target_os = "linux")]
    if let Some(fd) = sd_notify::listen_fds()?.next() {
        use std::os::fd::FromRawFd;
        // SAFETY: fd is valid and owned, provided by systemd socket activation
        #[expect(unsafe_code)]
        let std_listener = unsafe { std::net::TcpListener::from_raw_fd(fd) };
        std_listener.set_nonblocking(true)?;
        let listener = tokio::net::TcpListener::from_std(std_listener)?;
        let bound = Bound::of(&listener, true)?;
        return Ok((builder.listener(listener), Some(bound)));
    }

    if let Some(ref socket_path) = cli.unix_socket {
        use tokio::net::UnixListener;

        #[cfg(target_os = "linux")]
        let listener = if let Some(name) = socket_path.strip_prefix('@') {
            use std::os::linux::net::SocketAddrExt;
            let addr =
                std::os::unix::net::SocketAddr::from_abstract_name(name).with_context(|| {
                    format!("invalid abstract socket name in --unix-socket {socket_path}")
                })?;
            let std_listener = std::os::unix::net::UnixListener::bind_addr(&addr)
                .with_context(|| format!("failed to bind abstract unix socket {socket_path}"))?;
            std_listener.set_nonblocking(true)?;
            UnixListener::from_std(std_listener)?
        } else {
            UnixListener::bind(socket_path)
                .with_context(|| format!("failed to bind unix socket {socket_path}"))?
        };

        #[cfg(not(target_os = "linux"))]
        let listener = UnixListener::bind(socket_path)
            .with_context(|| format!("failed to bind unix socket {socket_path}"))?;

        return Ok((builder.listener(listener), None));
    }

    bind_tcp(builder, authority).await
}

#[cfg(not(unix))]
async fn configure_listener<Vendor, Layer>(
    builder: Builder<Vendor, Layer>,
    _cli: &cli::Cli,
    authority: &str,
) -> anyhow::Result<(Builder<Vendor, Layer>, Option<Bound>)> {
    bind_tcp(builder, authority).await
}

async fn bind_tcp<Vendor, Layer>(
    builder: Builder<Vendor, Layer>,
    authority: &str,
) -> anyhow::Result<(Builder<Vendor, Layer>, Option<Bound>)> {
    let listener = tokio::net::TcpListener::bind(authority)
        .await
        .with_context(|| format!("failed to bind {authority}"))?;
    let bound = Bound::of(&listener, false)?;
    Ok((builder.listener(listener), Some(bound)))
}

#[cfg(feature = "mdns")]
fn configure_mdns(
    args: cli::MdnsArgs,
    bound: Option<Bound>,
    uri: &http::Uri,
    tls: bool,
) -> anyhow::Result<Option<opensovd_extra::Announcement>> {
    let Some(identification) = args.identification().map(str::to_owned) else {
        return Ok(None);
    };
    let Bound {
        addr,
        v6_only,
        activated,
    } = bound.context("--mdns requires a TCP listener")?;
    if addr.ip().to_canonical().is_loopback() {
        let hint = if activated {
            "set ListenStream in the systemd socket unit"
        } else {
            "bind --url"
        };
        anyhow::bail!(
            "--mdns cannot announce the loopback address {}; {hint} to 0.0.0.0 or an interface address",
            addr.ip()
        );
    }
    anyhow::ensure!(
        tls || uri.scheme_str() != Some("https"),
        "--mdns with an https --url requires --tls-cert, since clients connect to the announced port directly"
    );

    let mut announcement = opensovd_extra::Announcement::new(addr)
        .identification(identification)
        .access(if tls { "https" } else { "http" }, uri.path())
        .interfaces(args.interfaces())
        .ipv6_only(v6_only);
    if let Some(host) = args.host {
        announcement = announcement.host(host);
    }
    Ok(Some(announcement))
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

fn notify_readiness() {
    #[cfg(target_os = "linux")]
    if let Err(e) = sd_notify::notify(&[sd_notify::NotifyState::Ready]) {
        tracing::warn!(target: TARGET, error = %e, "Failed to notify systemd readiness");
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use opensovd_core::Component;
    use opensovd_models::data::DataCategory;
    use opensovd_providers::data::{Constant, DataProviderBuilder};
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

    #[tokio::test]
    async fn invalid_cors_is_returned_not_exited() {
        let cli = cli::try_parse_from([
            "gateway",
            "--url",
            "http://127.0.0.1:0/sovd",
            "--cors-origin",
            "*",
            "--cors-credentials",
        ])
        .unwrap();
        let error = run_with_shutdown(cli, Topology::default(), std::future::pending())
            .await
            .unwrap_err();
        assert!(matches!(
            error.downcast_ref::<clap::Error>(),
            Some(error) if error.kind() == clap::error::ErrorKind::ValueValidation
        ));
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
