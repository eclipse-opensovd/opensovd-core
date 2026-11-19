// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Command-line interface definitions.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::Context;
use clap::{Args, Parser};
use http::Uri;
use libcli::ServerArgs;

pub const ABOUT: &str = "OpenSOVD Gateway Server";
const UPSTREAM_HEADING: &str = "Upstream Options";

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
    #[command(flatten)]
    pub server: ServerArgs,

    #[command(flatten)]
    pub upstream: UpstreamArgs,

    /// Enable mock entities for testing and development.
    #[cfg(feature = "mock")]
    #[arg(long)]
    pub mock: bool,
}

#[derive(Args)]
pub struct UpstreamArgs {
    /// Base URL of an upstream SOVD server's API, such as the classic
    /// diagnostic adapter at http://localhost:20002/vehicle/v15. Repeat it,
    /// or separate URLs by commas, for several upstreams. To connect over a
    /// Unix domain socket, give unix:<socket>[:<base path>], such as
    /// unix:///run/cda.sock:/vehicle/v15.
    ///
    /// Requests the gateway does not serve itself go to the upstream, and its
    /// responses are returned with their references to it pointing at the
    /// gateway:
    /// entities the gateway does not hold,
    /// functions, custom collections, and locks. The path
    /// below the gateway's API version is appended to this URL. Unset or empty,
    /// the gateway answers everything itself.
    ///
    /// All upstreams serve their entities, whose ids have to be unique across
    /// them; if several have an id, the first of them in the given order
    /// answers for it. A vehicle lock is acquired
    /// on all upstreams. Everything else goes to the first upstream.
    #[arg(
        help_heading = UPSTREAM_HEADING,
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
        help_heading = UPSTREAM_HEADING,
        long = "upstream-authorize-url",
        value_name = "URL",
        env = "SOVD_UPSTREAM_AUTHORIZE_URL"
    )]
    pub authorize_url: Option<String>,

    /// Timeout for connecting to the upstream.
    #[arg(
        help_heading = UPSTREAM_HEADING,
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
        help_heading = UPSTREAM_HEADING,
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
    /// Where the upstream's API is.
    pub endpoint: Endpoint,
    /// Whether this upstream takes `/authorize`, as named by
    /// `--upstream-authorize-url`.
    pub authorize: bool,
}

/// Where an upstream's API is, by how the gateway connects to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Endpoint {
    /// Base URL of the upstream's API, `http://` with a host and no query.
    Tcp(Uri),
    /// An upstream at a Unix domain socket.
    #[cfg(unix)]
    UnixSocket {
        /// Path of the socket, or `@name` for an abstract socket on Linux.
        socket: String,
        /// Path of the upstream's API, such as `/vehicle/v15`, or empty.
        base: String,
    },
}

impl std::fmt::Display for Endpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Tcp(url) => url.fmt(f),
            #[cfg(unix)]
            Self::UnixSocket { socket, base } if base.is_empty() => write!(f, "unix:{socket}"),
            #[cfg(unix)]
            Self::UnixSocket { socket, base } => write!(f, "unix:{socket}:{base}"),
        }
    }
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
                endpoint: url,
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
fn upstream_urls(urls: &[String]) -> anyhow::Result<Vec<Endpoint>> {
    urls.iter()
        .filter(|url| !url.is_empty())
        .map(|url| upstream_url(url))
        .collect()
}

fn upstream_url(url: &str) -> anyhow::Result<Endpoint> {
    #[cfg(unix)]
    if let Some(rest) = url.strip_prefix("unix:") {
        return unix_socket_upstream_url(url, rest);
    }
    #[cfg(not(unix))]
    anyhow::ensure!(
        !url.starts_with("unix:"),
        "--upstream-url {url}: Unix domain sockets are not supported on this platform"
    );
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
    Ok(Endpoint::Tcp(uri))
}

