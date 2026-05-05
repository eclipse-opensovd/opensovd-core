// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Command-line interface definitions.

use std::path::PathBuf;

#[cfg(feature = "tls")]
use anyhow::Context;
use clap::{Args, Parser};

pub const ABOUT: &str = "OpenSOVD Gateway Server";
const DEFAULT_URL: &str = "http://localhost:7690/sovd";

const VERSION_STRING: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    " (",
    env!("COMMIT_SHA"),
    " ",
    env!("BUILD_DATE"),
    ")"
);

#[derive(Parser)]
#[command(name = "opensovd-gateway")]
#[command(version = VERSION_STRING)]
#[command(about = ABOUT)]
#[command(after_help = "\
Examples:
  # Listen on all interfaces on port 8080
  opensovd-gateway --url http://0.0.0.0:8080/sovd

  # Custom base URI path
  opensovd-gateway --url http://localhost:7690/api/sovd

  # Listen on a Unix socket (filesystem path)
  opensovd-gateway --unix-socket /tmp/opensovd.sock

  # Listen on an abstract Unix socket
  opensovd-gateway --unix-socket @opensovd
")]
pub struct Cli {
    /// Server URL including base URI path (e.g., http://host:port/path).
    ///
    /// The host:port is used for TCP binding (ignored when using --unix-socket
    /// or systemd socket activation). The path is used as the base URI for all
    /// API routes.
    #[arg(long, env = "SOVD_URL", default_value = DEFAULT_URL)]
    pub url: String,

    /// Path to a Unix socket to listen on. Use '@' prefix for abstract sockets.
    /// When specified, the host:port from --url is ignored.
    #[cfg(unix)]
    #[arg(long)]
    pub unix_socket: Option<String>,

    #[command(flatten)]
    pub cors: CorsArgs,

    #[command(flatten)]
    pub auth: AuthArgs,

    #[cfg(feature = "tls")]
    #[command(flatten)]
    pub tls: TlsArgs,

    /// Enable mock entities for testing and development.
    #[arg(help_heading = "Options")]
    #[cfg(feature = "mock")]
    #[arg(long)]
    pub mock: bool,

    /// Serve static files from a directory.
    /// Format: PATH:DIRECTORY (e.g., "/ui:./webui/dist")
    #[arg(long, help_heading = "Options")]
    pub serve_dir: Option<String>,
}

#[derive(Args)]
#[command(next_help_heading = "CORS Options")]
pub struct CorsArgs {
    /// Allowed CORS origins. Use '*' for any origin.
    #[arg(long = "cors-origin", value_name = "ORIGIN")]
    pub origins: Vec<String>,

    /// Allowed CORS methods. Use '*' for any method.
    #[arg(long = "cors-method", value_name = "METHOD")]
    pub methods: Vec<String>,

    /// Allowed CORS headers. Use '*' for any header.
    #[arg(long = "cors-header", value_name = "HEADER")]
    pub headers: Vec<String>,

    /// Allow credentials in CORS requests.
    #[arg(long = "cors-credentials")]
    pub credentials: bool,

    /// Max age for CORS preflight cache in seconds.
    #[arg(long = "cors-max-age", value_name = "SECONDS")]
    pub max_age: Option<u64>,
}

#[cfg(feature = "tls")]
#[derive(Args)]
#[command(next_help_heading = "TLS Options")]
pub struct TlsArgs {
    // path to the server TLS certificate (PEM format).
    #[arg(long = "tls-cert", value_name = "FILE", env = "SOVD_TLS_CERT")]
    pub cert: Option<std::path::PathBuf>,

    // path to the server TLS private key (PEM format).
    #[arg(long = "tls-key", value_name = "FILE", env = "SOVD_TLS_KEY")]
    pub key: Option<std::path::PathBuf>,

    // one or more client CA cert files set, mTLS is enabled
    #[arg(
        long = "tls-client-ca",
        value_name = "FILE",
        env = "SOVD_TLS_CLIENT_CA"
    )]
    pub client_ca: Vec<std::path::PathBuf>,
}

#[cfg(feature = "tls")]
impl TlsArgs {
    // returns a rustls::ServerConfig if cert+key are provided, otherwise None
    pub fn build(self) -> anyhow::Result<Option<rustls::ServerConfig>> {
        use std::sync::Arc;

        use rustls::pki_types::pem::PemObject;
        use rustls::pki_types::{CertificateDer, PrivateKeyDer};

        let (cert_path, key_path) = match (self.cert, self.key) {
            (Some(c), Some(k)) => (c, k),
            (None, None) => return Ok(None),
            _ => anyhow::bail!("--tls-cert and --tls-key must both be provided"),
        };

        let certs: Vec<CertificateDer<'static>> = CertificateDer::pem_file_iter(&cert_path)
            .with_context(|| format!("failed to read {}", cert_path.display()))?
            .collect::<Result<_, _>>()
            .with_context(|| format!("failed to parse {}", cert_path.display()))?;
        anyhow::ensure!(
            !certs.is_empty(),
            "no certificates found in {}",
            cert_path.display()
        );

        let key = PrivateKeyDer::from_pem_file(&key_path)
            .with_context(|| format!("failed to read private key from {}", key_path.display()))?;

        let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
        let builder = rustls::ServerConfig::builder_with_provider(Arc::clone(&provider))
            .with_safe_default_protocol_versions()
            .context("rustls protocol version setup")?;

        let config = if self.client_ca.is_empty() {
            builder
                .with_no_client_auth()
                .with_single_cert(certs, key)
                .context("rustls server config")?
        } else {
            let mut roots = rustls::RootCertStore::empty();
            for ca in &self.client_ca {
                for cert in CertificateDer::pem_file_iter(ca)
                    .with_context(|| format!("failed to read {}", ca.display()))?
                {
                    let cert = cert.with_context(|| format!("failed to parse {}", ca.display()))?;
                    roots
                        .add(cert)
                        .with_context(|| format!("invalid CA cert in {}", ca.display()))?;
                }
            }
            let verifier = rustls::server::WebPkiClientVerifier::builder_with_provider(
                Arc::new(roots),
                provider,
            )
            .build()
            .context("client cert verifier")?;
            builder
                .with_client_cert_verifier(verifier)
                .with_single_cert(certs, key)
                .context("rustls server config")?
        };

        Ok(Some(config))
    }
}

#[derive(Args)]
#[command(next_help_heading = "Authentication & Authorization")]
pub struct AuthArgs {
    /// Base64-encoded key for JWT validation (HMAC secret or RSA public key in PKCS#1 DER).
    #[arg(
        long = "auth-jwt-secret",
        value_name = "SECRET",
        env = "SOVD_JWT_SECRET"
    )]
    pub jwt_key: Option<String>,

    /// JWT signing algorithm (HS512 or RS512). Defaults to HS512.
    #[arg(
        long = "auth-jwt-algo",
        value_name = "ALGORITHM",
        default_value = "HS512"
    )]
    pub jwt_algo: String,

    /// Expected `iss` (issuer) claim in JWT tokens.
    #[arg(
        long = "auth-jwt-issuer",
        value_name = "ISSUER",
        default_value = "OpenSOVD"
    )]
    pub jwt_issuer: String,

    /// Rego policy file.
    #[arg(long = "auth-policy", value_name = "FILE")]
    pub policy: Vec<PathBuf>,

    /// JSON data file for Rego policies.
    #[arg(long = "auth-policy-data", value_name = "FILE")]
    pub policy_data: Vec<PathBuf>,
}
