// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! OpenSOVD Gateway server binary.

mod cli;
mod cors;
mod serve_dir;

use std::net::SocketAddr;
use std::path::Path;
use std::process::ExitCode;

use anyhow::Context;
use opensovd_core::Topology;
use opensovd_extra::{JwtAuthenticator, RegorusAuthorizer};
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

#[tokio::main(flavor = "current_thread")]
#[expect(clippy::print_stderr)]
async fn main() -> ExitCode {
    let cli = cli::parse();

    if let Err(e) = libcli::init_tracing("warn,gw=info,srv=info,mdns=info,tower_http=debug", None) {
        eprintln!("Failed to initialize tracing: {e}");
        return ExitCode::FAILURE;
    }

    if let Err(e) = run(cli).await {
        eprintln!("Error: {e:?}");
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}

async fn run(mut cli: cli::Cli) -> anyhow::Result<()> {
    tracing::info!(
        target: TARGET,
        version = %VENDOR_INFO.version,
        channel = %env!("RELEASE_CHANNEL"),
        sha1 = %VENDOR_INFO.sha1,
        build_date = %VENDOR_INFO.build_date,
        "{}", cli::ABOUT);
    let auth = create_auth(&mut cli.auth)?;
    serve(cli, auth).await
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

async fn serve(cli: cli::Cli, auth: Option<Auth>) -> anyhow::Result<()> {
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
    builder = configure_topology(builder, &cli).await;

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
        cli::Cli::command()
            .error(clap::error::ErrorKind::ValueValidation, e)
            .exit()
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
        libcli::shutdown_signal().await;
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
async fn configure_topology<Vendor, Layer>(
    builder: opensovd_server::ServerBuilder<Vendor, Layer>,
    #[cfg_attr(not(feature = "mock"), expect(unused_variables))] cli: &cli::Cli,
) -> opensovd_server::ServerBuilder<Vendor, Layer> {
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

    builder.topology(topology)
}

fn notify_readiness() {
    #[cfg(target_os = "linux")]
    if let Err(e) = sd_notify::notify(&[sd_notify::NotifyState::Ready]) {
        tracing::warn!(target: TARGET, error = %e, "Failed to notify systemd readiness");
    }
}
