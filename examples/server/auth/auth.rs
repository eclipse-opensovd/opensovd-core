// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! JWT authentication + Rego policy authorization example.
//!
//! Starts a server on port 8080 with HS512 JWT authentication and a
//! small inline Rego policy that grants read access to the `reader` role.
//! The version info is public.
//!
//! Run with: `cargo run -p opensovd-examples-server --example auth`
//!
//! A sample `curl` command with a pre-built token is printed at startup.

use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use opensovd_extra::{JwtAlgorithm, JwtAuthenticator, RegorusAuthorizer};
use opensovd_mocks::create_mock_topology;
use opensovd_server::{Auth, Server};
use tokio::net::TcpListener;

const SECRET: &[u8] = b"example-secret-change-in-production";
const ISSUER: &str = "OpenSOVD";
const REGO_POLICY: &str = include_str!("sovd_authz.rego");
const REGO_DATA: &str = include_str!("sovd_data.json");

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    libcli::init_tracing("info", None)?;

    let jwt = JwtAuthenticator::new(JwtAlgorithm::HS512, SECRET, ISSUER);
    let rego = RegorusAuthorizer::new([("sovd_authz.rego", REGO_POLICY)], [REGO_DATA])?;

    let listener = TcpListener::bind("127.0.0.1:8080").await?;
    let topology = create_mock_topology().await;

    let server = Server::builder()
        .base_uri("http://127.0.0.1:8080/sovd")?
        .listener(listener)
        .topology(topology)
        .auth(Auth::new(jwt, rego))
        .layer(opensovd_extra::trace::server_layer())
        .build()?;

    let token = make_token(SECRET, ISSUER)?;
    tracing::info!("Server running on http://127.0.0.1:8080");
    tracing::info!(
        "Try: curl -H 'Authorization: Bearer {token}' http://127.0.0.1:8080/sovd/v1/components"
    );

    server.serve().await?;
    Ok(())
}

fn make_token(
    secret: &[u8],
    issuer: &str,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let now = jsonwebtoken::get_current_timestamp();
    let claims = serde_json::json!({
        "sub": "demo@example.com",
        "exp": now.saturating_add(86400),
        "iat": now,
        "iss": issuer,
        "roles": ["reader"],
    });
    Ok(encode(
        &Header::new(Algorithm::HS512),
        &claims,
        &EncodingKey::from_secret(secret),
    )?)
}
