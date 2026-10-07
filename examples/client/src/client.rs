// SPDX-FileCopyrightText: 2026 Copyright (c) Contributors to the Eclipse Foundation
//
// See the NOTICE file(s) distributed with this work for additional
// information regarding copyright ownership.
//
// This program and the accompanying materials are made available under the
// terms of the Apache License Version 2.0 which is available at
// https://www.apache.org/licenses/LICENSE-2.0
//
// SPDX-License-Identifier: Apache-2.0

#![expect(clippy::print_stdout)]

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

use clap::Parser;
use opensovd_client::{Client, Discovery, SovdInfo};

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
                let discovery = Client::builder()
                    .base_uri(&cli.url)?
                    .unix_socket_abstract(name)
                    .layer(opensovd_extra::trace::client_layer())
                    .discovery()?;
                discover_and_run(&discovery).await?;
                return Ok(());
            }
            #[cfg(not(target_os = "linux"))]
            {
                _ = name;
                return Err("abstract Unix sockets are only supported on Linux".into());
            }
        }
        let discovery = Client::builder()
            .base_uri(&cli.url)?
            .unix_socket(socket)
            .layer(opensovd_extra::trace::client_layer())
            .discovery()?;
        discover_and_run(&discovery).await?;
        return Ok(());
    }

    let discovery = Client::builder()
        .base_uri(&cli.url)?
        .layer(opensovd_extra::trace::client_layer())
        .discovery()?;
    discover_and_run(&discovery).await?;
    Ok(())
}
