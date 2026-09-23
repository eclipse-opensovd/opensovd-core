// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! mDNS and DNS-SD announcement of an SOVD server (ISO 17978-3, 5.11).
//!
//! ```no_run
//! # async fn run() -> Result<(), opensovd_extra::MdnsError> {
//! use opensovd_extra::Announcement;
//!
//! let addr = "0.0.0.0:7690".parse().unwrap();
//! let announcer = Announcement::new(addr)
//!     .identification("ABC123456789")
//!     .access("https", "/sovd")
//!     .announce()?;
//! // serve ...
//! announcer.shutdown().await;
//! # Ok(())
//! # }
//! ```

use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use mdns_sd::{
    DaemonEvent, IfKind, RRType, Receiver, ServiceDaemon, ServiceInfo, TxtProperty,
    UnregisterStatus,
};

/// DNS-SD service type of an SOVD server.
pub const SERVICE_TYPE: &str = "_sovd._tcp.local.";

const TARGET: &str = "mdns";
const FALLBACK_HOST: &str = "opensovd";
const MAX_HOST_LEN: usize = 60;
const LONGEST_RENAME: &str = "-99";
const UNREGISTER_TIMEOUT: Duration = Duration::from_secs(1);
const ANNOUNCE_TIMEOUT: Duration = Duration::from_secs(5);
const GOODBYE_REPEAT: Duration = Duration::from_millis(150);

