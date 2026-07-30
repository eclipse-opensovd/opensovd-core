// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

use std::net::{IpAddr, SocketAddr};

use opensovd_providers::{
    mdns::{MdnsError, MdnsServiceDiscovery, MdnsServiceDiscoverySession},
    service_discovery::ServiceDiscovery,
};

use crate::cli::MdnsArgs;

const TARGET: &str = "mdns";

// Returns the infotainment-facing gateway IP to advertise on mDNS.
pub fn resolve_host(args: &MdnsArgs, listener_ip: IpAddr) -> IpAddr {
    args.host.unwrap_or(listener_ip)
}

fn access_url(scheme: &str, ip: IpAddr, port: u16, base_path: &str) -> String {
    format!("{scheme}://{}{base_path}", SocketAddr::new(ip, port))
}

fn advertised_addr(args: &MdnsArgs, listener_addr: Option<SocketAddr>) -> Option<SocketAddr> {
    let listener_addr = listener_addr?;
    let ip = resolve_host(args, listener_addr.ip());
    (!ip.is_unspecified() && !ip.is_loopback()).then_some(SocketAddr::new(ip, listener_addr.port()))
}

/*
    Starts the default mDNS service discovery mechanism, advertises the gateway
    URL, and returns the session kept alive for the process lifetime.
*/
pub fn setup(
    args: &MdnsArgs,
    listener_addr: Option<SocketAddr>,
    scheme: &str,
    base_path: &str,
) -> Result<MdnsServiceDiscoverySession, MdnsError> {
    let session = MdnsServiceDiscovery.start()?;

    let Some(advertised_addr) = advertised_addr(args, listener_addr) else {
        tracing::warn!(
            target: TARGET,
            "Cannot advertise mDNS for a non-TCP, unspecified, or loopback listener. mDNS discovery will still run."
        );
        return Ok(session);
    };

    let identification = args.identification.as_deref().unwrap_or(args.name.as_str());
    let access_url = access_url(
        scheme,
        advertised_addr.ip(),
        advertised_addr.port(),
        base_path,
    );

    if let Err(e) = session.register(
        &args.name,
        identification,
        &access_url,
        advertised_addr.ip(),
        advertised_addr.port(),
    ) {
        tracing::warn!(
            target: TARGET,
            error = %e,
            "mDNS registration failed discovery will still run"
        );
    }

    Ok(session)
}

#[cfg(test)]
mod tests {
    use std::net::{Ipv4Addr, Ipv6Addr};

    use super::*;

    fn args(host: Option<IpAddr>) -> MdnsArgs {
        MdnsArgs {
            enabled: true,
            name: "opensovd".to_string(),
            host,
            identification: None,
        }
    }

    #[test]
    fn resolve_host_prefers_explicit_host() {
        let explicit = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 5));
        let resolved = resolve_host(&args(Some(explicit)), IpAddr::V4(Ipv4Addr::LOCALHOST));
        assert_eq!(resolved, explicit);
    }

    #[test]
    fn resolve_host_falls_back_to_listener_host() {
        let resolved = resolve_host(&args(None), IpAddr::V4(Ipv4Addr::new(192, 168, 1, 10)));
        assert_eq!(resolved, IpAddr::V4(Ipv4Addr::new(192, 168, 1, 10)));
    }

    #[test]
    fn advertised_addr_uses_bound_port_and_skips_unadvertisable_listeners() {
        let listener = SocketAddr::from((Ipv4Addr::new(192, 168, 1, 10), 53_421));
        assert_eq!(advertised_addr(&args(None), Some(listener)), Some(listener));
        assert_eq!(advertised_addr(&args(None), None), None);
        assert_eq!(
            advertised_addr(
                &args(None),
                Some(SocketAddr::from((Ipv4Addr::UNSPECIFIED, 7690)))
            ),
            None
        );
        assert_eq!(
            advertised_addr(
                &args(Some(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 5)))),
                Some(SocketAddr::from((Ipv4Addr::UNSPECIFIED, 53_421))),
            ),
            Some(SocketAddr::from((Ipv4Addr::new(10, 0, 0, 5), 53_421)))
        );
    }

    #[test]
    fn access_url_formats_ip_addresses_as_url_hosts() {
        assert_eq!(
            access_url(
                "http",
                IpAddr::V4(Ipv4Addr::new(192, 168, 1, 10)),
                7690,
                "/sovd"
            ),
            "http://192.168.1.10:7690/sovd"
        );
        assert_eq!(
            access_url("https", IpAddr::V6(Ipv6Addr::LOCALHOST), 7690, "/sovd"),
            "https://[::1]:7690/sovd"
        );
    }
}
