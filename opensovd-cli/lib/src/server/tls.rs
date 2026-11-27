// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use anyhow::Context;
use clap::{Args, ValueEnum};
use http::Uri;
use opensovd_extra::{ClientAuth, ServerTlsConfig};

use crate::TARGET;

const HEADING: &str = "TLS Options";

/// The `--tls-*` options.
#[derive(Args)]
#[group(skip)]
#[command(about = None, long_about = None)]
pub struct TlsArgs {
    /// Server TLS certificate chain (PEM). Requires an https:// --url.
    #[arg(
        id = "tls_cert",
        help_heading = HEADING,
        long = "tls-cert",
        value_name = "FILE",
        env = "SOVD_TLS_CERT",
        requires = "tls_key"
    )]
    cert: Option<PathBuf>,

    /// Server TLS private key (PEM).
    #[arg(
        id = "tls_key",
        help_heading = HEADING,
        long = "tls-key",
        value_name = "FILE",
        env = "SOVD_TLS_KEY",
        requires = "tls_cert"
    )]
    key: Option<PathBuf>,

    /// Client CA certificate bundle (PEM). Setting at least one enables mTLS.
    #[arg(
        id = "tls_client_ca",
        help_heading = HEADING,
        long = "tls-client-ca",
        value_name = "FILE",
        env = "SOVD_TLS_CLIENT_CA",
        requires = "tls_cert"
    )]
    client_ca: Vec<PathBuf>,

    /// Whether mTLS clients must present a certificate.
    #[arg(
        id = "tls_client_auth",
        help_heading = HEADING,
        long = "tls-client-auth",
        value_name = "MODE",
        default_value = "required",
        requires = "tls_client_ca"
    )]
    client_auth: ClientAuthMode,
}

#[derive(Clone, Copy, ValueEnum)]
enum ClientAuthMode {
    Required,
    /// Accept anonymous clients; verify any certificate that is presented.
    Optional,
}

impl From<ClientAuthMode> for ClientAuth {
    fn from(mode: ClientAuthMode) -> Self {
        match mode {
            ClientAuthMode::Required => Self::Required,
            ClientAuthMode::Optional => Self::Optional,
        }
    }
}

impl TlsArgs {
    /// Whether `--tls-cert` is set.
    #[must_use]
    pub fn enabled(&self) -> bool {
        self.cert.is_some()
    }

    /// The server TLS configuration, or `None` without `--tls-cert`.
    ///
    /// # Errors
    ///
    /// Returns an error if `url` is not https or the certificates cannot be
    /// loaded.
    pub fn config(&self, url: &Uri) -> anyhow::Result<Option<rustls::ServerConfig>> {
        let (Some(cert), Some(key)) = (&self.cert, &self.key) else {
            return Ok(None);
        };
        anyhow::ensure!(
            url.scheme_str() == Some("https"),
            "--tls-cert requires an https:// --url"
        );
        let config = ServerTlsConfig::new(cert, key)
            .client_cas(&self.client_ca)
            .client_auth(self.client_auth.into());
        let mtls = config.is_mtls();
        let config = config.build().context("failed to configure TLS")?;
        tracing::info!(target: TARGET, mtls, "TLS enabled");
        Ok(Some(config))
    }
}