/// Error returned when a service cannot be announced.
#[derive(Debug, thiserror::Error)]
pub enum MdnsError {
    #[error("invalid mDNS host {0:?}: expected 1-60 letters, digits or inner hyphens")]
    InvalidHost(String),
    #[error("none of the interfaces {interfaces} carries the bound address {ip}")]
    InterfaceMismatch { ip: IpAddr, interfaces: String },
    #[error("failed to list network interfaces")]
    Interfaces(#[source] std::io::Error),
    #[error("mDNS announcement requires a tokio runtime")]
    NoRuntime,
    #[error("mDNS accessurl {0:?} does not fit into a TXT record after a rename")]
    AccessUrlTooLong(String),
    #[error(transparent)]
    Daemon(#[from] mdns_sd::Error),
}

/// Checks that `host` is a single DNS label usable as `<host>.local`.
///
/// The limit is 60 characters so that a rename after a conflict, e.g.
/// `-2`, still fits into a 63 byte label.
///
/// # Errors
///
/// Returns [`MdnsError::InvalidHost`] when the label is empty, too long, or
/// contains anything but ASCII letters, digits and inner hyphens.
pub fn validate_host(host: &str) -> Result<(), MdnsError> {
    let valid = !host.is_empty()
        && host.len() <= MAX_HOST_LEN
        && !host.starts_with('-')
        && !host.ends_with('-')
        && host.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-');
    if valid {
        Ok(())
    } else {
        Err(MdnsError::InvalidHost(host.to_owned()))
    }
}

/// Describes the SOVD server to announce.
#[derive(Debug, Clone)]
pub struct Announcement {
    addr: SocketAddr,
    host: Option<String>,
    identification: Option<String>,
    scheme: String,
    path: String,
    interfaces: Vec<String>,
    ipv6_only: bool,
}

impl Announcement {
    /// Announces the server bound to `addr`.
    ///
    /// The instance name is the host label in lower case.
    ///
    /// `0.0.0.0` announces all non-loopback IPv4 addresses and `[::]` all
    /// non-loopback addresses of both families, or only IPv6 with
    /// [`Announcement::ipv6_only`]. Any other IP is announced on its own
    /// interface only. All of them follow interface and address changes.
    #[must_use]
    pub fn new(addr: SocketAddr) -> Self {
        Self {
            addr: SocketAddr::new(addr.ip().to_canonical(), addr.port()),
            host: None,
            identification: None,
            scheme: "https".to_owned(),
            path: String::new(),
            interfaces: Vec::new(),
            ipv6_only: false,
        }
    }

    /// Sets the host label published as `<host>.local`.
    ///
    /// Defaults to the identification with spaces and underscores turned into
    /// hyphens and other characters dropped. Without an identification, or if
    /// nothing is left, the system host name is used.
    #[must_use]
    pub fn host(mut self, host: impl Into<String>) -> Self {
        self.host = Some(host.into());
        self
    }

    /// Sets the `identification` TXT value, e.g. the VIN.
    #[must_use]
    pub fn identification(mut self, identification: impl Into<String>) -> Self {
        self.identification = Some(identification.into());
        self
    }

    /// Sets the scheme and base path of the `accessurl` TXT value.
    ///
    /// The path is the parent of `version-info`. Defaults to `https` and an
    /// empty path.
    #[must_use]
    pub fn access(mut self, scheme: &str, path: &str) -> Self {
        scheme.clone_into(&mut self.scheme);
        let path = path.trim_matches('/');
        self.path = if path.is_empty() {
            String::new()
        } else {
            format!("/{path}")
        };
        self
    }

    /// Limits the announcement to the named network interfaces.
    ///
    /// Only applies to an unspecified bind address. A specific IP is always
    /// announced on its own interface, which must be in this list.
    #[must_use]
    pub fn interfaces<I, S>(mut self, interfaces: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.interfaces = interfaces.into_iter().map(Into::into).collect();
        self
    }

    /// Marks an unspecified IPv6 address as IPv6 only, so no IPv4 address is
    /// announced.
    #[must_use]
    pub fn ipv6_only(mut self, ipv6_only: bool) -> Self {
        self.ipv6_only = ipv6_only;
        self
    }

    /// Starts the responder and registers the service.
    ///
    /// Must be called within a tokio runtime, which runs the task that
    /// follows renames and announcements.
    ///
    /// # Errors
    ///
    /// Fails when the host is invalid, the interfaces do not match the bound
    /// address, no tokio runtime is running, or the responder cannot start.
    pub fn announce(self) -> Result<MdnsAnnouncer, MdnsError> {
        let host = self.resolve_host()?;
        self.check_interfaces()?;
        let instance = host.to_ascii_lowercase();
        self.check_access_url(&host)?;
        let info = self.service_info(&instance, &host)?;
        let fullname = info.get_fullname().to_owned();

        let runtime = tokio::runtime::Handle::try_current().map_err(|_| MdnsError::NoRuntime)?;
        let daemon = ServiceDaemon::new()?;
        let events = match self.start(&daemon, info) {
            Ok(events) => events,
            Err(e) => {
                let _ = daemon.shutdown();
                return Err(e);
            }
        };
        self.log_enabled(&instance, &host);

        let current = Arc::new(Mutex::new(host));
        let stopping = Arc::new(AtomicBool::new(false));
        let current_fullname = Arc::new(Mutex::new(fullname.clone()));
        let watcher = Watcher {
            fullname: Arc::clone(&current_fullname),
            announcement: self.clone(),
            instance: instance.clone(),
            daemon: daemon.clone(),
            host: Arc::clone(&current),
            stopping: Arc::clone(&stopping),
        };
        runtime.spawn(watcher.run(events));

        Ok(MdnsAnnouncer {
            daemon: Some(daemon),
            registered: fullname,
            fullname: current_fullname,
            instance,
            host: current,
            stopping,
        })
    }

    fn start(
        &self,
        daemon: &ServiceDaemon,
        info: ServiceInfo,
    ) -> Result<Receiver<DaemonEvent>, MdnsError> {
        let ip = self.addr.ip();
        if !ip.is_unspecified() {
            daemon.disable_interface(IfKind::All)?;
            daemon.enable_interface(IfKind::Addr(ip))?;
        } else if !self.interfaces.is_empty() {
            daemon.disable_interface(IfKind::All)?;
            daemon.enable_interface(self.if_names())?;
        }
        if !ip.is_loopback() {
            daemon.disable_interface(vec![IfKind::LoopbackV4, IfKind::LoopbackV6])?;
        }
        if ip.is_unspecified() && ip.is_ipv4() {
            daemon.disable_interface(IfKind::IPv6)?;
        }
        if ip.is_unspecified() && ip.is_ipv6() && self.ipv6_only {
            daemon.disable_interface(IfKind::IPv4)?;
        }
        let events = daemon.monitor()?;
        daemon.register(info)?;
        Ok(events)
    }

    fn resolve_host(&self) -> Result<String, MdnsError> {
        if let Some(host) = &self.host {
            validate_host(host)?;
            return Ok(host.clone());
        }
        Ok(self
            .identification
            .as_deref()
            .and_then(host_from)
            .unwrap_or_else(system_host))
    }

    fn check_interfaces(&self) -> Result<(), MdnsError> {
        if self.interfaces.is_empty() {
            return Ok(());
        }
        let ip = self.addr.ip();
        let intfs = if_addrs::get_if_addrs().map_err(MdnsError::Interfaces)?;
        if !ip.is_unspecified() {
            let found = intfs
                .iter()
                .any(|intf| intf.ip() == ip && self.interfaces.contains(&intf.name));
            return if found {
                Ok(())
            } else {
                Err(MdnsError::InterfaceMismatch {
                    ip,
                    interfaces: self.interfaces.join(","),
                })
            };
        }
        for name in &self.interfaces {
            let usable = intfs.iter().any(|intf| {
                &intf.name == name && !intf.is_loopback() && self.family_matches(intf.ip())
            });
            if !usable {
                tracing::warn!(
                    target: TARGET,
                    interface = %name,
                    "mDNS interface has no usable address yet"
                );
            }
        }
        Ok(())
    }

    fn family_matches(&self, ip: IpAddr) -> bool {
        match self.addr.ip() {
            IpAddr::V4(_) => ip.is_ipv4(),
            IpAddr::V6(_) => !self.ipv6_only || ip.is_ipv6(),
        }
    }

    fn check_access_url(&self, host: &str) -> Result<(), MdnsError> {
        let url = self.access_url(&format!("{host}{LONGEST_RENAME}"));
        if "accessurl=".len().saturating_add(url.len()) > usize::from(u8::MAX) {
            return Err(MdnsError::AccessUrlTooLong(url));
        }
        Ok(())
    }

    fn access_url(&self, host: &str) -> String {
        format!(
            "{}://{host}.local:{}{}",
            self.scheme,
            self.addr.port(),
            self.path
        )
    }

    fn service_info(&self, instance: &str, host: &str) -> Result<ServiceInfo, MdnsError> {
        let mut properties = Vec::with_capacity(2);
        if let Some(id) = &self.identification {
            properties.push(TxtProperty::from(("identification", id.as_str())));
        }
        properties.push(TxtProperty::from((
            "accessurl",
            self.access_url(host).as_str(),
        )));

        let hostname = format!("{host}.local.");
        let ip = self.addr.ip();
        let mut info = ServiceInfo::new(
            SERVICE_TYPE,
            instance,
            &hostname,
            (),
            self.addr.port(),
            properties,
        )?
        .enable_addr_auto();

        let kinds = if !ip.is_unspecified() {
            vec![IfKind::Addr(ip)]
        } else if !self.interfaces.is_empty() {
            self.if_names()
        } else if ip.is_ipv4() {
            vec![IfKind::IPv4]
        } else if self.ipv6_only {
            vec![IfKind::IPv6]
        } else {
            vec![IfKind::All]
        };
        info.set_interfaces(kinds);
        Ok(info)
    }

    fn if_names(&self) -> Vec<IfKind> {
        self.interfaces.iter().cloned().map(IfKind::Name).collect()
    }

    fn log_enabled(&self, instance: &str, host: &str) {
        let ip = self.addr.ip();
        let interfaces = if !self.interfaces.is_empty() {
            Some(self.interfaces.join(","))
        } else if ip.is_unspecified() {
            Some("all".to_owned())
        } else {
            None
        };
        tracing::info!(
            target: TARGET,
            %instance,
            service = %SERVICE_TYPE,
            host = %format!("{host}.local."),
            port = self.addr.port(),
            addr = %ip,
            interfaces = interfaces.map(tracing::field::display),
            identification = self.identification.as_deref().map(tracing::field::display),
            accessurl = %self.access_url(host),
            "mDNS enabled"
        );
    }
}

struct Watcher {
    fullname: Arc<Mutex<String>>,
    announcement: Announcement,
    instance: String,
    daemon: ServiceDaemon,
    host: Arc<Mutex<String>>,
    stopping: Arc<AtomicBool>,
}

impl Watcher {
    async fn run(self, events: Receiver<DaemonEvent>) {
        let mut followed = false;
        let mut warned = false;
        let mut deadline = tokio::time::Instant::now().checked_add(ANNOUNCE_TIMEOUT);
        loop {
            let event = match deadline {
                Some(at) => match tokio::time::timeout_at(at, events.recv_async()).await {
                    Ok(Ok(event)) => event,
                    Ok(Err(_)) => break,
                    Err(_) => {
                        tracing::warn!(
                            target: TARGET,
                            "mDNS has not announced on any interface yet"
                        );
                        deadline = None;
                        continue;
                    }
                },
                None => match events.recv_async().await {
                    Ok(event) => event,
                    Err(_) => break,
                },
            };
            let change = match event {
                DaemonEvent::Announce(..) => {
                    deadline = None;
                    continue;
                }
                DaemonEvent::NameChange(change) => change,
                DaemonEvent::Error(e) => {
                    tracing::warn!(target: TARGET, error = %e, "mDNS responder error");
                    continue;
                }
                _ => continue,
            };
            match change.rr_type {
                RRType::A | RRType::AAAA if !followed => {
                    followed = self.host_renamed(&change.new_name);
                }
                RRType::A | RRType::AAAA if !warned => {
                    tracing::warn!(
                        target: TARGET,
                        host = %change.new_name,
                        interface = %change.intf_name,
                        "mDNS host conflict persists, the host name must be unique"
                    );
                    warned = true;
                }
                RRType::A | RRType::AAAA => {}
                _ => self.instance_renamed(&change.original, change.new_name),
            }
        }
    }

    fn instance_renamed(&self, original: &str, new_name: String) {
        let mut fullname = self.fullname.lock().unwrap_or_else(PoisonError::into_inner);
        if *fullname != new_name {
            tracing::warn!(
                target: TARGET,
                from = %original,
                to = %new_name,
                "mDNS instance renamed"
            );
            *fullname = new_name;
        }
    }

    fn host_renamed(&self, new_name: &str) -> bool {
        if self.stopping.load(Ordering::Acquire) {
            return true;
        }
        let label = new_name
            .trim_end_matches('.')
            .trim_end_matches(".local")
            .to_owned();
        if let Err(e) = validate_host(&label) {
            tracing::warn!(target: TARGET, error = %e, "mDNS host rename not followed");
            return true;
        }
        let from = self
            .host
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        if from == label {
            return false;
        }

        let result = self
            .announcement
            .service_info(&self.instance, &label)
            .and_then(|info| Ok(self.daemon.register(info)?));
        match result {
            Ok(()) => {
                label.clone_into(&mut self.host.lock().unwrap_or_else(PoisonError::into_inner));
                tracing::warn!(
                target: TARGET,
                from = %format!("{from}.local."),
                to = %format!("{label}.local."),
                accessurl = %self.announcement.access_url(&label),
                "mDNS host renamed"
                );
            }
            Err(e) => tracing::warn!(
                target: TARGET,
                error = %e,
                to = %format!("{label}.local."),
                "mDNS re-registration failed"
            ),
        }
        true
    }
}

/// A registered service.
///
/// Call [`MdnsAnnouncer::shutdown`] to withdraw it. Dropping it withdraws the
/// service without waiting.
pub struct MdnsAnnouncer {
    daemon: Option<ServiceDaemon>,
    registered: String,
    fullname: Arc<Mutex<String>>,
    instance: String,
    host: Arc<Mutex<String>>,
    stopping: Arc<AtomicBool>,
}

impl MdnsAnnouncer {
    /// Full DNS-SD instance name, e.g. `abc123456789._sovd._tcp.local.`,
    /// including renames after a conflict.
    #[must_use]
    pub fn fullname(&self) -> String {
        self.fullname
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Current host name, including renames after a conflict.
    #[must_use]
    pub fn host(&self) -> String {
        let host = self.host.lock().unwrap_or_else(PoisonError::into_inner);
        format!("{host}.local.")
    }

    /// Sends goodbye packets and stops the responder.
    ///
    /// If the returned future is dropped early, the announcement is withdrawn
    /// as on drop.
    pub async fn shutdown(mut self) {
        self.stopping.store(true, Ordering::Release);
        let Some(daemon) = self.daemon.clone() else {
            return;
        };
        let confirmed = match daemon.unregister(&self.registered) {
            Ok(status) => matches!(
                tokio::time::timeout(UNREGISTER_TIMEOUT, status.recv_async()).await,
                Ok(Ok(UnregisterStatus::OK))
            ),
            Err(_) => false,
        };
        if confirmed {
            tokio::time::sleep(GOODBYE_REPEAT).await;
        }
        self.daemon = None;
        let _ = daemon.shutdown();

        let host = self.host();
        if confirmed {
            tracing::info!(target: TARGET, instance = %self.instance, %host, "mDNS disabled");
        } else {
            tracing::warn!(
                target: TARGET,
                instance = %self.instance,
                %host,
                "mDNS disabled without confirmed goodbye"
            );
        }
    }
}

impl std::fmt::Debug for MdnsAnnouncer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MdnsAnnouncer")
            .field("fullname", &self.fullname())
            .field("host", &self.host())
            .finish_non_exhaustive()
    }
}

impl Drop for MdnsAnnouncer {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::Release);
        if let Some(daemon) = self.daemon.take() {
            let _ = daemon.unregister(&self.registered);
            let _ = std::thread::Builder::new()
                .name("mdns-stop".to_owned())
                .spawn(move || {
                    std::thread::sleep(GOODBYE_REPEAT);
                    let _ = daemon.shutdown();
                });
        }
    }
}

