// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Command-line interface definitions.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::Context;
#[cfg(feature = "tls")]
use clap::ValueEnum;
use clap::builder::NonEmptyStringValueParser;
use clap::{Args, CommandFactory, FromArgMatches, Parser};
use http::Uri;

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

    #[command(flatten)]
    pub upstream: UpstreamArgs,

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
#[command(next_help_heading = "Upstream Options")]
pub struct UpstreamArgs {
    /// Base URL of an upstream SOVD server's API, such as the classic
    /// diagnostic adapter at http://localhost:20002/vehicle/v15. Repeat it,
    /// or separate URLs by commas, for several upstreams.
    ///
    /// Requests the gateway does not serve itself go to the upstream, and its
    /// responses are returned with their references to it pointing at the
    /// gateway:
    /// entities the gateway does not hold,
    /// functions, custom collections, and locks. The path
    /// below the gateway's API version is appended to this URL. Unset or empty,
    /// the gateway answers everything itself.
    ///
    /// All upstreams serve their entities, and if several have an id, the first
    /// of them in the given order answers for it. A vehicle lock is acquired
    /// on all upstreams. Everything else goes to the first upstream.
    #[arg(
        long = "upstream-url",
        value_name = "URL",
        env = "SOVD_UPSTREAM_URL",
        value_delimiter = ','
    )]
    pub upstream_url: Vec<String>,

    /// The --upstream-url that takes `/authorize`, if that upstream issues
    /// tokens. Unset or empty, the gateway answers it, and clients get tokens
    /// from the provider that issues them.
    #[arg(
        long = "upstream-authorize-url",
        value_name = "URL",
        env = "SOVD_UPSTREAM_AUTHORIZE_URL"
    )]
    pub authorize_url: Option<String>,

    /// Timeout for connecting to the upstream.
    #[arg(
        long = "upstream-connect-timeout",
        value_name = "SECONDS",
        env = "SOVD_UPSTREAM_CONNECT_TIMEOUT",
        default_value_t = 5
    )]
    pub upstream_connect_timeout: u64,

    /// Timeout for what the gateway asks an upstream to find out what it has:
    /// `/`, `/components` and `/apps`, which it combines with its own, and,
    /// with several upstreams, which of them has an entity. Without an answer
    /// in time, the lists and `/` go on with the others, and the upstream
    /// answers 504 for an entity no upstream before it has, as it may have
    /// the entity. Other requests have no timeout, as flashing and operations
    /// may run long.
    #[arg(
        long = "upstream-discovery-timeout",
        value_name = "SECONDS",
        env = "SOVD_UPSTREAM_DISCOVERY_TIMEOUT",
        default_value_t = 5
    )]
    pub upstream_discovery_timeout: u64,
}

/// The upstreams of validated [`UpstreamArgs`].
pub struct UpstreamConfig {
    /// The upstreams, in the order of `--upstream-url`, which decides between
    /// equal entity ids and makes the first take what has a single answer.
    pub upstreams: Vec<UpstreamUrl>,
    /// How long to wait for a connection to an upstream.
    pub connect_timeout: Duration,
    /// How long an upstream gets to answer what the gateway asks to find out
    /// what it has.
    pub discovery_timeout: Duration,
}

/// An upstream of an [`UpstreamConfig`].
pub struct UpstreamUrl {
    /// Base URL of the upstream's API, `http://` with a host and no query.
    pub url: Uri,
    /// Whether this upstream takes `/authorize`, as named by
    /// `--upstream-authorize-url`.
    pub authorize: bool,
}

