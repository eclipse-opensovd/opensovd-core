// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! OpenSOVD Gateway server binary.

mod cli;

use std::process::ExitCode;

use clap::Parser;
use opensovd_core::Topology;
#[cfg(feature = "mock")]
use opensovd_mocks::create_mock_topology;
use opensovd_server::Server;
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
    let cli = cli::Cli::parse();

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

async fn run(cli: cli::Cli) -> anyhow::Result<()> {
    tracing::info!(
        target: TARGET,
        version = %VENDOR_INFO.version,
        channel = %env!("RELEASE_CHANNEL"),
        sha1 = %VENDOR_INFO.sha1,
        build_date = %VENDOR_INFO.build_date,
        "{}", cli::ABOUT);
    let args = &cli.server;

    let mut builder = Server::builder();
    if let Some(auth) = args.auth()? {
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
    builder = configure_topology(builder, &cli).await;

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
        libcli::shutdown_signal().await;
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
async fn configure_topology<Vendor, Layer>(
    builder: opensovd_server::ServerBuilder<Vendor, Layer>,
    cli: &cli::Cli,
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
