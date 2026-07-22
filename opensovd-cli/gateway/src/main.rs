// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! OpenSOVD Gateway server binary.

mod cli;
mod cors;
mod serve_dir;

#[cfg(feature = "mdns")]
mod mdns;

use std::process::ExitCode;
#[cfg(feature = "mdns")]
use std::sync::Arc;

use anyhow::Context;
use base64::Engine;
use clap::Parser;
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
    version: env!("CARGO_PKG_VERSION"),
    sha1: env!("COMMIT_SHA"),
    build_date: env!("BUILD_DATE"),
    name: "OpenSOVD",
};

#[tokio::main(flavor = "current_thread")]
#[allow(clippy::print_stderr)]
async fn main() -> ExitCode {
    let cli = cli::Cli::parse();

    if let Err(e) = libcli::init_tracing("gw=info,srv=info,tower_http=debug,axum=trace", None) {
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
    let base_uri = uri.path();
    let authority = uri
        .authority()
        .ok_or_else(|| {
            anyhow::anyhow!("--url must include host:port (e.g., http://localhost:7690/sovd)")
        })?
        .as_str();

    let builder = Server::builder()
        .authenticator(authenticator)
        .authorizer(authorizer);

    let (mut builder, listener_addr) = configure_listener(builder, &cli, authority).await?;
    #[cfg(not(feature = "mdns"))]
    let _ = listener_addr;
    builder = configure_topology(builder, &cli).await;

    #[cfg(all(feature = "tls", feature = "mdns"))]
    let (mut builder, tls_enabled) = configure_tls(builder, cli.tls)?;
    #[cfg(all(feature = "tls", not(feature = "mdns")))]
    let mut builder = configure_tls(builder, cli.tls)?;
    #[cfg(all(not(feature = "tls"), feature = "mdns"))]
    let tls_enabled = false;

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
    let (builder, mdns_wrapper) = configure_mdns(
        builder,
        &cli.mdns,
        &uri,
        listener_addr,
        base_uri,
        tls_enabled,
    );

    let server = builder
        .layer(libcli::trace::trace_layer())
        .layer(tower::util::option_layer(cors))
        .base_uri(base_uri)?
        .vendor_info(VENDOR_INFO)
        .build()?;

    notify_readiness();
    let serve_result = server.serve().await;

    #[cfg(feature = "mdns")]
    if let Some(wrapper) = mdns_wrapper
        && let Err(e) = wrapper.shutdown()
    {
        tracing::warn!(target: TARGET, error = %e, "Failed to shut down mDNS");
    }

    serve_result?;
    tracing::info!(target: TARGET, "Shutdown complete");

    Ok(())
}

#[cfg(all(feature = "tls", feature = "mdns"))]
fn configure_tls<Vendor, Authn, Authz, Layer>(
    builder: opensovd_server::ServerBuilder<Vendor, Authn, Authz, Layer>,
    tls: cli::TlsArgs,
) -> anyhow::Result<(
    opensovd_server::ServerBuilder<Vendor, Authn, Authz, Layer>,
    bool,
)> {
    if let Some(tls_config) = tls.build()? {
        tracing::info!(target: TARGET, "TLS enabled");
        Ok((builder.tls(tls_config), true))
    } else {
        Ok((builder, false))
    }
}

#[cfg(all(feature = "tls", not(feature = "mdns")))]
fn configure_tls<Vendor, Authn, Authz, Layer>(
    builder: opensovd_server::ServerBuilder<Vendor, Authn, Authz, Layer>,
    tls: cli::TlsArgs,
) -> anyhow::Result<opensovd_server::ServerBuilder<Vendor, Authn, Authz, Layer>> {
    if let Some(tls_config) = tls.build()? {
        tracing::info!(target: TARGET, "TLS enabled");
        Ok(builder.tls(tls_config))
    } else {
        Ok(builder)
    }
}

#[cfg(feature = "mdns")]
fn configure_mdns<Vendor, Authn, Authz, Layer>(
    mut builder: opensovd_server::ServerBuilder<Vendor, Authn, Authz, Layer>,
    mdns_args: &cli::MdnsArgs,
    uri: &http::Uri,
    listener_addr: Option<std::net::SocketAddr>,
    base_uri: &str,
    tls_enabled: bool,
) -> (
    opensovd_server::ServerBuilder<Vendor, Authn, Authz, Layer>,
    Option<Arc<opensovd_providers::mdns::MdnsWrapper>>,
) {
    if !mdns_args.enabled {
        return (builder, None);
    }

    let mdns_scheme = if tls_enabled {
        "https"
    } else {
        uri.scheme_str().unwrap_or("http")
    };

    match mdns::setup(mdns_args, listener_addr, mdns_scheme, base_uri) {
        Ok((wrapper, provider)) => {
            tracing::info!(target: TARGET, "mDNS enabled");
            builder = builder.discovery(Box::new(provider));
            (builder, Some(wrapper))
        }
        Err(e) => {
            tracing::error!(target: TARGET, error = %e, "Failed to start mDNS");
            (builder, None)
        }
    }
}

#[cfg(unix)]
async fn configure_listener<Vendor, Authn, Authz, Layer>(
    builder: opensovd_server::ServerBuilder<Vendor, Authn, Authz, Layer>,
    cli: &cli::Cli,
    authority: &str,
) -> anyhow::Result<(
    opensovd_server::ServerBuilder<Vendor, Authn, Authz, Layer>,
    Option<std::net::SocketAddr>,
)> {
    #[cfg(target_os = "linux")]
    if let Some(fd) = sd_notify::listen_fds()?.next() {
        use std::os::fd::FromRawFd;
        // SAFETY: fd is valid and owned, provided by systemd socket activation
        #[allow(unsafe_code)]
        let std_listener = unsafe { std::net::TcpListener::from_raw_fd(fd) };
        std_listener.set_nonblocking(true)?;
        let listener = tokio::net::TcpListener::from_std(std_listener)?;
        let addr = listener.local_addr()?;
        return Ok((builder.listener(listener), Some(addr)));
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

    let listener = tokio::net::TcpListener::bind(authority)
        .await
        .with_context(|| format!("failed to bind {authority}"))?;
    let addr = listener.local_addr()?;
    Ok((builder.listener(listener), Some(addr)))
}

#[cfg(not(unix))]
async fn configure_listener<Vendor, Authn, Authz, Layer>(
    builder: opensovd_server::ServerBuilder<Vendor, Authn, Authz, Layer>,
    _cli: &cli::Cli,
    authority: &str,
) -> anyhow::Result<(
    opensovd_server::ServerBuilder<Vendor, Authn, Authz, Layer>,
    Option<std::net::SocketAddr>,
)> {
    let listener = tokio::net::TcpListener::bind(authority)
        .await
        .with_context(|| format!("failed to bind {authority}"))?;
    let addr = listener.local_addr()?;
    Ok((builder.listener(listener), Some(addr)))
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
