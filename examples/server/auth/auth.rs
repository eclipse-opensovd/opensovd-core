// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! JWT authentication + Rego policy authorization example.
//!
//! Starts a server on port 8080 with HS512 JWT authentication and a
//! small inline Rego policy that grants read access to the `reader` role.
//!
//! Run with: `cargo run -p opensovd-examples-server --example auth`
//!
//! A sample `curl` command with a pre-built token is printed at startup.

use std::io::Cursor;

use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use opensovd_extra::auth::jwt::Claims;
use opensovd_extra::{JwtAlgorithm, JwtAuthenticator, RegorusAuthorizer};
use opensovd_mocks::create_mock_topology;
use opensovd_server::Server;
use tokio::net::TcpListener;

const SECRET: &[u8] = b"example-secret-change-in-production";
const ISSUER: &str = "OpenSOVD";
const REGO_POLICY: &[u8] = include_bytes!("sovd_authz.rego");
const REGO_DATA: &[u8] = include_bytes!("sovd_data.json");

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    libcli::init_tracing("info", None)?;

    let jwt = JwtAuthenticator::new(JwtAlgorithm::HS512, SECRET, ISSUER);
    let rego = RegorusAuthorizer::new(
        &mut [("policy", &mut Cursor::new(REGO_POLICY))],
        &mut [&mut Cursor::new(REGO_DATA)],
    )?;

    let listener = TcpListener::bind("127.0.0.1:8080").await?;
    let topology = create_mock_topology().await;

    let server = Server::builder()
        .base_uri("http://127.0.0.1:8080/sovd")?
        .listener(listener)
        .topology(topology)
        .authenticator(jwt)
        .authorizer(rego)
        .layer(libcli::trace::trace_layer())
        .build()?;

    let token = make_token(SECRET, ISSUER)?;
    tracing::info!("Server running on http://127.0.0.1:8080");
    tracing::info!(
        "Try: curl -H 'Authorization: Bearer {token}' http://127.0.0.1:8080/sovd/v1/components"
    );

    server.serve().await?;
    Ok(())
}

fn make_token(secret: &[u8], issuer: &str) -> Result<String, Box<dyn std::error::Error>> {
    let claims = Claims {
        sub: "demo@example.com".to_owned(),
        exp: jsonwebtoken::get_current_timestamp().saturating_add(86400),
        iat: Some(jsonwebtoken::get_current_timestamp()),
        iss: Some(issuer.to_owned()),
        roles: vec!["reader".to_owned()],
        scope: None,
    };
    Ok(encode(
        &Header::new(Algorithm::HS512),
        &claims,
        &EncodingKey::from_secret(secret),
    )?)
}
