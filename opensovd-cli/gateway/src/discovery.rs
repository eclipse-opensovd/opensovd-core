// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Upstream SOVD servers found via mDNS, for `--enable-mdns-upstream-discovery`.
//!
//! A server announced as `_sovd._tcp` joins the federation as an upstream of
//! its entities once its `/version-info` names its API, and leaves when it
//! withdraws its announcement.

use std::collections::HashMap;
use std::mem;
use std::net::{IpAddr, SocketAddr};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use http::{Method, Request, Uri};
use http_body_util::{BodyExt, Limited};
use opensovd_extra::mdns::{BrowseEvent, Browser, DiscoveredServer};
use opensovd_extra::{Federation, Upstream, UpstreamProxy};
use opensovd_server::Body;
use tokio::sync::Semaphore;
use tokio::task::{AbortHandle, JoinSet};
use tokio::time::MissedTickBehavior;
use tower::ServiceExt;

const TARGET: &str = "mdns";

/// The largest `/version-info` accepted from a discovered server.
const VERSION_INFO_LIMIT: usize = 1024 * 1024;

/// How the upstreams found are reached.
#[derive(Clone, Copy)]
pub struct Timeouts {
    /// For connecting to an upstream.
    pub connect: Duration,
    /// For what is asked to find out what an upstream has, including its
    /// `/version-info`.
    pub discovery: Duration,
}

/// The gateway's own announcement name as it is now, which a conflict with
/// another server's announcement may change.
pub type OwnName = Arc<dyn Fn() -> String + Send + Sync>;

/// How many servers are probed at once.
const PROBES: usize = 8;

/// When servers are probed.
#[derive(Clone, Copy)]
struct Pace {
    /// The wait before a server whose probe failed is probed again, as a
    /// server may announce itself before it serves. It doubles with each
    /// failure in a row.
    first_retry: Duration,
    /// The longest wait between the probes of a server.
    last_retry: Duration,
    /// How often to look whether the gateway's own announcement was renamed.
    own_check: Duration,
}

impl Pace {
    const DEFAULT: Self = Self {
        first_retry: Duration::from_secs(1),
        last_retry: Duration::from_secs(60),
        own_check: Duration::from_secs(1),
    };

    /// The wait after a failed probe that was made `waited` after the one
    /// before, or at once.
    fn retry(self, waited: Duration) -> Duration {
        if waited.is_zero() {
            self.first_retry
        } else {
            waited.saturating_mul(2).min(self.last_retry)
        }
    }
}

/// What makes an announcement unusable however often it is probed, such as
/// an `https` accessurl, so that it is not probed again until it changes.
#[derive(Debug)]
struct Unusable(String);

impl std::fmt::Display for Unusable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Unusable {}

/// What a probe finds: the API of a server and the upstream that forwards
/// to it.
type ProbeFuture = Pin<Box<dyn Future<Output = anyhow::Result<(Uri, Upstream)>> + Send>>;

/// How a server is probed, [`upstream`] but in tests.
type Probe = Arc<dyn Fn(DiscoveredServer, Timeouts) -> ProbeFuture + Send + Sync>;

/// Where discovery learns of the servers there are.
trait Events: Send + 'static {
    /// The next server found or removed, or `None` once there are no more.
    fn next(&mut self) -> impl Future<Output = Option<BrowseEvent>> + Send;
}

impl Events for Browser {
    fn next(&mut self) -> impl Future<Output = Option<BrowseEvent>> + Send {
        Browser::next(self)
    }
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
        own: Option<OwnName>,
        timeouts: Timeouts,
    ) -> Self {
        let probe: Probe = Arc::new(
            |server: DiscoveredServer, timeouts: Timeouts| -> ProbeFuture {
                Box::pin(async move { upstream(&server, timeouts).await })
            },
        );
        let state = State::new(federation, own, timeouts, probe, Pace::DEFAULT);
        let task = tokio::spawn(run(browser, state));
        Self { task }
    }
}

