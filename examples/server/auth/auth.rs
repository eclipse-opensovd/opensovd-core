// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! JWT authentication + Rego policy authorization example.
//!
//! Starts a server on port 8080 that verifies tokens with a freshly generated
//! ES256 key and applies the example Rego policy, which grants read access to
//! the `reader` role. The version info is public.
//!
//! Run with: `cargo run -p opensovd-examples-server --example auth`
//!
//! A sample `curl` command with a signed token is printed at startup.

use aws_lc_rs::signature::{ECDSA_P256_SHA256_FIXED_SIGNING, EcdsaKeyPair};
use jsonwebtoken::jwk::{Jwk, JwkSet};
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use opensovd_extra::{JwtAuthenticator, RegorusAuthorizer};
use opensovd_mocks::create_mock_topology;
use opensovd_server::{Auth, Server};
use tokio::net::TcpListener;

type Error = Box<dyn std::error::Error + Send + Sync>;

const KID: &str = "example";
const ISSUER: &str = "OpenSOVD";
const REGO_POLICY: &str = include_str!("sovd_authz.rego");
const REGO_DATA: &str = include_str!("sovd_data.json");

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Error> {
    libcli::init_tracing("info", None)?;

    let key = signing_key()?;
    let jwt = JwtAuthenticator::new(&key_set(&key)?, ISSUER, &[])?;
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

    let token = make_token(&key, ISSUER)?;
    tracing::info!("Server running on http://127.0.0.1:8080");
    tracing::info!(
        "Try: curl -H 'Authorization: Bearer {token}' http://127.0.0.1:8080/sovd/v1/components"
    );

    server.serve().await?;
    Ok(())
}

fn signing_key() -> Result<EncodingKey, Error> {
    let pkcs8 = EcdsaKeyPair::generate(&ECDSA_P256_SHA256_FIXED_SIGNING)?.to_pkcs8v1()?;
    Ok(EncodingKey::from_ec_der(pkcs8.as_ref()))
}

/// The JSON Web Key Set with the public half of `key`.
fn key_set(key: &EncodingKey) -> Result<JwkSet, Error> {
    let mut jwk = Jwk::from_encoding_key(key, Algorithm::ES256)?;
    jwk.common.key_id = Some(KID.to_owned());
    Ok(JwkSet { keys: vec![jwk] })
}

fn make_token(key: &EncodingKey, issuer: &str) -> Result<String, Error> {
    let now = jsonwebtoken::get_current_timestamp();
    let claims = serde_json::json!({
        "sub": "demo@example.com",
        "exp": now.saturating_add(86400),
        "iat": now,
        "iss": issuer,
        "roles": ["reader"],
    });
    let mut header = Header::new(Algorithm::ES256);
    header.kid = Some(KID.to_owned());
    Ok(encode(&header, &claims, key)?)
}
