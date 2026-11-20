// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Upstream SOVD servers found via mDNS, for `--enable-mdns-upstream-discovery`.
//!
//! A server announced as `_sovd._tcp` joins the federation as an upstream of
//! its entities once its `/version-info` names its API, and leaves when it
//! withdraws its announcement.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use http::{Method, Request, Uri};
use http_body_util::{BodyExt, Limited};
use opensovd_extra::mdns::{BrowseEvent, Browser, DiscoveredServer};
use opensovd_extra::{Federation, Upstream, UpstreamProxy};
use opensovd_server::Body;
use tower::ServiceExt;

const TARGET: &str = "mdns";

/// The largest `/version-info` accepted from a discovered server.
const VERSION_INFO_LIMIT: usize = 1024 * 1024;

/// How often `/version-info` is asked for, as a server may announce itself
/// just before it serves.
const VERSION_INFO_ATTEMPTS: u32 = 3;

/// The wait between the attempts.
const VERSION_INFO_RETRY: Duration = Duration::from_millis(500);

/// How the upstreams found are reached.
#[derive(Clone, Copy)]
pub struct Timeouts {
    /// For connecting to an upstream.
    pub connect: Duration,
    /// For what is asked to find out what an upstream has, including its
    /// `/version-info`.
    pub discovery: Duration,
}

/// The task that adds the servers a [`Browser`] finds to a [`Federation`].
/// Dropping it stops looking.
pub struct Discovery {
    task: tokio::task::JoinHandle<()>,
}

impl Discovery {
    /// Starts adding what `browser` finds to `federation`, but the gateway's
    /// own announcement `own`.
    pub fn start(
        browser: Browser,
        federation: Arc<Federation>,
        own: Option<String>,
        timeouts: Timeouts,
    ) -> Self {
        let task = tokio::spawn(run(browser, federation, own, timeouts));
        Self { task }
    }
}

