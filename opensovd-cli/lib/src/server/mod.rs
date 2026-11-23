// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Command-line options for an SOVD server.

mod auth;
mod cors;
mod listen;
#[cfg(feature = "mdns")]
mod mdns;
#[cfg(feature = "tls")]
mod tls;

use anyhow::Context;
use clap::{ArgMatches, Args, Command, FromArgMatches};
use http::Uri;
#[cfg(feature = "mdns")]
use opensovd_extra::Announcement;
use opensovd_server::{Auth, Listener};
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;

pub use self::auth::AuthArgs;
pub use self::cors::CorsArgs;
pub use self::listen::ListenArgs;
#[cfg(feature = "mdns")]
pub use self::mdns::MdnsArgs;
#[cfg(feature = "tls")]
pub use self::tls::TlsArgs;
use crate::TARGET;

/// All server options, plus `--serve-dir`.
///
/// Flatten it into a binary's [`clap::Parser`], or flatten only the groups
/// the binary needs. Only `ServerArgs` rejects `--mdns` with `--unix-socket`
/// while parsing.
pub struct ServerArgs(Options);

#[derive(Args)]
#[group(skip)]
struct Options {
    #[command(flatten)]
    listen: ListenArgs,

    /// Serve static files from a directory.
    /// Format: PATH:DIRECTORY (e.g., "/ui:./webui/dist")
    #[arg(long)]
    serve_dir: Option<String>,

    #[command(flatten)]
    cors: CorsArgs,

    #[command(flatten)]
    auth: AuthArgs,

    #[cfg(feature = "tls")]
    #[command(flatten)]
    tls: TlsArgs,

    #[cfg(feature = "mdns")]
    #[command(flatten)]
    mdns: MdnsArgs,
}

impl FromArgMatches for ServerArgs {
    fn from_arg_matches(matches: &ArgMatches) -> Result<Self, clap::Error> {
        let options = Options::from_arg_matches(matches)?;
        #[cfg(all(unix, feature = "mdns"))]
        options.check()?;
        Ok(Self(options))
    }

    fn update_from_arg_matches(&mut self, matches: &ArgMatches) -> Result<(), clap::Error> {
        self.0.update_from_arg_matches(matches)?;
        #[cfg(all(unix, feature = "mdns"))]
        self.0.check()?;
        Ok(())
    }
}

impl Args for ServerArgs {
    fn augment_args(cmd: Command) -> Command {
        Options::augment_args(cmd)
    }

    fn augment_args_for_update(cmd: Command) -> Command {
        Options::augment_args_for_update(cmd)
    }
}

#[cfg(all(unix, feature = "mdns"))]
impl Options {
    fn check(&self) -> Result<(), clap::Error> {
        if self.mdns.enabled() && self.listen.unix_socket() {
            return Err(clap::Error::raw(
                clap::error::ErrorKind::ArgumentConflict,
                "--mdns cannot be used with --unix-socket",
            ));
        }
        Ok(())
    }
}

impl ServerArgs {
    /// The `--url`.
    #[must_use]
    pub fn url(&self) -> &Uri {
        self.0.listen.url()
    }

    /// See [`AuthArgs::auth`].
    ///
    /// # Errors
    ///
    /// Returns an error if the key set or a policy cannot be loaded.
    pub fn auth(&self) -> anyhow::Result<Option<Auth>> {
        self.0.auth.auth()
    }

    /// Whether `--auth-jwks` asks for the auth that [`Self::auth`] builds.
    #[must_use]
    pub fn auth_enabled(&self) -> bool {
        self.0.auth.enabled()
    }

    /// See [`TlsArgs::config`].
    ///
    /// # Errors
    ///
    /// Returns an error if the `--url` is not https or the certificates
    /// cannot be loaded.
    #[cfg(feature = "tls")]
    pub fn tls(&self) -> anyhow::Result<Option<rustls::ServerConfig>> {
        self.0.tls.config(self.url())
    }

    /// See [`ListenArgs::listen`].
    ///
    /// # Errors
    ///
    /// Returns an error if the socket cannot be bound.
    pub async fn listen(&self) -> anyhow::Result<Listener> {
        self.0.listen.listen().await
    }

