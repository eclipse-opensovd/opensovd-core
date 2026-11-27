// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

use clap::error::ErrorKind;
use clap::parser::ValueSource;
use clap::{ArgMatches, Args, Command, FromArgMatches};
use http::Uri;
use opensovd_extra::Announcement;
use opensovd_server::Listener;

const HEADING: &str = "mDNS Options";

/// The `--mdns*` options.
pub struct MdnsArgs(Options);

#[derive(Args)]
#[group(skip)]
struct Options {
    /// Announce via mDNS with this vehicle identification, e.g. the VIN.
    #[arg(
        id = "mdns",
        help_heading = HEADING,
        long = "mdns",
        value_name = "ID",
        env = "SOVD_MDNS"
    )]
    identification: Option<String>,

    /// Host label published as HOST.local. Derived from the identification by
    /// default.
    #[arg(
        id = "mdns_host",
        help_heading = HEADING,
        long = "mdns-host",
        value_name = "HOST",
        env = "SOVD_MDNS_HOST"
    )]
    host: Option<String>,

    /// Announce only on these network interfaces (comma-separated).
    #[arg(
        id = "mdns_interface",
        help_heading = HEADING,
        long = "mdns-interface",
        value_name = "IFNAME",
        env = "SOVD_MDNS_INTERFACE",
        value_delimiter = ','
    )]
    interfaces: Vec<String>,
}

impl FromArgMatches for MdnsArgs {
    fn from_arg_matches(matches: &ArgMatches) -> Result<Self, clap::Error> {
        let options = Options::from_arg_matches(matches)?;
        options.check(matches)?;
        Ok(Self(options))
    }

    fn update_from_arg_matches(&mut self, matches: &ArgMatches) -> Result<(), clap::Error> {
        self.0.update_from_arg_matches(matches)?;
        self.0.check(matches)
    }
}

impl Args for MdnsArgs {
    fn augment_args(cmd: Command) -> Command {
        Options::augment_args(cmd)
    }

    fn augment_args_for_update(cmd: Command) -> Command {
        Options::augment_args_for_update(cmd)
    }
}

impl Options {
    /// The identification, or `None` when mDNS is disabled.
    fn identification(&self) -> Option<&str> {
        self.identification
            .as_deref()
            .map(str::trim)
            .filter(|id| !id.is_empty())
    }

    fn interfaces(&self) -> impl Iterator<Item = &str> {
        self.interfaces.iter().map(|name| name.trim())
    }

    /// Checks the rules clap cannot express: an empty value given on the
    /// command line is an error, while an empty `SOVD_MDNS` disables mDNS.
    fn check(&self, matches: &ArgMatches) -> Result<(), clap::Error> {
        let error = |kind, message: String| Err(clap::Error::raw(kind, message));
        let invalid = |arg: &str, value: &str, reason: &str| {
            error(
                ErrorKind::ValueValidation,
                format!("invalid value '{value}' for '{arg}': {reason}"),
            )
        };
        if matches.value_source("mdns") == Some(ValueSource::CommandLine)
            && self.identification().is_none()
        {
            return invalid("--mdns <ID>", "", "identification must not be empty");
        }
        if self.identification().is_none() {
            for (id, message) in [
                ("mdns_host", "--mdns-host requires --mdns"),
                ("mdns_interface", "--mdns-interface requires --mdns"),
            ] {
                if matches.value_source(id) == Some(ValueSource::CommandLine) {
                    return error(ErrorKind::MissingRequiredArgument, message.to_owned());
                }
            }
            return Ok(());
        }
        if self.interfaces().any(str::is_empty) {
            let names = self.interfaces.join(",");
            return invalid(
                "--mdns-interface <IFNAME>",
                &names,
                "interface name must not be empty",
            );
        }
        Ok(())
    }
}

impl MdnsArgs {
    #[cfg(unix)]
    pub(super) fn enabled(&self) -> bool {
        self.0.identification().is_some()
    }

    /// The mDNS announcement of `listener`, or `None` without `--mdns`.
    ///
    /// `url` gives the advertised path and `tls` whether the server
    /// terminates TLS itself.
    ///
    /// # Errors
    ///
    /// Returns an error if `listener` is a loopback or Unix socket, or if
    /// `url` is https without `tls`.
    pub fn announcement(
        &self,
        listener: &Listener,
        url: &Uri,
        tls: bool,
    ) -> anyhow::Result<Option<Announcement>> {
        let Some(identification) = self.0.identification() else {
            return Ok(None);
        };
        let listener = match listener {
            Listener::Tcp(listener) => listener,
            #[cfg(unix)]
            Listener::Unix(_) => anyhow::bail!("--mdns requires a TCP listener"),
        };
        let addr = listener.local_addr()?;
        if addr.ip().to_canonical().is_loopback() {
            let hint = if socket_activated() {
                "set ListenStream in the systemd socket unit"
            } else {
                "bind --url"
            };
            anyhow::bail!(
                "--mdns cannot announce the loopback address {}; {hint} to 0.0.0.0 or an interface address",
                addr.ip()
            );
        }
        anyhow::ensure!(
            tls || url.scheme_str() != Some("https"),
            "--mdns with an https --url requires --tls-cert, since clients connect to the announced port directly"
        );
        let v6_only = addr.is_ipv6() && socket2::SockRef::from(listener).only_v6()?;

        let mut announcement = Announcement::new(addr)
            .identification(identification)
            .access(if tls { "https" } else { "http" }, url.path())
            .interfaces(self.0.interfaces())
            .ipv6_only(v6_only);
        if let Some(host) = &self.0.host {
            announcement = announcement.host(host.as_str());
        }
        Ok(Some(announcement))
    }
}

#[cfg(target_os = "linux")]
fn socket_activated() -> bool {
    sd_notify::listen_fds().is_ok_and(|mut fds| fds.next().is_some())
}

#[cfg(not(target_os = "linux"))]
fn socket_activated() -> bool {
    false
}
