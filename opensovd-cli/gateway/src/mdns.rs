// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

use std::net::IpAddr;
use std::sync::Arc;

use opensovd_providers::mdns::{MdnsDiscoveryProvider, MdnsError, MdnsWrapper};

use crate::cli::MdnsArgs;

const TARGET: &str = "mdns";

// Returns the infotainment-facing gateway IP to advertise on mDNS.
pub fn resolve_host(args: &MdnsArgs, url_host: &str) -> Option<IpAddr> {
    if let Some(ip) = args.host {
        return Some(ip);
    }
    url_host.parse::<IpAddr>().ok()
}

/*
    Creates the mDNS daemon, advertises the gateway URL, and returns the
    wrapper (kept alive for the process lifetime) plus the private-side discovery provider.
*/
pub fn setup(
    args: &MdnsArgs,
    url_host: &str,
    port: u16,
    scheme: &str,
    base_path: &str,
) -> Result<(Arc<MdnsWrapper>, MdnsDiscoveryProvider), MdnsError> {
    let wrapper = Arc::new(MdnsWrapper::new()?);

    match resolve_host(args, url_host) {
        Some(ip) if ip.is_unspecified() || ip.is_loopback() => {
            tracing::warn!(
                target: TARGET,
                ip = %ip,
                "Cannot advertise an unspecified or loopback mDNS IP - use --mdns-host with the infotainment-facing gateway address. mDNS discovery will still run."
            );
        }
        Some(ip) => {
            let identification = args.identification.as_deref().unwrap_or(args.name.as_str());
            let access_url = format!("{scheme}://{ip}:{port}{base_path}");

            if let Err(e) = wrapper.register(&args.name, identification, &access_url, ip, port) {
                tracing::warn!(
                    target: TARGET,
                    error = %e,
                    "mDNS registration failed discovery will still run"
                );
            }
        }
        None => {
            tracing::warn!(
                target: TARGET,
                "Cannot determine advertise IP - use --mdns-host to set it explicitly. \
                 mDNS discovery will still run."
            );
        }
    }

    let provider = MdnsDiscoveryProvider::from_wrapper(Arc::clone(&wrapper));
    Ok((wrapper, provider))
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;

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
        let resolved = resolve_host(&args(Some(explicit)), "192.168.1.10");
        assert_eq!(resolved, Some(explicit));
    }

    #[test]
    fn resolve_host_falls_back_to_url_host() {
        let resolved = resolve_host(&args(None), "192.168.1.10");
        assert_eq!(resolved, Some(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 10))));
    }

    #[test]
    fn resolve_host_returns_none_for_non_ip_host() {
        assert_eq!(resolve_host(&args(None), "localhost"), None);
    }
}