impl Drop for Discovery {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// The outcome of the probe of a server.
struct Probed {
    name: String,
    /// Which probe of `name` this is, as a newer one replaces it.
    seq: u64,
    server: DiscoveredServer,
    result: anyhow::Result<(Uri, Upstream)>,
}

/// A probe that has not finished, or waits to be made again.
struct Pending {
    seq: u64,
    server: DiscoveredServer,
    /// How long the probe waited before it was made, zero for the first.
    waited: Duration,
    handle: AbortHandle,
}

/// What discovery knows, and does with the events of the browser.
struct State {
    federation: Arc<Federation>,
    own: Option<OwnName>,
    timeouts: Timeouts,
    probe: Probe,
    pace: Pace,
    /// What is known of each server added, so that an announcement repeated
    /// as it is does not ask for its `/version-info` again.
    known: HashMap<String, DiscoveredServer>,
    /// The probes under way or to be made again, by server. A server that is
    /// removed or announced again leaves no stale probe that could add it.
    pending: HashMap<String, Pending>,
    /// The announcements of what was taken for the gateway itself, to use
    /// them if the gateway turns out to be named otherwise.
    skipped: HashMap<String, DiscoveredServer>,
    /// The gateway's own name when last looked at.
    own_name: Option<String>,
    probes: JoinSet<Probed>,
    permits: Arc<Semaphore>,
    seq: u64,
}

async fn run(mut events: impl Events, mut state: State) {
    // A rename of the gateway's own announcement comes with no event, so it
    // is looked for now and then.
    let mut own_check = tokio::time::interval(state.pace.own_check);
    own_check.set_missed_tick_behavior(MissedTickBehavior::Delay);
    // Not at once, as the name was just looked at.
    own_check.reset();
    loop {
        tokio::select! {
            event = events.next() => match event {
                Some(event) => {
                    state.own_changed();
                    state.event(event);
                }
                None => break,
            },
            Some(done) = state.probes.join_next(), if !state.probes.is_empty() => {
                // A probe that was aborted has been replaced or removed.
                if let Ok(probed) = done {
                    state.own_changed();
                    state.probed(probed);
                }
            }
            _ = own_check.tick(), if state.own.is_some() => state.own_changed(),
        }
    }
}

impl State {
    fn new(
        federation: Arc<Federation>,
        own: Option<OwnName>,
        timeouts: Timeouts,
        probe: Probe,
        pace: Pace,
    ) -> Self {
        let own_name = own.as_ref().map(|own| own());
        Self {
            federation,
            own,
            timeouts,
            probe,
            pace,
            known: HashMap::new(),
            pending: HashMap::new(),
            skipped: HashMap::new(),
            own_name,
            probes: JoinSet::new(),
            permits: Arc::new(Semaphore::new(PROBES)),
            seq: 0,
        }
    }

    fn is_own(&self, fullname: &str) -> bool {
        self.own_name.as_deref() == Some(fullname)
    }

    /// Follows a rename of the gateway's own announcement: the server that
    /// kept the name it had is an upstream, and the gateway is none.
    fn own_changed(&mut self) {
        let Some(now) = self.own.as_ref().map(|own| own()) else {
            return;
        };
        if self.own_name.as_deref() == Some(now.as_str()) {
            return;
        }
        self.own_name = Some(now.clone());
        self.forget(&now);
        self.skipped.remove(&now);
        for (_, server) in mem::take(&mut self.skipped) {
            self.probe(server, Duration::ZERO);
        }
    }

    fn event(&mut self, event: BrowseEvent) {
        match event {
            BrowseEvent::Resolved(server) => {
                if self.is_own(&server.fullname) {
                    self.skipped.insert(server.fullname.clone(), server);
                    return;
                }
                if self.known.get(&server.fullname) == Some(&server)
                    || self
                        .pending
                        .get(&server.fullname)
                        .is_some_and(|pending| pending.server == server)
                {
                    return;
                }
                self.probe(server, Duration::ZERO);
            }
            BrowseEvent::Removed(fullname) => {
                self.skipped.remove(&fullname);
                self.forget(&fullname);
            }
            _ => {}
        }
    }

    /// Asks `server` what it has after `wait`, instead of any probe of it
    /// before.
    fn probe(&mut self, server: DiscoveredServer, wait: Duration) {
        if let Some(old) = self.pending.remove(&server.fullname) {
            old.handle.abort();
        }
        self.seq = self.seq.wrapping_add(1);
        let seq = self.seq;
        let timeouts = self.timeouts;
        let probe = Arc::clone(&self.probe);
        let permits = Arc::clone(&self.permits);
        let name = server.fullname.clone();
        let task_server = server.clone();
        let handle = self.probes.spawn(async move {
            // It waits without a permit, so that a server that keeps failing
            // holds up the probes of no other.
            if !wait.is_zero() {
                tokio::time::sleep(wait).await;
            }
            let result = {
                let _permit = permits.acquire_owned().await;
                probe(task_server.clone(), timeouts).await
            };
            Probed {
                name,
                seq,
                server: task_server,
                result,
            }
        });
        self.pending.insert(
            server.fullname.clone(),
            Pending {
                seq,
                server,
                waited: wait,
                handle,
            },
        );
    }