impl UpstreamArgs {
    /// The validated upstreams.
    ///
    /// # Errors
    ///
    /// Returns an error naming the option whose value is invalid.
    pub fn config(&self) -> anyhow::Result<UpstreamConfig> {
        let authorize = authorize_url(self.authorize_url.as_deref())?;
        let urls = upstream_urls(&self.upstream_url)?;
        // The same upstream twice would take a vehicle lock twice, and
        // `/authorize` twice.
        for (at, url) in urls.iter().enumerate() {
            anyhow::ensure!(
                !urls.iter().take(at).any(|before| same_url(before, url)),
                "--upstream-url {url} is given more than once"
            );
        }
        let upstreams = urls
            .into_iter()
            .map(|url| UpstreamUrl {
                authorize: authorize.as_ref().is_some_and(|name| same_url(&url, name)),
                url,
            })
            .collect::<Vec<_>>();
        if let Some(name) = &authorize {
            anyhow::ensure!(
                upstreams.iter().any(|upstream| upstream.authorize),
                "--upstream-authorize-url {name} is not an --upstream-url"
            );
        }
        Ok(UpstreamConfig {
            upstreams,
            connect_timeout: Duration::from_secs(self.upstream_connect_timeout),
            discovery_timeout: Duration::from_secs(self.upstream_discovery_timeout),
        })
    }
}

/// The URLs of `--upstream-url`, if there are upstreams. Empty values mean
/// none, as a deployment may set `SOVD_UPSTREAM_URL` to an empty string to
/// leave it out.
fn upstream_urls(urls: &[String]) -> anyhow::Result<Vec<Uri>> {
    urls.iter()
        .filter(|url| !url.is_empty())
        .map(|url| upstream_url(url))
        .collect()
}

fn upstream_url(url: &str) -> anyhow::Result<Uri> {
    let uri: Uri = url
        .parse()
        .with_context(|| format!("invalid --upstream-url {url}"))?;
    anyhow::ensure!(
        uri.scheme_str() == Some("http"),
        "--upstream-url must be an http:// URL, got {url}"
    );
    anyhow::ensure!(
        uri.query().is_none(),
        "--upstream-url must not have a query"
    );
    anyhow::ensure!(
        uri.authority().is_some(),
        "--upstream-url must include host:port, got {url}"
    );
    Ok(uri)
}

/// The value of `--upstream-authorize-url`. An empty value means none, as for
/// `--upstream-url`.
fn authorize_url(url: Option<&str>) -> anyhow::Result<Option<Uri>> {
    url.filter(|url| !url.is_empty())
        .map(|url| {
            url.parse()
                .with_context(|| format!("invalid --upstream-authorize-url {url}"))
        })
        .transpose()
}

