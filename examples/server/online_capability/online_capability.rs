// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Online capability description example (ISO 17978-3).
//!
//! Starts a server on port 7691 with a mock topology containing components,
//! apps, and data resources. The point of interest is the SOVD *online
//! capability description*: appending `/docs` to the data collection path returns
//! a self-contained OpenAPI 3.1 specification describing how to interact with
//! that endpoint.
//!
//! Run with: `cargo run -p opensovd-examples-server --example online_capability`
//!
//! Then query the online capability description:
//!
//! ```bash
//! curl -s http://localhost:7691/sovd/v1/components/ecu/data/docs | jq
//! curl -s http://localhost:7691/sovd/v1/apps/diagnostics/data/docs | jq
//! ```

use opensovd_mocks::create_mock_topology;
use opensovd_server::{Server, Topology};
use tokio::net::TcpListener;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    libcli::init_tracing("info", None)?;

    let topology: Topology = create_mock_topology().await;

    let listener = TcpListener::bind("127.0.0.1:7691").await?;
    let server = Server::builder()
        .base_uri("http://127.0.0.1:7691/sovd")?
        .listener(listener)
        .topology(topology)
        .layer(libcli::trace::trace_layer())
        .build()?;

    tracing::info!(
        "Server running. Try: curl -s \
            http://localhost:7691/sovd/v1/components/ecu/data/docs | jq; \
            curl -s http://localhost:7691/sovd/v1/apps/diagnostics/data/docs | jq"
    );
    server.serve().await?;
    Ok(())
}