impl Drop for Discovery {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn run(
    mut browser: Browser,
    federation: Arc<Federation>,
    own: Option<String>,
    timeouts: Timeouts,
) {
    // What is known of each server added, so that an announcement repeated
    // as it is does not ask for its `/version-info` again.
    let mut known: HashMap<String, DiscoveredServer> = HashMap::new();
    while let Some(event) = browser.next().await {
        match event {
            BrowseEvent::Resolved(server) => {
                if own.as_deref() == Some(server.fullname.as_str())
                    || known.get(&server.fullname) == Some(&server)
                {
                    continue;
                }
                match upstream(&server, timeouts).await {
                    Ok((api, upstream)) => {
                        tracing::info!(
                            target: TARGET,
                            server = %server.fullname,
                            identification = server.identification.as_deref().map(tracing::field::debug),
                            %api,
                            "Upstream found"
                        );
                        federation.add_discovered(&server.fullname, upstream);
                        known.insert(server.fullname.clone(), server);
                    }
                    Err(error) => {
                        tracing::warn!(
                            target: TARGET,
                            server = %server.fullname,
                            error = format!("{error:#}"),
                            "Upstream found but not used"
                        );
                    }
                }
            }
            BrowseEvent::Removed(fullname) => {
                known.remove(&fullname);
                if federation.remove_discovered(&fullname) {
                    tracing::info!(target: TARGET, server = %fullname, "Upstream left");
                }
            }
            _ => {}
        }
    }
}

/// The API of `server` and the upstream that forwards to it.
async fn upstream(
    server: &DiscoveredServer,
    timeouts: Timeouts,
) -> anyhow::Result<(Uri, Upstream)> {
    let (scheme, path) = server
        .access()
        .context("the announcement has no valid accessurl")?;
    anyhow::ensure!(
        scheme == "http",
        "only http upstreams are supported, the accessurl is {scheme}"
    );
    let ip = address(&server.addresses).context("the announcement has no usable address")?;
    let origin = SocketAddr::new(ip, server.port);
    let root: Uri = format!("http://{origin}{path}")
        .parse()
        .context("invalid accessurl")?;
    let base = api_path(&root, timeouts).await?;
    let api: Uri = format!("http://{origin}{base}")
        .parse()
        .context("invalid base_uri in /version-info")?;
    let proxy = UpstreamProxy::new(&api, timeouts.connect)?;
    let upstream = Upstream::new(proxy.into_service()).discovery_timeout(timeouts.discovery);
    Ok((api, upstream))
}

/// The address to reach a server at: IPv4 first, and no IPv6 link-local one,
/// which needs a scope the announcement does not carry.
fn address(addresses: &[IpAddr]) -> Option<IpAddr> {
    let usable = |ip: &&IpAddr| match ip {
        IpAddr::V4(v4) => !v4.is_unspecified(),
        IpAddr::V6(v6) => !v6.is_unspecified() && !v6.is_unicast_link_local(),
    };
    addresses
        .iter()
        .filter(usable)
        .min_by_key(|ip| ip.is_ipv6())
        .copied()
}

/// The path of the API below `root`, the parent of `/version-info`, as the
/// first entry of its `/version-info` names it, such as `/sovd/v1`.
async fn api_path(root: &Uri, timeouts: Timeouts) -> anyhow::Result<String> {
    let service = UpstreamProxy::new(root, timeouts.connect)?.into_service();
    let mut answer = Err(anyhow::anyhow!("/version-info was not asked for"));
    for attempt in 1..=VERSION_INFO_ATTEMPTS {
        if attempt > 1 {
            tokio::time::sleep(VERSION_INFO_RETRY).await;
        }
        let request = Request::builder()
            .method(Method::GET)
            .uri("/version-info")
            .body(Body::new(String::new()))?;
        answer = tokio::time::timeout(timeouts.discovery, async {
            let Ok(response) = service.clone().oneshot(request).await;
            let status = response.status();
            anyhow::ensure!(status.is_success(), "/version-info answered {status}");
            let body = Limited::new(response.into_body(), VERSION_INFO_LIMIT)
                .collect()
                .await
                .map_err(|error| anyhow::anyhow!("failed to read /version-info: {error}"))?;
            Ok(body.to_bytes())
        })
        .await
        .unwrap_or_else(|_| Err(anyhow::anyhow!("/version-info did not answer in time")));
        if answer.is_ok() {
            break;
        }
    }
    base_path(&answer?)
}

/// The path of the `base_uri` of the first entry of a `/version-info` `body`.
/// Only the path is used, as the server is reached at the address it was
/// found at.
fn base_path(body: &[u8]) -> anyhow::Result<String> {
    let info: serde_json::Value = serde_json::from_slice(body).context("invalid /version-info")?;
    let base = info
        .pointer("/sovd_info/0/base_uri")
        .and_then(serde_json::Value::as_str)
        .context("/version-info names no base_uri")?;
    let base: Uri = base
        .parse()
        .with_context(|| format!("invalid base_uri {base:?} in /version-info"))?;
    Ok(base.path().trim_end_matches('/').to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ipv4_is_preferred_and_link_local_ipv6_skipped() {
        let ips =
            |ips: &[&str]| -> Vec<IpAddr> { ips.iter().map(|ip| ip.parse().unwrap()).collect() };

        assert_eq!(
            address(&ips(&["2001:db8::1", "192.0.2.1"])),
            Some("192.0.2.1".parse().unwrap())
        );
        assert_eq!(
            address(&ips(&["fe80::1", "2001:db8::1"])),
            Some("2001:db8::1".parse().unwrap())
        );
        assert_eq!(address(&ips(&["fe80::1"])), None);
        assert_eq!(address(&[]), None);
    }

    #[test]
    fn the_base_path_is_that_of_the_first_version() {
        let body = br#"{"sovd_info":[{"version":"1.1","base_uri":"http://h:1/sovd/v1/"},{"version":"2","base_uri":"/x"}]}"#;
        assert_eq!(base_path(body).unwrap(), "/sovd/v1");
        let body = br#"{"sovd_info":[{"version":"1.1","base_uri":"/vehicle/v15"}]}"#;
        assert_eq!(base_path(body).unwrap(), "/vehicle/v15");
        assert!(base_path(br#"{"sovd_info":[]}"#).is_err());
        assert!(base_path(b"not json").is_err());
    }
}