fn host_from(identification: &str) -> Option<String> {
    let mut label = String::new();
    for c in identification.chars() {
        if c.is_ascii_alphanumeric() {
            label.push(c);
        } else if matches!(c, ' ' | '\t' | '_' | '-') && !label.ends_with('-') {
            label.push('-');
        }
    }
    let label: String = label.trim_matches('-').chars().take(MAX_HOST_LEN).collect();
    let label = label.trim_end_matches('-');
    (!label.is_empty()).then(|| label.to_owned())
}

fn system_host() -> String {
    os_host_name()
        .and_then(|name| name.split('.').next().and_then(host_from))
        .unwrap_or_else(|| FALLBACK_HOST.to_owned())
}

#[cfg(unix)]
#[allow(clippy::unnecessary_wraps)]
fn os_host_name() -> Option<String> {
    Some(
        rustix::system::uname()
            .nodename()
            .to_string_lossy()
            .into_owned(),
    )
}

#[cfg(windows)]
fn os_host_name() -> Option<String> {
    std::env::var("COMPUTERNAME").ok()
}

#[cfg(not(any(unix, windows)))]
fn os_host_name() -> Option<String> {
    None
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::arithmetic_side_effects
)]
mod tests {
    use std::net::Ipv4Addr;
    use std::time::Instant;