    /// See [`MdnsArgs::announcement`].
    ///
    /// # Errors
    ///
    /// Returns an error if `listener` is a loopback or Unix socket, or if
    /// the `--url` is https without TLS.
    #[cfg(feature = "mdns")]
    pub fn mdns(&self, listener: &Listener) -> anyhow::Result<Option<Announcement>> {
        #[cfg(feature = "tls")]
        let tls = self.0.tls.enabled();
        #[cfg(not(feature = "tls"))]
        let tls = false;
        self.0.mdns.announcement(listener, self.url(), tls)
    }

    /// See [`CorsArgs::layer`].
    #[must_use]
    pub fn cors(&self) -> Option<CorsLayer> {
        self.0.cors.layer()
    }

    /// The `--serve-dir` path and the service for its directory.
    ///
    /// # Errors
    ///
    /// Returns an error if the value is not in `PATH:DIRECTORY` form.
    pub fn serve_dir(&self) -> anyhow::Result<Option<(&str, ServeDir)>> {
        let Some(value) = &self.0.serve_dir else {
            return Ok(None);
        };
        let (path, dir) = value
            .split_once(':')
            .context("--serve-dir format: PATH:DIRECTORY (e.g., /ui:./webui/dist)")?;
        tracing::info!(target: TARGET, %path, %dir, "Serving static files");
        Ok(Some((path, ServeDir::new(dir))))
    }
}

/// Tells systemd that the server is ready.
pub fn notify_ready() {
    #[cfg(target_os = "linux")]
    if let Err(e) = sd_notify::notify(&[sd_notify::NotifyState::Ready]) {
        tracing::warn!(target: TARGET, error = %e, "Failed to notify systemd readiness");
    }
}

#[cfg(test)]
mod tests {
    use clap::error::ErrorKind;
    use clap::{CommandFactory, Parser};

    use super::*;

    #[derive(Parser)]
    struct Cli {
        #[command(flatten)]
        server: ServerArgs,

        #[arg(long)]
        key: Option<String>,

        #[arg(long)]
        headers: Vec<String>,

        #[arg(long)]
        policy: Option<String>,
    }

    fn parse<P: Parser>(args: &[&str]) -> Result<P, ErrorKind> {
        P::try_parse_from(std::iter::once("test").chain(args.iter().copied())).map_err(|e| e.kind())
    }

    #[test]
    fn ids_do_not_clash_with_common_field_names() {
        Cli::command().debug_assert();
    }

    fn assert_no_about<A: Args>() {
        let cmd = A::augment_args(Command::new("test"));
        assert!(cmd.get_about().is_none());
        assert!(cmd.get_long_about().is_none());
    }

    #[test]
    fn doc_comments_stay_out_of_the_about_text() {
        assert_no_about::<ServerArgs>();
        assert_no_about::<ListenArgs>();
        assert_no_about::<CorsArgs>();
        assert_no_about::<AuthArgs>();
        #[cfg(feature = "tls")]
        assert_no_about::<TlsArgs>();
        #[cfg(feature = "mdns")]
        assert_no_about::<MdnsArgs>();
    }

    #[test]
    fn heading_does_not_leak_into_later_fields() {
        let cmd = Cli::command();
        let key = cmd.get_arguments().find(|arg| arg.get_id() == "key");
        assert_eq!(key.map(clap::Arg::get_help_heading), Some(None));
    }

    #[test]
    fn rejects_url_without_authority() {
        assert_eq!(
            parse::<Cli>(&["--url", "/sovd"]).err(),
            Some(ErrorKind::ValueValidation)
        );
    }

    #[test]
    fn rejects_cors_wildcard_with_credentials() {
        assert_eq!(
            parse::<Cli>(&["--cors-origin", "*", "--cors-credentials"]).err(),
            Some(ErrorKind::ValueValidation)
        );
    }

    #[cfg(all(unix, feature = "mdns"))]
    #[test]
    fn rejects_mdns_with_unix_socket() {
        assert_eq!(
            parse::<Cli>(&["--mdns", "VIN", "--unix-socket", "gw.sock"]).err(),
            Some(ErrorKind::ArgumentConflict)
        );
    }

    #[cfg(feature = "mdns")]
    #[test]
    fn mdns_args_work_alone() {
        #[derive(Parser)]
        struct Cli {
            #[command(flatten)]
            mdns: MdnsArgs,
        }

        Cli::command().debug_assert();
        assert!(parse::<Cli>(&["--mdns", "VIN"]).is_ok());
        assert_eq!(
            parse::<Cli>(&["--mdns", " "]).err(),
            Some(ErrorKind::ValueValidation)
        );
    }
}
