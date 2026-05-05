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

use std::sync::Arc;

use opensovd_mocks::create_mock_topology;
use opensovd_server::Server;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use tokio::net::TcpListener;

// paths relative to workspace root; run scripts/mkcerts.sh to generate.
const CERT: &str = "gen/certs/server.crt";
const KEY: &str = "gen/certs/server.key";
const CLIENT_CA: &str = "gen/certs/ca.crt";

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    libcli::init_tracing("info", None)?;

    let certs: Vec<CertificateDer<'static>> =
        CertificateDer::pem_file_iter(CERT)?.collect::<Result<_, _>>()?;
    let key = PrivateKeyDer::from_pem_file(KEY)?;

    let mut roots = rustls::RootCertStore::empty();
    for ca in CertificateDer::pem_file_iter(CLIENT_CA)? {
        roots.add(ca?)?;
    }

    let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
    let verifier = rustls::server::WebPkiClientVerifier::builder_with_provider(
        Arc::new(roots),
        Arc::clone(&provider),
    )
    .build()?;
    let tls = rustls::ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()?
        .with_client_cert_verifier(verifier)
        .with_single_cert(certs, key)?;

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