/// An upstream at a Unix domain socket, `unix:<socket>[:<base path>]`, such as
/// `unix:///run/cda.sock:/vehicle/v15`. The socket is an absolute path, or
/// `@name` for an abstract socket on Linux. The base path starts at the first
/// `:/`, so the socket may contain `:` but not `:/`.
#[cfg(unix)]
fn unix_socket_upstream_url(url: &str, rest: &str) -> anyhow::Result<Endpoint> {
    let rest = rest.strip_prefix("//").unwrap_or(rest);
    let (socket, base) = rest
        .split_once(":/")
        .map_or((rest, String::new()), |(socket, base)| {
            (socket, format!("/{base}"))
        });
    let abstract_name = cfg!(target_os = "linux") && socket.starts_with('@');
    anyhow::ensure!(
        socket.starts_with('/') || abstract_name,
        "--upstream-url must name the socket as an absolute path, got {url}"
    );
    let socket = normal_socket(socket, abstract_name);
    anyhow::ensure!(
        socket.len() > 1,
        "--upstream-url must name the socket, got {url}"
    );
    // Built now so that a socket the platform cannot address, such as one too
    // long for it, fails at startup rather than on every request.
    socket_address(&socket).with_context(|| format!("invalid socket in --upstream-url {url}"))?;
    anyhow::ensure!(
        !base.contains(['?', '#']),
        "--upstream-url must not have a query"
    );
    if !base.is_empty() {
        base.parse::<http::uri::PathAndQuery>()
            .with_context(|| format!("invalid base path in --upstream-url {url}"))?;
    }
    Ok(Endpoint::UnixSocket { socket, base })
}

/// `socket` spelled one way only, so that spellings of one socket are the same
/// upstream: a path without `.` segments, repeated `/` or a trailing `/`. A
/// `..` stays, as it may cross a symlink. An abstract name is a name, and
/// stays as it is.
#[cfg(unix)]
fn normal_socket(socket: &str, abstract_name: bool) -> String {
    if abstract_name {
        return socket.to_owned();
    }
    std::path::Path::new(socket)
        .components()
        .collect::<PathBuf>()
        .to_str()
        .map_or_else(|| socket.to_owned(), str::to_owned)
}

/// The address of `socket`, a path or, on Linux, `@name` for an abstract
/// socket.
#[cfg(unix)]
fn socket_address(socket: &str) -> std::io::Result<std::os::unix::net::SocketAddr> {
    #[cfg(target_os = "linux")]
    if let Some(name) = socket.strip_prefix('@') {
        use std::os::linux::net::SocketAddrExt;
        return std::os::unix::net::SocketAddr::from_abstract_name(name);
    }
    std::os::unix::net::SocketAddr::from_pathname(socket)
}

/// The value of `--upstream-authorize-url`. An empty value means none, as for
/// `--upstream-url`.
fn authorize_url(url: Option<&str>) -> anyhow::Result<Option<Endpoint>> {
    url.filter(|url| !url.is_empty())
        .map(|url| {
            upstream_url(url).with_context(|| format!("invalid --upstream-authorize-url {url}"))
        })
        .transpose()
}

