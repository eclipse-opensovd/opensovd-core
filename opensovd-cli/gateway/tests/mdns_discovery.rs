// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! `--enable-mdns-upstream-discovery`: a gateway finds an SOVD server that is
//! announced via mDNS, serves its entities, and lets go of it once the server
//! withdraws its announcement. Without the option it does not look.
//!
//! The server is announced on all interfaces but loopback, so the host needs
//! one that carries multicast. Where none does, such as on the macOS CI
//! runners, set `OPENSOVD_SKIP_MULTICAST_TESTS` to skip the test.

#![cfg(feature = "mdns")]
#![expect(clippy::unwrap_used, clippy::expect_used)]

use std::net::SocketAddr;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use opensovd_core::{Component, Topology};
use opensovd_extra::Announcement;
use opensovd_gateway::{cli, run_with_shutdown};
use opensovd_server::Server;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

/// How long mDNS gets to find or lose the server.
const MDNS_TIMEOUT: Duration = Duration::from_secs(20);

/// A gateway that runs until its `stop` is sent.
struct Gateway {
    address: SocketAddr,
    stop: oneshot::Sender<()>,
    task: JoinHandle<anyhow::Result<()>>,
}

impl Gateway {
    async fn start(extra: &[&str]) -> Self {
        // The runner binds from its URL; reserve an ephemeral port until just before startup.
        let reservation = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = reservation.local_addr().unwrap();
        let url = format!("http://{address}/sovd");
        let args = ["gateway", "--url", &url]
            .into_iter()
            .chain(extra.iter().copied());
        let options = cli::try_parse_from(args).unwrap();
        let (stop, stopped) = oneshot::channel();
        drop(reservation);
        let task = tokio::spawn(run_with_shutdown(options, Topology::default(), async {
            let _ = stopped.await;
        }));
        let gateway = Self {
            address,
            stop,
            task,
        };
        tokio::time::timeout(Duration::from_secs(5), async {
            while TcpStream::connect(address).await.is_err() {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("gateway started");
        gateway
    }

    /// The status line and body of `GET /sovd/v1{path}`.
    async fn get(&self, path: &str) -> String {
        let mut stream = TcpStream::connect(self.address).await.unwrap();
        let request =
            format!("GET /sovd/v1{path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut response = Vec::new();
        stream.read_to_end(&mut response).await.unwrap();
        String::from_utf8(response).unwrap()
    }

    /// Whether `/components` lists `id`.
    async fn lists(&self, id: &str) -> bool {
        let response = self.get("/components").await;
        assert!(response.starts_with("HTTP/1.1 200"), "{response}");
        response.contains(&format!("\"id\":\"{id}\""))
    }

    /// Waits until `/components` lists `id`, or no longer does.
    async fn wait_until_listed(&self, id: &str, listed: bool) {
        tokio::time::timeout(MDNS_TIMEOUT, async {
            while self.lists(id).await != listed {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("{id} listed is not {listed} in time"));
    }

    async fn shutdown(self) {
        self.stop.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(5), self.task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }
}

/// A name no other test on the network uses at the same time.
fn unique(prefix: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .subsec_nanos();
    format!("{prefix}-{}-{nanos}", std::process::id())
}

#[tokio::test]
async fn gateway_serves_the_entities_of_an_upstream_announced_via_mdns() {
    // CI sets the variable empty where the test runs.
    if std::env::var_os("OPENSOVD_SKIP_MULTICAST_TESTS").is_some_and(|skip| !skip.is_empty()) {
        return;
    }
    let id = unique("mdns-remote");

    // An SOVD server on all interfaces, as mDNS announces their addresses.
    let listener = TcpListener::bind("0.0.0.0:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let topology = Topology::default();
    topology
        .write()
        .await
        .add_component(Component::new(&id, "Remote"));
    let (upstream_stop, upstream_stopped) = oneshot::channel::<()>();
    let upstream = Server::builder()
        .listener(listener)
        .base_uri(format!("http://127.0.0.1:{port}/remote"))
        .unwrap()
        .topology(topology)
        .shutdown(async {
            let _ = upstream_stopped.await;
        })
        .build()
        .unwrap();
    let upstream_task = tokio::spawn(async move { upstream.serve().await });

    let discovering = Gateway::start(&["--enable-mdns-upstream-discovery"]).await;
    let default = Gateway::start(&[]).await;

    let announcer = Announcement::new(SocketAddr::from(([0, 0, 0, 0], port)))
        .identification(unique("vin"))
        .access("http", "/remote")
        .announce()
        .await
        .unwrap();

    discovering.wait_until_listed(&id, true).await;
    let response = discovering.get(&format!("/components/{id}")).await;
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert!(response.contains("\"name\":\"Remote\""), "{response}");
    let response = discovering.get("/components").await;
    assert!(
        response.contains(&format!(
            "\"href\":\"http://localhost/sovd/v1/components/{id}\""
        )),
        "the list points at the gateway: {response}"
    );

    // Discovery is off by default: the other gateway had as long to find it.
    assert!(!default.lists(&id).await);
    let response = default.get(&format!("/components/{id}")).await;
    assert!(response.starts_with("HTTP/1.1 404"), "{response}");

    announcer.shutdown().await;
    discovering.wait_until_listed(&id, false).await;

    discovering.shutdown().await;
    default.shutdown().await;
    upstream_stop.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(5), upstream_task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}
