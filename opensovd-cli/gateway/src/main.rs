// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! OpenSOVD Gateway server binary.

mod cli;
mod cors;
mod serve_dir;

use std::net::SocketAddr;
use std::process::ExitCode;

use anyhow::Context;
use base64::Engine;
use opensovd_core::Topology;
use opensovd_extra::{JwtAlgorithm, JwtAuthenticator, RegorusAuthorizer};
#[cfg(feature = "mock")]
use opensovd_mocks::create_mock_topology;
use opensovd_server::{AllowAll, Authenticator, Authorizer, NoAuth, Server};
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
#[allow(clippy::print_stderr)]
async fn main() -> ExitCode {
    let cli = cli::parse();

    if let Err(e) = libcli::init_tracing(
        "gw=info,srv=info,mdns=info,tower_http=debug,axum=trace",
        None,
    ) {
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
    let jwt_key = cli.auth.jwt_key.take();

    if let Some(key) = jwt_key {
        let authenticator = create_jwt_authenticator(&key, &mut cli.auth)?;

        if cli.auth.policy.is_empty() {
            serve(cli, authenticator, AllowAll).await
        } else {
            let authorizer = create_rego_authorizer(&mut cli.auth)?;
            serve(cli, authenticator, authorizer).await
        }
    } else {
        serve(cli, NoAuth, AllowAll).await
    }
}

fn create_jwt_authenticator(
    secret: &str,
    auth: &mut cli::AuthArgs,
) -> anyhow::Result<JwtAuthenticator> {
    let algo: JwtAlgorithm = auth
        .jwt_algo
        .parse()
        .map_err(|e: String| anyhow::anyhow!(e))
        .with_context(|| format!("invalid --auth-jwt-algo {:?}", auth.jwt_algo))?;
    let key = base64::engine::general_purpose::STANDARD
        .decode(secret)
        .context("--auth-jwt-secret must be base64-encoded")?;
    let issuer = std::mem::take(&mut auth.jwt_issuer);

    tracing::info!(target: TARGET, %algo, %issuer, "JWT authentication enabled");
    Ok(JwtAuthenticator::new(algo, &key, &issuer))
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

async fn serve<Authn, Authz>(
    cli: cli::Cli,
    authenticator: Authn,
    authorizer: Authz,
) -> anyhow::Result<()>
where
    Authn: Authenticator,
    Authz: Authorizer<Authn::Identity>,
{
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

    let builder = Server::builder()
        .authenticator(authenticator)
        .authorizer(authorizer);

    let (mut builder, local_addr) = configure_listener(builder, &cli, authority).await?;
    builder = configure_topology(builder, &cli).await;

    #[cfg(feature = "tls")]
    let tls = if let Some(tls_config) = cli.tls.build()? {
        tracing::info!(target: TARGET, "TLS enabled");
        builder = builder.tls(tls_config);
        true
    } else {
        false
    };
    #[cfg(not(feature = "tls"))]
    let tls = false;

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
        .layer(libcli::trace::trace_layer())
        .layer(tower::util::option_layer(cors))
        .base_uri(uri)?
        .vendor_info(VENDOR_INFO)
        .build()?;

    #[cfg(feature = "mdns")]
    {
        let announcer = mdns
            .map(opensovd_extra::Announcement::announce)
            .transpose()
            .context("failed to announce via mDNS")?;
        let _ = announcer_tx.send(announcer);
    }

    notify_readiness();
    server.serve().await?;
    tracing::info!(target: TARGET, "Shutdown complete");

    Ok(())
}

type Builder<Vendor, Authn, Authz, Layer> =
    opensovd_server::ServerBuilder<Vendor, Authn, Authz, Layer>;

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
async fn configure_listener<Vendor, Authn, Authz, Layer>(
    builder: Builder<Vendor, Authn, Authz, Layer>,
    cli: &cli::Cli,
    authority: &str,
) -> anyhow::Result<(Builder<Vendor, Authn, Authz, Layer>, Option<Bound>)> {
    #[cfg(target_os = "linux")]
    if let Some(fd) = sd_notify::listen_fds()?.next() {
        use std::os::fd::FromRawFd;
        // SAFETY: fd is valid and owned, provided by systemd socket activation
        #[allow(unsafe_code)]
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
async fn configure_listener<Vendor, Authn, Authz, Layer>(
    builder: Builder<Vendor, Authn, Authz, Layer>,
    _cli: &cli::Cli,
    authority: &str,
) -> anyhow::Result<(Builder<Vendor, Authn, Authz, Layer>, Option<Bound>)> {
    bind_tcp(builder, authority).await
}

async fn bind_tcp<Vendor, Authn, Authz, Layer>(
    builder: Builder<Vendor, Authn, Authz, Layer>,
    authority: &str,
) -> anyhow::Result<(Builder<Vendor, Authn, Authz, Layer>, Option<Bound>)> {
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
        .interfaces(args.interfaces)
        .ipv6_only(v6_only);
    if let Some(host) = args.host {
        announcement = announcement.host(host);
    }
    Ok(Some(announcement))
}

async fn configure_topology<Vendor, Authn, Authz, Layer>(
    builder: opensovd_server::ServerBuilder<Vendor, Authn, Authz, Layer>,
    cli: &cli::Cli,
) -> opensovd_server::ServerBuilder<Vendor, Authn, Authz, Layer> {
    #[cfg(feature = "mock")]
    let topology = if cli.mock {
        tracing::info!(target: TARGET, "Mock topology enabled");
        create_mock_topology().await
    } else {
        Topology::default()
    };

    #[cfg(not(feature = "mock"))]
    let topology = Topology::default();

    builder.topology(topology)
}

fn notify_readiness() {
    #[cfg(target_os = "linux")]
    if let Err(e) = sd_notify::notify(&[sd_notify::NotifyState::Ready]) {
        tracing::warn!(target: TARGET, error = %e, "Failed to notify systemd readiness");
    }
}
