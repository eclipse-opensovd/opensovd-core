// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

/*
    mTLS example server.

    Starts a server that requires clients to present a certificate signed by
    the local CA. Run scripts/mkcerts.sh first to generate the test certificates.

    Run with:
        cargo run -p opensovd-examples-server --example mtls --features tls

    Test with curl (client cert required):
        curl --cacert gen/certs/ca.crt \
            --cert gen/certs/client.crt \
            --key  gen/certs/client.key \
            https://127.0.0.1:8443/sovd/v1/components
*/

use opensovd_mocks::create_mock_topology;
use opensovd_server::{Server, TlsConfig};
use tokio::net::TcpListener;

// paths relative to workspace root; run scripts/mkcerts.sh to generate.
const CERT: &str = "gen/certs/server.crt";
const KEY: &str = "gen/certs/server.key";
const CLIENT_CA: &str = "gen/certs/ca.crt";

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    libcli::init_tracing("info", None)?;

    let tls = TlsConfig::new(CERT, KEY).with_client_ca(CLIENT_CA);
    let listener = TcpListener::bind("127.0.0.1:8443").await?;
    let topology = create_mock_topology().await;

    let server = Server::builder()
        .listener(listener)
        .tls(tls)
        .base_uri("https://127.0.0.1:8443/sovd")?
        .topology(topology)
        .layer(libcli::trace::trace_layer())
        .build()?;

    tracing::info!("mTLS server on https://127.0.0.1:8443/sovd");
    tracing::info!("Client cert required — run mkcerts.sh to generate test certs");

    server.serve().await?;
    Ok(())
}