    use mdns_sd::{ResolvedService, ServiceEvent};

    use super::*;

    const VIN: &str = "ABC123456789";

    fn wildcard() -> SocketAddr {
        SocketAddr::from((Ipv4Addr::UNSPECIFIED, 7690))
    }

    #[test]
    fn host_labels_are_validated() {
        for ok in ["a", "ABC123456789", "ecu-1", &"a".repeat(60)] {
            assert!(validate_host(ok).is_ok(), "{ok}");
        }
        for bad in ["", "-a", "a-", "ABC_1", "a b", "a.local", &"a".repeat(61)] {
            assert!(validate_host(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn host_defaults_to_identification_then_system() {
        let explicit = Announcement::new(wildcard())
            .identification(VIN)
            .host("ecu-1");
        assert_eq!(explicit.resolve_host().unwrap(), "ecu-1");

        let by_id = Announcement::new(wildcard()).identification(VIN);
        assert_eq!(by_id.resolve_host().unwrap(), VIN);

        let system = Announcement::new(wildcard()).resolve_host().unwrap();
        assert!(validate_host(&system).is_ok(), "{system}");

        let invalid = Announcement::new(wildcard()).host("VIN_1");
        assert!(matches!(
            invalid.resolve_host(),
            Err(MdnsError::InvalidHost(_))
        ));
    }

    #[test]
    fn identification_is_made_dns_safe() {
        assert_eq!(host_from("VIN_0001").as_deref(), Some("VIN-0001"));
        assert_eq!(host_from(" Bench 3 ").as_deref(), Some("Bench-3"));
        assert_eq!(host_from("Line 2's Rig").as_deref(), Some("Line-2s-Rig"));
        assert_eq!(host_from("ECU__front").as_deref(), Some("ECU-front"));
        assert_eq!(host_from("rig - left").as_deref(), Some("rig-left"));
        assert_eq!(host_from("Bay #7").as_deref(), Some("Bay-7"));
        assert_eq!(host_from("Rig.07/B").as_deref(), Some("Rig07B"));
        assert_eq!(host_from(&"a".repeat(70)).map(|h| h.len()), Some(60));
        assert_eq!(host_from("__"), None);

        let fallback = Announcement::new(wildcard()).identification("__");
        assert!(validate_host(&fallback.resolve_host().unwrap()).is_ok());
    }

    #[test]
    fn txt_records_match_the_spec_example() {
        let info = Announcement::new(wildcard())
            .identification(VIN)
            .access("https", "/vehicle/")
            .service_info("abc123456789", VIN)
            .unwrap();

        assert_eq!(info.get_fullname(), "abc123456789._sovd._tcp.local.");
        assert_eq!(info.get_hostname(), "ABC123456789.local.");
        assert_eq!(info.get_property_val_str("identification"), Some(VIN));
        assert_eq!(
            info.get_property_val_str("accessurl"),
            Some("https://ABC123456789.local:7690/vehicle")
        );
    }

    #[test]
    fn access_url_leaves_room_for_a_rename() {
        let path = format!("/{}", "p".repeat(222));
        let long = Announcement::new(wildcard())
            .host("gw")
            .access("https", &path)
            .announce();
        assert!(
            matches!(long, Err(MdnsError::AccessUrlTooLong(_))),
            "{long:?}"
        );
    }

    #[test]
    fn mapped_ipv4_is_announced_as_ipv4() {
        let mapped: SocketAddr = "[::ffff:192.0.2.1]:7690".parse().unwrap();
        let announcement = Announcement::new(mapped);
        assert_eq!(announcement.addr, SocketAddr::from(([192, 0, 2, 1], 7690)));
    }

    #[test]
    fn access_path_gets_one_leading_slash() {
        for (path, url) in [
            ("sovd", "http://gw.local:7690/sovd"),
            ("/sovd/", "http://gw.local:7690/sovd"),
            ("/", "http://gw.local:7690"),
        ] {
            let access = Announcement::new(wildcard()).access("http", path);
            assert_eq!(access.access_url("gw"), url);
        }
    }

    #[test]
    fn announce_needs_a_runtime() {
        let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, 7690));
        let result = Announcement::new(addr).host("gw").announce();
        assert!(matches!(result, Err(MdnsError::NoRuntime)), "{result:?}");
    }

    #[test]
    fn identification_is_optional() {
        let info = Announcement::new(wildcard())
            .access("http", "/sovd")
            .service_info("gw", "gw")
            .unwrap();
        assert_eq!(info.get_property("identification"), None);
        assert_eq!(
            info.get_property_val_str("accessurl"),
            Some("http://gw.local:7690/sovd")
        );
    }

    #[test]
    fn wildcard_follows_interface_addresses() {
        let info = Announcement::new(wildcard())
            .service_info("gw", "gw")
            .unwrap();
        assert!(info.is_addr_auto());
        assert!(info.get_addresses().is_empty());
    }

    #[test]
    fn specific_address_follows_its_interface() {
        let ip = Ipv4Addr::new(192, 0, 2, 1);
        let info = Announcement::new(SocketAddr::from((ip, 7690)))
            .service_info("gw", "gw")
            .unwrap();
        assert!(info.is_addr_auto());
        assert!(info.get_addresses().is_empty());
    }

    #[test]
    fn interfaces_must_carry_a_specific_address() {
        let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, 7690));
        let err = Announcement::new(addr)
            .interfaces(["no-such-intf"])
            .check_interfaces()
            .unwrap_err();
        assert!(matches!(err, MdnsError::InterfaceMismatch { .. }), "{err}");

        let wildcard = Announcement::new(wildcard()).interfaces(["no-such-intf"]);
        assert!(wildcard.check_interfaces().is_ok());
    }

