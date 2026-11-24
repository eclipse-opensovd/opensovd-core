// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context;
use clap::Args;
use http::Uri;
use http::uri::Authority;
use opensovd_server::Listener;

const DEFAULT_URL: &str = "http://127.0.0.1:7690/sovd";

/// The `--url` and `--unix-socket` options.
#[derive(Args)]
#[group(skip)]
#[command(about = None, long_about = None)]
pub struct ListenArgs {
    /// Server URL including base URI path (e.g., http://host:port/path).
    ///
    /// The host:port is used for TCP binding (ignored when using --unix-socket
    /// or systemd socket activation). The path is used as the base URI for all
    /// API routes. An https:// scheme is required with --tls-cert and is
    /// otherwise only advertised, e.g. behind a TLS-terminating proxy.
    #[arg(long, env = "SOVD_URL", default_value = DEFAULT_URL, value_parser = parse_url)]
    url: Uri,

    /// Path to a Unix socket to listen on. Use '@' prefix for abstract sockets.
    /// When specified, the host:port from --url is ignored.
    #[cfg(unix)]
    #[arg(long)]
    unix_socket: Option<String>,
}

fn parse_url(value: &str) -> anyhow::Result<Uri> {
    let url: Uri = value.parse()?;
    anyhow::ensure!(
        url.authority().is_some(),
        "must include host:port (e.g., http://localhost:7690/sovd)"
    );
    Ok(url)
}

impl ListenArgs {
    #[cfg(all(unix, feature = "mdns"))]
    pub(super) fn unix_socket(&self) -> bool {
        self.unix_socket.is_some()
    }

    /// The `--url`.
    #[must_use]
    pub fn url(&self) -> &Uri {
        &self.url
    }

    /// Binds the listener: a socket passed by systemd, the `--unix-socket`,
    /// or the `--url` address.
    ///
    /// # Errors
    ///
    /// Returns an error if the socket cannot be bound.
    pub async fn listen(&self) -> anyhow::Result<Listener> {
        #[cfg(target_os = "linux")]
        if let Some(fd) = sd_notify::listen_fds()?.next() {
            use std::os::fd::FromRawFd;
            // SAFETY: fd is valid and owned, provided by systemd socket activation
            #[expect(unsafe_code)]
            let listener = unsafe { std::net::TcpListener::from_raw_fd(fd) };
            listener.set_nonblocking(true)?;
            return Ok(tokio::net::TcpListener::from_std(listener)?.into());
        }

        #[cfg(unix)]
        if let Some(path) = &self.unix_socket {
            return Ok(bind_unix(path)?.into());
        }

        let authority = self.url.authority().map_or("", Authority::as_str);
        let listener = tokio::net::TcpListener::bind(authority)
            .await
            .with_context(|| format!("failed to bind {authority}"))?;
        Ok(listener.into())
    }
}

#[cfg(unix)]
fn bind_unix(path: &str) -> anyhow::Result<tokio::net::UnixListener> {
    #[cfg(target_os = "linux")]
    if let Some(name) = path.strip_prefix('@') {
        use std::os::linux::net::SocketAddrExt;
        let addr = std::os::unix::net::SocketAddr::from_abstract_name(name)
            .with_context(|| format!("invalid abstract socket name in --unix-socket {path}"))?;
        let listener = std::os::unix::net::UnixListener::bind_addr(&addr)
            .with_context(|| format!("failed to bind abstract unix socket {path}"))?;
        listener.set_nonblocking(true)?;
        return Ok(tokio::net::UnixListener::from_std(listener)?);
    }
    tokio::net::UnixListener::bind(path)
        .with_context(|| format!("failed to bind unix socket {path}"))
}
