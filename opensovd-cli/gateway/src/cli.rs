// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Command-line interface definitions.

use std::path::PathBuf;

#[cfg(feature = "tls")]
use clap::ValueEnum;
use clap::{Args, CommandFactory, FromArgMatches, Parser};

pub const ABOUT: &str = "OpenSOVD Gateway Server";
const DEFAULT_URL: &str = "http://localhost:7690/sovd";

const VERSION_STRING: &str = concat!(
    env!("VERSION"),
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

  # Announce via mDNS on the diagnostic port
  opensovd-gateway --url http://0.0.0.0:7690/sovd --mdns ABC123456789 --mdns-interface eth1
")]
pub struct Cli {
    /// Server URL including base URI path (e.g., http://host:port/path).
    ///
    /// The host:port is used for TCP binding (ignored when using --unix-socket
    /// or systemd socket activation). The path is used as the base URI for all
    /// API routes. An https:// scheme is required with --tls-cert and is
    /// otherwise only advertised, e.g. behind a TLS-terminating proxy.
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

    #[cfg(feature = "mdns")]
    #[command(flatten)]
    pub mdns: MdnsArgs,

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
    /// Server TLS certificate chain (PEM). Requires an https:// --url.
    #[arg(
        long = "tls-cert",
        value_name = "FILE",
        env = "SOVD_TLS_CERT",
        requires = "key"
    )]
    pub cert: Option<PathBuf>,

    /// Server TLS private key (PEM).
    #[arg(
        long = "tls-key",
        value_name = "FILE",
        env = "SOVD_TLS_KEY",
        requires = "cert"
    )]
    pub key: Option<PathBuf>,

    /// Client CA certificate bundle (PEM). Setting at least one enables mTLS.
    #[arg(
        long = "tls-client-ca",
        value_name = "FILE",
        env = "SOVD_TLS_CLIENT_CA",
        requires = "cert"
    )]
    pub client_ca: Vec<PathBuf>,

    /// Whether mTLS clients must present a certificate.
    #[arg(
        long = "tls-client-auth",
        value_name = "MODE",
        default_value = "required",
        requires = "client_ca"
    )]
    pub client_auth: ClientAuthMode,
}

#[cfg(feature = "tls")]
#[derive(Clone, Copy, ValueEnum)]
pub enum ClientAuthMode {
    Required,
    /// Accept anonymous clients; verify any certificate that is presented.
    Optional,
}

#[cfg(feature = "tls")]
impl From<ClientAuthMode> for opensovd_extra::ClientAuth {
    fn from(mode: ClientAuthMode) -> Self {
        match mode {
            ClientAuthMode::Required => Self::Required,
            ClientAuthMode::Optional => Self::Optional,
        }
    }
}

#[cfg(feature = "tls")]
impl TlsArgs {
    pub fn config(&self) -> Option<opensovd_extra::ServerTlsConfig> {
        let (cert, key) = (self.cert.as_ref()?, self.key.as_ref()?);
        Some(
            opensovd_extra::ServerTlsConfig::new(cert, key)
                .client_cas(&self.client_ca)
                .client_auth(self.client_auth.into()),
        )
    }
}

#[cfg(feature = "mdns")]
#[derive(Args)]
#[command(next_help_heading = "mDNS Options")]
pub struct MdnsArgs {
    /// Announce via mDNS with this vehicle identification, e.g. the VIN.
    #[arg(id = "mdns", long = "mdns", value_name = "ID", env = "SOVD_MDNS")]
    identification: Option<String>,

    /// Host label published as HOST.local. Derived from the identification by
    /// default.
    #[arg(
        id = "mdns_host",
        long = "mdns-host",
        value_name = "HOST",
        env = "SOVD_MDNS_HOST"
    )]
    pub host: Option<String>,

    /// Announce only on these network interfaces (comma-separated).
    #[arg(
        id = "mdns_interface",
        long = "mdns-interface",
        value_name = "IFNAME",
        env = "SOVD_MDNS_INTERFACE",
        value_delimiter = ','
    )]
    interfaces: Vec<String>,
}

#[cfg(feature = "mdns")]
impl MdnsArgs {
    /// The identification, or `None` when mDNS is disabled.
    pub fn identification(&self) -> Option<&str> {
        self.identification
            .as_deref()
            .map(str::trim)
            .filter(|id| !id.is_empty())
    }

    /// The interface names, trimmed.
    pub fn interfaces(&self) -> impl Iterator<Item = &str> {
        self.interfaces.iter().map(|name| name.trim())
    }
}

/// Parses the command line and checks rules that clap cannot express.
pub fn parse() -> Cli {
    let mut cmd = Cli::command();
    let matches = cmd.get_matches_mut();
    let cli = Cli::from_arg_matches(&matches).unwrap_or_else(|e| e.format(&mut cmd).exit());
    #[cfg(feature = "mdns")]
    if let Err(e) = check_mdns(&cli, &matches) {
        cmd.error(e.0, e.1).exit();
    }
    cli
}

#[cfg(feature = "mdns")]
fn check_mdns(
    cli: &Cli,
    matches: &clap::ArgMatches,
) -> Result<(), (clap::error::ErrorKind, String)> {
    use clap::error::ErrorKind;
    use clap::parser::ValueSource;

    let invalid = |arg: &str, value: &str, reason: &dyn std::fmt::Display| {
        (
            ErrorKind::ValueValidation,
            format!("invalid value '{value}' for '{arg}': {reason}"),
        )
    };
    let enabled = cli.mdns.identification().is_some();
    #[cfg(unix)]
    if enabled && cli.unix_socket.is_some() {
        return Err((
            ErrorKind::ArgumentConflict,
            "--mdns cannot be used with --unix-socket".to_owned(),
        ));
    }
    if !enabled {
        for (id, msg) in [
            ("mdns_host", "--mdns-host requires --mdns"),
            ("mdns_interface", "--mdns-interface requires --mdns"),
        ] {
            if matches.value_source(id) == Some(ValueSource::CommandLine) {
                return Err((ErrorKind::MissingRequiredArgument, msg.to_owned()));
            }
        }
        return Ok(());
    }
    if cli.mdns.interfaces().any(str::is_empty) {
        let names = cli.mdns.interfaces.join(",");
        let reason = "interface name must not be empty";
        return Err(invalid("--mdns-interface <IFNAME>", &names, &reason));
    }
    Ok(())
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
