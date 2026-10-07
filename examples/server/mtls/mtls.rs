// SPDX-FileCopyrightText: 2026 Copyright (c) Contributors to the Eclipse Foundation
//
// See the NOTICE file(s) distributed with this work for additional
// information regarding copyright ownership.
//
// This program and the accompanying materials are made available under the
// terms of the Apache License Version 2.0 which is available at
// https://www.apache.org/licenses/LICENSE-2.0
//
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

use opensovd_extra::ServerTlsConfig;
use opensovd_mocks::create_mock_topology;
use opensovd_server::Server;
use tokio::net::TcpListener;

// paths relative to workspace root; run scripts/mkcerts.sh to generate.
const CERT: &str = "gen/certs/server.crt";
const KEY: &str = "gen/certs/server.key";
const CLIENT_CA: &str = "gen/certs/ca.crt";

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    libcli::init_tracing("info", None)?;

    let tls = ServerTlsConfig::new(CERT, KEY)
        .client_ca(CLIENT_CA)
        .build()?;

    let listener = TcpListener::bind("127.0.0.1:8443").await?;
    let topology = create_mock_topology().await;

    let server = Server::builder()
        .listener(listener)
        .tls(tls)
        .base_uri("https://127.0.0.1:8443/sovd")?
        .topology(topology)
        .layer(opensovd_extra::trace::server_layer())
        .build()?;

    tracing::info!("mTLS server on https://127.0.0.1:8443/sovd");
    tracing::info!("Client cert required — run mkcerts.sh to generate test certs");

    server.serve().await?;
    Ok(())
}