    #[tokio::test]
    #[ignore = "needs multicast on the loopback interface"]
    async fn cancelled_shutdown_still_stops_the_responder() {
        let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, 7690));
        let announcer = Announcement::new(addr)
            .host("opensovd-cancel-test")
            .announce()
            .unwrap();
        let daemon = announcer.daemon.clone().unwrap();

        tokio::select! {
            biased;
            () = announcer.shutdown() => panic!("shutdown finished before being cancelled"),
            () = std::future::ready(()) => {}
        }

        tokio::time::sleep(GOODBYE_REPEAT * 3).await;
        let status = daemon
            .status()
            .ok()
            .and_then(|rx| rx.recv_timeout(Duration::from_millis(500)).ok());
        assert_ne!(status, Some(mdns_sd::DaemonStatus::Running));
    }

    fn resolve(fullname: &str, timeout: Duration) -> Box<ResolvedService> {
        let browser = ServiceDaemon::new().unwrap();
        let events = browser.browse(SERVICE_TYPE).unwrap();
        let deadline = Instant::now() + timeout;
        let resolved = loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if let ServiceEvent::ServiceResolved(svc) = events
                .recv_timeout(remaining)
                .expect("service not resolved")
                && svc.fullname == fullname
            {
                break svc;
            }
        };
        let _ = browser.shutdown();
        resolved
    }

    #[tokio::test]
    #[ignore = "needs multicast on the loopback interface"]
    async fn announcement_is_discoverable_on_loopback() {
        let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, 7690));
        let announcer = Announcement::new(addr)
            .host("opensovd-extra-test")
            .identification(VIN)
            .access("http", "/sovd")
            .announce()
            .unwrap();

        let svc = resolve(&announcer.fullname(), Duration::from_secs(10));
        assert_eq!(svc.port, 7690);
        assert_eq!(svc.get_property_val_str("identification"), Some(VIN));
        assert_eq!(
            svc.get_property_val_str("accessurl"),
            Some("http://opensovd-extra-test.local:7690/sovd")
        );
        announcer.shutdown().await;
    }

    #[tokio::test]
    #[ignore = "needs multicast on the loopback interface"]
    async fn host_rename_updates_access_url() {
        let owner = ServiceDaemon::new().unwrap();
        let taken = ServiceInfo::new(
            "_sovdowner._tcp.local.",
            "owner",
            "opensovd-dup.local.",
            IpAddr::from(Ipv4Addr::new(127, 0, 0, 2)),
            1,
            None,
        )
        .unwrap();
        owner.register(taken).unwrap();
        tokio::time::sleep(Duration::from_secs(2)).await;

        let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, 7690));
        let announcer = Announcement::new(addr)
            .host("opensovd-dup")
            .access("http", "/sovd")
            .announce()
            .unwrap();

        let deadline = Instant::now() + Duration::from_secs(10);
        while announcer.host() == "opensovd-dup.local." && Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        assert_eq!(announcer.host(), "opensovd-dup-2.local.");

        let svc = resolve(&announcer.fullname(), Duration::from_secs(10));
        assert_eq!(
            svc.get_property_val_str("accessurl"),
            Some("http://opensovd-dup-2.local:7690/sovd")
        );
        announcer.shutdown().await;
        let _ = owner.shutdown();
    }
}