    /// Forgets the server `fullname` and its upstream, and what is on its way.
    fn forget(&mut self, fullname: &str) {
        if let Some(old) = self.pending.remove(fullname) {
            old.handle.abort();
        }
        self.known.remove(fullname);
        if self.federation.remove_discovered(fullname) {
            tracing::info!(target: TARGET, server = %fullname, "Upstream left");
        }
    }

    fn probed(&mut self, probed: Probed) {
        let Probed {
            name,
            seq,
            server,
            result,
        } = probed;
        let waited = match self.pending.get(&name) {
            Some(pending) if pending.seq == seq => pending.waited,
            _ => return,
        };
        self.pending.remove(&name);
        match result {
            Ok((api, upstream)) => {
                tracing::info!(
                    target: TARGET,
                    server = %name,
                    identification = server.identification.as_deref().map(tracing::field::debug),
                    %api,
                    "Upstream found"
                );
                self.federation.add_discovered(&name, upstream);
                self.known.insert(name, server);
            }
            Err(error) if error.downcast_ref::<Unusable>().is_some() => {
                tracing::warn!(
                    target: TARGET,
                    server = %name,
                    error = format!("{error:#}"),
                    "Upstream found but not used"
                );
                self.known.remove(&name);
                if self.federation.remove_discovered(&name) {
                    tracing::info!(target: TARGET, server = %name, "Upstream left");
                }
            }
            Err(error) => {
                let retry = self.pace.retry(waited);
                if waited.is_zero() {
                    tracing::warn!(
                        target: TARGET,
                        server = %name,
                        error = format!("{error:#}"),
                        ?retry,
                        "Upstream found but not used yet"
                    );
                } else {
                    tracing::debug!(
                        target: TARGET,
                        server = %name,
                        error = format!("{error:#}"),
                        ?retry,
                        "Upstream still not used"
                    );
                }
                // A known server is probed again only when its announcement
                // changed. What it announces now is unusable, so the upstream
                // it had is not what it offers anymore, even if that may be
                // for a moment only: it is added again once a probe succeeds.
                self.known.remove(&name);
                if self.federation.remove_discovered(&name) {
                    tracing::info!(target: TARGET, server = %name, "Upstream left");
                }
                // It is probed again for as long as it is announced so, which
                // a removal or another announcement of it ends.
                self.probe(server, retry);
            }
        }
    }
}

/// The API of `server` and the upstream that forwards to it.
async fn upstream(
    server: &DiscoveredServer,
    timeouts: Timeouts,
) -> anyhow::Result<(Uri, Upstream)> {
    let unusable = |reason: String| anyhow::Error::new(Unusable(reason));
    let (scheme, path) = server
        .access()
        .ok_or_else(|| unusable("the announcement has no valid accessurl".to_owned()))?;
    if !scheme.eq_ignore_ascii_case("http") {
        let reason = format!("only http upstreams are supported, the accessurl is {scheme}");
        return Err(unusable(reason));
    }
    let ip = address(&server.addresses)
        .ok_or_else(|| unusable("the announcement has no usable address".to_owned()))?;
    let origin = SocketAddr::new(ip, server.port);
    let root: Uri = format!("http://{origin}{path}")
        .parse()
        .map_err(|error| unusable(format!("invalid accessurl: {error}")))?;
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
    let request = Request::builder()
        .method(Method::GET)
        .uri("/version-info")
        .body(Body::new(String::new()))?;
    let body = tokio::time::timeout(timeouts.discovery, async {
        let Ok(response) = service.oneshot(request).await;
        let status = response.status();
        anyhow::ensure!(status.is_success(), "/version-info answered {status}");
        let body = Limited::new(response.into_body(), VERSION_INFO_LIMIT)
            .collect()
            .await
            .map_err(|error| anyhow::anyhow!("failed to read /version-info: {error}"))?;
        Ok(body.to_bytes())
    })
    .await
    .unwrap_or_else(|_| Err(anyhow::anyhow!("/version-info did not answer in time")))?;
    base_path(&body)
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
    use std::sync::Mutex;

    use opensovd_server::Topology;
    use tokio::sync::mpsc;
    use tokio::task::JoinHandle;

    use super::*;

    const GATEWAY: &str = "gw._sovd._tcp.local.";
    const SERVER: &str = "x._sovd._tcp.local.";

    impl Events for mpsc::UnboundedReceiver<BrowseEvent> {
        fn next(&mut self) -> impl Future<Output = Option<BrowseEvent>> + Send {
            self.recv()
        }
    }

    fn ms(ms: u64) -> Duration {
        Duration::from_millis(ms)
    }

    /// How long a probe of a server takes, and whether it succeeds, by the
    /// server and the number of probes of its name before.
    type Outcome = dyn Fn(&DiscoveredServer, usize) -> (Duration, bool) + Send + Sync;

    /// Discovery with servers that are not there but probed as `outcome`
    /// says.
    struct Harness {
        events: mpsc::UnboundedSender<BrowseEvent>,
        federation: Arc<Federation>,
        own: Arc<Mutex<String>>,
        calls: Arc<Mutex<Vec<DiscoveredServer>>>,
        task: JoinHandle<()>,
    }

    impl Harness {
        fn start(
            outcome: impl Fn(&DiscoveredServer, usize) -> (Duration, bool) + Send + Sync + 'static,
        ) -> Self {
            let outcome: Arc<Outcome> = Arc::new(outcome);
            let calls = Arc::new(Mutex::new(Vec::<DiscoveredServer>::new()));
            let probe_calls = Arc::clone(&calls);
            let probe: Probe = Arc::new(move |server: DiscoveredServer, _| -> ProbeFuture {
                let before = {
                    let mut calls = probe_calls.lock().unwrap();
                    let before = calls
                        .iter()
                        .filter(|call| call.fullname == server.fullname)
                        .count();
                    calls.push(server.clone());
                    before
                };
                let (delay, ok) = outcome(&server, before);
                Box::pin(async move {
                    tokio::time::sleep(delay).await;
                    if server.port == 0 {
                        return Err(Unusable("port 0".to_owned()).into());
                    }
                    anyhow::ensure!(ok, "{} does not answer", server.fullname);
                    let api: Uri = format!("http://192.0.2.1:{}/sovd/v1", server.port).parse()?;
                    let proxy = UpstreamProxy::new(&api, Duration::from_secs(1))?;
                    Ok((api, Upstream::new(proxy.into_service())))
                })
            });
            let own = Arc::new(Mutex::new(GATEWAY.to_owned()));
            let own_source = Arc::clone(&own);
            let own_name: OwnName = Arc::new(move || own_source.lock().unwrap().clone());
            let federation = Arc::new(Federation::new(Vec::new(), Topology::default()).unwrap());
            let pace = Pace {
                first_retry: ms(20),
                last_retry: ms(80),
                own_check: ms(10),
            };
            let timeouts = Timeouts {
                connect: Duration::from_secs(1),
                discovery: Duration::from_secs(1),
            };
            let state = State::new(
                Arc::clone(&federation),
                Some(own_name),
                timeouts,
                probe,
                pace,
            );
            let (events, receiver) = mpsc::unbounded_channel();
            let task = tokio::spawn(run(receiver, state));
            Self {
                events,
                federation,
                own,
                calls,
                task,
            }
        }

        fn send(&self, event: BrowseEvent) {
            self.events.send(event).unwrap();
        }

        fn calls(&self) -> Vec<DiscoveredServer> {
            self.calls.lock().unwrap().clone()
        }

        /// Whether `name` was added, which removes it.
        fn take(&self, name: &str) -> bool {
            self.federation.remove_discovered(name)
        }

        /// Whether `name` is added within a while, which removes it.
        async fn takes(&self, name: &str) -> bool {
            eventually(|| self.take(name)).await
        }
    }

    impl Drop for Harness {
        fn drop(&mut self) {
            self.task.abort();
        }
    }

    /// Whether `condition` holds within a while.
    async fn eventually(mut condition: impl FnMut() -> bool) -> bool {
        tokio::time::timeout(Duration::from_secs(5), async {
            while !condition() {
                tokio::time::sleep(ms(5)).await;
            }
        })
        .await
        .is_ok()
    }

    fn server(fullname: &str, port: u16) -> DiscoveredServer {
        DiscoveredServer::new(fullname, port)
    }

    #[tokio::test]
    async fn the_own_announcement_is_skipped() {
        let harness = Harness::start(|_, _| (Duration::ZERO, true));

        harness.send(BrowseEvent::Resolved(server(GATEWAY, 1)));
        harness.send(BrowseEvent::Resolved(server(SERVER, 2)));

        assert!(harness.takes(SERVER).await);
        assert_eq!(harness.calls(), [server(SERVER, 2)]);
        assert!(!harness.take(GATEWAY));
    }

    #[tokio::test]
    async fn the_server_that_kept_the_name_of_a_renamed_gateway_is_added() {
        let harness = Harness::start(|_, _| (Duration::ZERO, true));
        harness.send(BrowseEvent::Resolved(server(GATEWAY, 1)));
        tokio::time::sleep(ms(50)).await;
        assert_eq!(harness.calls(), []);

        // No event follows the rename.
        "gw (2)._sovd._tcp.local.".clone_into(&mut harness.own.lock().unwrap());

        assert!(harness.takes(GATEWAY).await);
        assert_eq!(harness.calls(), [server(GATEWAY, 1)]);
    }

    #[tokio::test]
    async fn a_removal_cancels_the_probe_under_way() {
        let harness = Harness::start(|_, _| (ms(100), true));
        harness.send(BrowseEvent::Resolved(server(SERVER, 1)));
        assert!(eventually(|| !harness.calls().is_empty()).await);

        harness.send(BrowseEvent::Removed(SERVER.to_owned()));
        tokio::time::sleep(ms(300)).await;

        assert!(!harness.take(SERVER));
        assert_eq!(harness.calls().len(), 1);
    }

    #[tokio::test]
    async fn a_newer_announcement_replaces_the_probe_under_way() {
        // The old announcement would be usable, the new one is not.
        let harness = Harness::start(|server, _| (ms(100), server.port == 1));
        harness.send(BrowseEvent::Resolved(server(SERVER, 1)));
        assert!(eventually(|| !harness.calls().is_empty()).await);

        harness.send(BrowseEvent::Resolved(server(SERVER, 2)));
        assert!(
            eventually(|| harness.calls().len() > 2).await,
            "the new announcement is probed again"
        );

        assert!(!harness.take(SERVER));
        assert!(harness.calls().iter().skip(1).all(|call| call.port == 2));
    }

    #[tokio::test]
    async fn a_failed_probe_is_retried_until_it_succeeds() {
        let harness = Harness::start(|_, before| (Duration::ZERO, before >= 3));
        let started = std::time::Instant::now();

        harness.send(BrowseEvent::Resolved(server(SERVER, 1)));

        assert!(harness.takes(SERVER).await);
        assert_eq!(harness.calls().len(), 4);
        // 20, 40 and 80 ms between the probes.
        assert!(started.elapsed() >= ms(140));
    }

    #[tokio::test]
    async fn a_removal_ends_the_retries() {
        let harness = Harness::start(|_, _| (Duration::ZERO, false));
        harness.send(BrowseEvent::Resolved(server(SERVER, 1)));
        assert!(eventually(|| harness.calls().len() >= 2).await);

        harness.send(BrowseEvent::Removed(SERVER.to_owned()));
        tokio::time::sleep(ms(50)).await;
        let probed = harness.calls().len();
        tokio::time::sleep(ms(300)).await;

        assert_eq!(harness.calls().len(), probed);
        assert!(!harness.take(SERVER));
    }

    #[tokio::test]
    async fn an_unusable_announcement_is_not_probed_again_until_it_changes() {
        let harness = Harness::start(|_, _| (Duration::ZERO, true));
        harness.send(BrowseEvent::Resolved(server(SERVER, 0)));
        assert!(eventually(|| !harness.calls().is_empty()).await);
        tokio::time::sleep(ms(200)).await;
        assert_eq!(harness.calls().len(), 1);

        harness.send(BrowseEvent::Resolved(server(SERVER, 1)));

        assert!(harness.takes(SERVER).await);
        assert_eq!(harness.calls(), [server(SERVER, 0), server(SERVER, 1)]);
    }

    #[test]
    fn the_wait_between_probes_doubles_up_to_a_minute() {
        let pace = Pace::DEFAULT;
        let mut waited = Duration::ZERO;
        let waits: Vec<u64> = (0..9)
            .map(|_| {
                waited = pace.retry(waited);
                waited.as_secs()
            })
            .collect();
        assert_eq!(waits, [1, 2, 4, 8, 16, 32, 60, 60, 60]);
    }

    #[tokio::test]
    async fn the_scheme_of_the_accessurl_is_case_insensitive() {
        let timeouts = Timeouts {
            connect: Duration::from_secs(1),
            discovery: Duration::from_secs(1),
        };
        let mut found = server(SERVER, 1);
        found.access_url = Some("HTTP://h.local:1/sovd".to_owned());
        let error = upstream(&found, timeouts).await.err().unwrap();
        assert!(
            format!("{error:#}").contains("no usable address"),
            "{error:#}"
        );

        found.access_url = Some("https://h.local:1/sovd".to_owned());
        let error = upstream(&found, timeouts).await.err().unwrap();
        assert!(format!("{error:#}").contains("only http"), "{error:#}");
        assert!(error.downcast_ref::<Unusable>().is_some());
    }

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
