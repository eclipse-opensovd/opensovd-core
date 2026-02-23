// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;

use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use opensovd_core::{Component, DiscoveryProvider, Topology};
use opensovd_server::Server;
use tokio::net::TcpListener;
use tokio::sync::oneshot;

pub struct TestEntity;

impl TestEntity {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(id: &str, name: &str) -> Component {
        Component::new(id, name)
    }

    pub fn with_metadata(component: Component, metadata: HashMap<String, String>) -> Component {
        component.with_metadata(metadata)
    }
}

pub type HttpClient =
    Client<hyper_util::client::legacy::connect::HttpConnector, http_body_util::Empty<bytes::Bytes>>;

pub struct TestServer {
    pub addr: std::net::SocketAddr,
    shutdown: Option<oneshot::Sender<()>>,
}

impl TestServer {
    pub async fn start() -> Self {
        TestServerBuilder::default().build().await
    }

    pub fn builder() -> TestServerBuilder {
        TestServerBuilder::default()
    }

    pub fn url(&self, path: &str) -> String {
        format!("http://{}{}", self.addr, path)
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
    }
}

#[derive(Default)]
pub struct TestServerBuilder {
    base: Option<String>,
    topology: Option<Topology>,
    discovery_providers: Vec<Box<dyn DiscoveryProvider>>,
}

impl TestServerBuilder {
    pub fn base_uri(mut self, base: &str) -> Self {
        self.base = Some(base.to_string());
        self
    }

    pub fn topology(mut self, topology: Topology) -> Self {
        self.topology = Some(topology);
        self
    }

    pub fn discovery(mut self, provider: impl DiscoveryProvider + 'static) -> Self {
        self.discovery_providers.push(Box::new(provider));
        self
    }

    pub async fn build(self) -> TestServer {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (shutdown_tx, shutdown_rx) = oneshot::channel();

        let base = self.base.unwrap_or_else(|| "/sovd".to_string());

        let mut builder = Server::builder()
            .listener(listener)
            .base_uri(&base)
            .unwrap()
            .shutdown(async {
                shutdown_rx.await.ok();
            });

        if let Some(topology) = self.topology {
            builder = builder.topology(topology);
        }

        for provider in self.discovery_providers {
            builder = builder.discovery(provider);
        }

        let server = builder.build().unwrap();
        tokio::spawn(async move {
            server.serve().await.unwrap();
        });

        TestServer {
            addr,
            shutdown: Some(shutdown_tx),
        }
    }
}

pub fn client() -> HttpClient {
    Client::builder(TokioExecutor::new()).build_http()
}
