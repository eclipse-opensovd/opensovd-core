// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

#![allow(clippy::print_stdout)]

//! CLI client example exercising the `opensovd-client` API.
//!
//! Connects to a running gateway, discovers the advertised SOVD versions via
//! `/version-info`, and for every found version prints its metadata and lists
//! its components, data items, apps, and areas.
//!
//! `--url` is the unversioned discovery root in every mode.
//!
//! ```text
//! # TCP (default)
//! cargo run --example client
//!
//! # Custom URL
//! cargo run --example client -- --url http://host:8080/sovd
//!
//! # Unix socket (filesystem path)
//! cargo run --example client -- --unix-socket /tmp/opensovd.sock --url http://localhost/sovd
//!
//! # Abstract Unix socket
//! cargo run --example client -- --unix-socket @opensovd --url http://localhost/sovd
//! ```

use std::time::Duration;

use bytes::Bytes;
use clap::Parser;
use http::{HeaderMap, Request, Response};
use http_body_util::Full;
use opensovd_client::{Client, Discovery, SovdInfo};
use tower_http::classify::ServerErrorsFailureClass;
use tower_http::trace::TraceLayer;
use tracing::Span;

#[derive(Parser)]
#[command(name = "client")]
#[command(about = "OpenSOVD client example")]
#[command(after_help = "\
Examples:
  # Discover versions over TCP (default)
  client --url http://localhost:7690/sovd

  # Discover over a Unix socket (filesystem path)
  client --unix-socket /tmp/opensovd.sock --url http://localhost/sovd

  # Discover over an abstract Unix socket
  client --unix-socket @opensovd --url http://localhost/sovd
")]
struct Cli {
    /// SOVD `accessurl`: root of the SOVD API, parent of `version-info` (no version
    /// identifier). `/version-info` is fetched from it to enumerate supported versions.
    #[arg(long, default_value = "http://localhost:7690/sovd")]
    url: String,

    /// Path to a Unix socket to connect to. Use '@' prefix for abstract sockets.
    /// When specified, the path component of --url is used as the base path.
    #[cfg(unix)]
    #[arg(long)]
    unix_socket: Option<String>,
}

/// Exercise the client API and print results.
async fn run(client: &Client) -> Result<(), opensovd_client::Error> {
    // Components
    let components = client.list_components().send().await?;
    for c in &components.data.items {
        println!("component: {} ({})", c.id, c.name);
    }

    // Data items for the first component
    if let Some(first) = components.data.items.first() {
        let data = client.component(&first.id).list_data().send().await?;
        for d in &data.data.items {
            println!("  data: {} ({})", d.id, d.name);
        }
    }

    // Apps
    let apps = client.list_apps().send().await?;
    for a in &apps.data.items {
        println!("app: {} ({})", a.id, a.name);
    }

    // Areas
    let areas = client.list_areas().send().await?;
    for a in &areas.data.items {
        println!("area: {} ({})", a.id, a.name);
    }

    Ok(())
}

/// Print each advertised version's metadata and run the exercises against its client.
async fn discover_and_run(discovery: &Discovery) -> Result<(), opensovd_client::Error> {
    // Value vendor payload so any server's vendor_info shape prints.
    let versions = discovery.versions::<serde_json::Value>().await?;
    println!("found {} version(s)", versions.len());

    for v in &versions {
        let vendor = v.vendor_info.as_ref().map_or_else(
            || "none".to_string(),
            |info| serde_json::to_string_pretty(info).unwrap_or_else(|_| "<unprintable>".into()),
        );
        println!("version {}", v.version);
        println!("  base_uri: {}", v.base_uri.0);
        println!("  vendor_info: {vendor}");

        let client = discovery
            .select(|s: &SovdInfo<serde_json::Value>| s.version == v.version)
            .await?;
        run(&client).await?;
    }

    Ok(())
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    drop(libcli::init_tracing("info", None));
    let cli = Cli::parse();

    #[cfg(unix)]
    if let Some(ref socket) = cli.unix_socket {
        if let Some(name) = socket.strip_prefix('@') {
            #[cfg(target_os = "linux")]
            {
                let discovery = Discovery::connect_unix_abstract(&cli.url, name)?;
                discover_and_run(&discovery).await?;
                return Ok(());
            }
            #[cfg(not(target_os = "linux"))]
            {
                _ = name;
                return Err("abstract Unix sockets are only supported on Linux".into());
            }
        }
        let discovery = Discovery::connect_unix(&cli.url, socket)?;
        discover_and_run(&discovery).await?;
        return Ok(());
    }

    let discovery = Client::builder()
        .base_uri(&cli.url)?
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(|req: &Request<Full<Bytes>>| {
                    tracing::debug_span!(
                        target: "cli",
                        "http",
                        method = %req.method(),
                        url = %req.uri(),
                        status_code = tracing::field::Empty,
                        latency_us = tracing::field::Empty,
                    )
                })
                .on_request(|_req: &Request<Full<Bytes>>, _span: &Span| {
                    tracing::debug!(target: "cli", "Requesting");
                })
                .on_response(
                    |res: &Response<hyper::body::Incoming>, latency: Duration, span: &Span| {
                        span.record("status_code", res.status().as_u16());
                        span.record(
                            "latency_us",
                            u64::try_from(latency.as_micros()).unwrap_or(u64::MAX),
                        );
                    },
                )
                .on_eos(|_: Option<&HeaderMap>, _duration: Duration, _span: &Span| {
                    tracing::debug!(target: "cli", "Stream closed");
                })
                .on_failure(
                    |ec: ServerErrorsFailureClass, latency: Duration, span: &Span| {
                        span.record(
                            "latency_us",
                            u64::try_from(latency.as_micros()).unwrap_or(u64::MAX),
                        );
                        match ec {
                            ServerErrorsFailureClass::StatusCode(status) => {
                                span.record("status_code", status.as_u16());
                                tracing::error!(target: "cli", %status, "Request failed");
                            }
                            ServerErrorsFailureClass::Error(err) => {
                                tracing::error!(target: "cli", error = %err, "Request failed");
                            }
                        }
                    },
                ),
        )
        .discovery()?;
    discover_and_run(&discovery).await?;
    Ok(())
}