/// Whether `a` and `b` are the same upstream: the scheme and the host ignore
/// case (RFC 3986 6.2.2.1), the default port of `http` is the same as none, and
/// a `/` at the end of the path does not count. Sockets are the same by their
/// path.
fn same_url(a: &Endpoint, b: &Endpoint) -> bool {
    let normal = |uri: &Uri| {
        let scheme = uri.scheme_str().unwrap_or_default().to_ascii_lowercase();
        let host = uri.host().unwrap_or_default().to_ascii_lowercase();
        let port = uri
            .port_u16()
            .filter(|port| !(scheme == "http" && *port == 80));
        let path = uri.path().trim_end_matches('/').to_owned();
        (scheme, host, port, path)
    };
    match (a, b) {
        (Endpoint::Tcp(a), Endpoint::Tcp(b)) => normal(a) == normal(b),
        #[cfg(unix)]
        (
            Endpoint::UnixSocket { socket, base },
            Endpoint::UnixSocket {
                socket: other_socket,
                base: other_base,
            },
        ) => {
            socket == other_socket && base.trim_end_matches('/') == other_base.trim_end_matches('/')
        }
        #[cfg(unix)]
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn urls(urls: &[&str]) -> anyhow::Result<Vec<Endpoint>> {
        let urls: Vec<_> = urls.iter().map(ToString::to_string).collect();
        upstream_urls(&urls)
    }

    #[test]
    fn unset_or_empty_urls_mean_no_upstream() {
        assert_eq!(urls(&[]).unwrap(), Vec::<Endpoint>::new());
        assert_eq!(urls(&[""]).unwrap(), Vec::<Endpoint>::new());
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
        let same =
            |a: &str, b: &str| same_url(&upstream_url(a).unwrap(), &upstream_url(b).unwrap());

        assert!(same(
            "http://cda:1/vehicle/v15",
            "HTTP://CDA:1/vehicle/v15/"
        ));
        assert!(same("http://cda/x", "http://cda:80/x"));
        assert!(!same("http://cda:1/x", "http://cda:2/x"));
        assert!(!same("http://cda/x", "http://cda/X"));
        assert!(!same("http://cda/x", "http://other/x"));
        #[cfg(unix)]
        {
            assert!(same("unix:/run/a.sock:/x", "unix:///run/a.sock:/x/"));
            assert!(same("unix:/run/a.sock:/x", "unix:/run/./a.sock:/x"));
            assert!(!same("unix:/run/a.sock:/x", "unix:/run/b.sock:/x"));
            assert!(!same("unix:/run/a.sock:/x", "http://localhost/x"));
        }
    }

    #[test]
    #[cfg(not(unix))]
    fn unix_socket_upstreams_are_refused_without_unix_sockets() {
        assert!(upstream_url("unix:/run/cda.sock:/vehicle/v15").is_err());
    }

    #[test]
    #[cfg(unix)]
    fn unix_socket_upstreams_name_a_socket_and_a_base_path() {
        let unix_socket = |socket: &str, base: &str| Endpoint::UnixSocket {
            socket: socket.to_owned(),
            base: base.to_owned(),
        };

        let endpoint = upstream_url("unix:///run/cda.sock:/vehicle/v15").unwrap();
        assert_eq!(endpoint, unix_socket("/run/cda.sock", "/vehicle/v15"));
        assert_eq!(endpoint.to_string(), "unix:/run/cda.sock:/vehicle/v15");

        let endpoint = upstream_url("unix:/run/cda.sock").unwrap();
        assert_eq!(endpoint, unix_socket("/run/cda.sock", ""));
        assert_eq!(endpoint.to_string(), "unix:/run/cda.sock");

        // A `:` without a `/` after it is part of the socket.
        let endpoint = upstream_url("unix:/run/user:1000/cda.sock:/vehicle/v15").unwrap();
        assert_eq!(
            endpoint,
            unix_socket("/run/user:1000/cda.sock", "/vehicle/v15")
        );
        assert_eq!(
            upstream_url(&endpoint.to_string()).unwrap(),
            endpoint,
            "{endpoint}"
        );

        // A path is spelled one way only, but `..` stays.
        for spelling in [
            "unix:/run/./cda.sock",
            "unix:/run//cda.sock",
            "unix:/run/cda.sock/",
        ] {
            assert_eq!(
                upstream_url(spelling).unwrap(),
                unix_socket("/run/cda.sock", ""),
                "{spelling}"
            );
        }
        assert_eq!(
            upstream_url("unix:/run/x/../cda.sock").unwrap(),
            unix_socket("/run/x/../cda.sock", "")
        );

        // Longer than any platform's `sun_path`.
        let too_long = format!("unix:/{}", "s".repeat(200));

        for url in [
            "unix:",
            "unix:/",
            "unix:/.",
            "unix:run/cda.sock",
            "unix:/run/cda.sock:/v?x=1",
            "unix:/run/cda\0.sock",
            &too_long,
        ] {
            assert!(upstream_url(url).is_err(), "{url}");
        }
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn abstract_unix_socket_upstreams_may_contain_a_colon() {
        assert_eq!(
            upstream_url("unix:@svc:a").unwrap(),
            Endpoint::UnixSocket {
                socket: "@svc:a".to_owned(),
                base: String::new(),
            }
        );

        assert!(upstream_url(&format!("unix:@{}", "s".repeat(200))).is_err());
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