/// Whether `a` and `b` are the same URL: the scheme and the host ignore case
/// (RFC 3986 6.2.2.1), the default port of `http` is the same as none, and a
/// `/` at the end of the path does not count.
fn same_url(a: &Uri, b: &Uri) -> bool {
    let normal = |uri: &Uri| {
        let scheme = uri.scheme_str().unwrap_or_default().to_ascii_lowercase();
        let host = uri.host().unwrap_or_default().to_ascii_lowercase();
        let port = uri
            .port_u16()
            .filter(|port| !(scheme == "http" && *port == 80));
        let path = uri.path().trim_end_matches('/').to_owned();
        (scheme, host, port, path)
    };
    normal(a) == normal(b)
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
    if matches.value_source("mdns") == Some(ValueSource::CommandLine)
        && cli.mdns.identification().is_none()
    {
        let reason = "identification must not be empty";
        return Err(invalid("--mdns <ID>", "", &reason));
    }
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
    /// JSON Web Key Set with the public keys that sign tokens; a token names its key with `kid`.
    #[arg(
        long = "auth-jwks",
        value_name = "FILE",
        env = "SOVD_AUTH_JWKS",
        requires = "issuer"
    )]
    pub jwks: Option<PathBuf>,

    /// The `iss` (issuer) claim tokens must carry.
    #[arg(
        long = "auth-issuer",
        value_name = "ISSUER",
        env = "SOVD_AUTH_ISSUER",
        value_parser = NonEmptyStringValueParser::new(),
        requires = "jwks"
    )]
    pub issuer: Option<String>,

    /// An `aud` (audience) claim value accepted in tokens, repeatable; without one, `aud` is not checked.
    #[arg(
        long = "auth-audience",
        value_name = "AUDIENCE",
        env = "SOVD_AUTH_AUDIENCE",
        value_delimiter = ',',
        value_parser = NonEmptyStringValueParser::new(),
        requires = "jwks"
    )]
    pub audience: Vec<String>,

    /// Rego policy file defining `data.sovd.authz.allow`.
    #[arg(
        long = "auth-policy",
        value_name = "FILE",
        env = "SOVD_AUTH_POLICY",
        requires = "jwks"
    )]
    pub policy: Vec<PathBuf>,

    /// JSON data file for Rego policies.
    #[arg(
        long = "auth-policy-data",
        value_name = "FILE",
        env = "SOVD_AUTH_POLICY_DATA",
        requires = "policy"
    )]
    pub policy_data: Vec<PathBuf>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn urls(urls: &[&str]) -> anyhow::Result<Vec<Uri>> {
        let urls: Vec<_> = urls.iter().map(ToString::to_string).collect();
        upstream_urls(&urls)
    }

    #[test]
    fn unset_or_empty_urls_mean_no_upstream() {
        assert_eq!(urls(&[]).unwrap(), Vec::<Uri>::new());
        assert_eq!(urls(&[""]).unwrap(), Vec::<Uri>::new());
        assert_eq!(urls(&["http://a:1", "", "http://b:2"]).unwrap().len(), 2);
        assert!(urls(&["http://["]).is_err());
    }

    #[test]
    fn only_http_urls_with_a_host_are_accepted() {
        for url in [
            "https://cda:20002/vehicle/v15",
            "/vehicle/v15",
            "http://cda/v?x=1",
        ] {
            assert!(upstream_url(url).is_err(), "{url}");
        }
    }

    #[test]
    fn the_upstream_of_authorize_has_to_be_an_upstream() {
        let args = |authorize: Option<&str>| UpstreamArgs {
            upstream_url: vec!["http://a:1/x".to_owned(), "http://b:1/x".to_owned()],
            authorize_url: authorize.map(str::to_owned),
            upstream_connect_timeout: 5,
            upstream_discovery_timeout: 5,
        };
        let authorized = |authorize: Option<&str>| -> Vec<bool> {
            let config = args(authorize).config().unwrap();
            config.upstreams.iter().map(|u| u.authorize).collect()
        };

        assert_eq!(authorized(None), [false, false]);
        assert_eq!(authorized(Some("")), [false, false]);
        assert_eq!(authorized(Some("http://b:1/x/")), [false, true]);
        assert_eq!(authorized(Some("HTTP://B:1/x")), [false, true]);
        let error = args(Some("http://c:1/x")).config().err().unwrap();
        assert_eq!(
            error.to_string(),
            "--upstream-authorize-url http://c:1/x is not an --upstream-url"
        );
    }

    #[test]
    fn urls_are_the_same_by_their_meaning() {
        let same = |a: &str, b: &str| same_url(&a.parse().unwrap(), &b.parse().unwrap());

        assert!(same(
            "http://cda:1/vehicle/v15",
            "HTTP://CDA:1/vehicle/v15/"
        ));
        assert!(same("http://cda/x", "http://cda:80/x"));
        assert!(!same("http://cda:1/x", "http://cda:2/x"));
        assert!(!same("http://cda/x", "http://cda/X"));
        assert!(!same("http://cda/x", "http://other/x"));
    }

    #[test]
    fn an_upstream_may_be_given_once_only() {
        let config = |urls: &[&str]| {
            UpstreamArgs {
                upstream_url: urls.iter().map(ToString::to_string).collect(),
                authorize_url: None,
                upstream_connect_timeout: 5,
                upstream_discovery_timeout: 5,
            }
            .config()
        };

        assert!(config(&["http://a:1/x", "http://b:1/x"]).is_ok());
        let error = config(&["http://a:1/x", "http://A:1/x/"]).err().unwrap();
        assert_eq!(
            error.to_string(),
            "--upstream-url http://A:1/x/ is given more than once"
        );
    }
}
