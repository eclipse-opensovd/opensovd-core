// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! The server's auth middleware fills the request span of the trace layer.

#![expect(clippy::unwrap_used)]

use std::fmt;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use aws_lc_rs::signature::{ECDSA_P256_SHA256_FIXED_SIGNING, EcdsaKeyPair};
use jsonwebtoken::jwk::{Jwk, JwkSet};
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use opensovd_extra::JwtAuthenticator;
use opensovd_extra::trace::server_layer;
use opensovd_server::{Auth, Authenticated, Server};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tracing::field::Field;
use tracing::span::{Id, Record};
use tracing_subscriber::layer::{Context, SubscriberExt};

const ISSUER: &str = "OpenSOVD";

/// Records the fields set on spans after creation, formatted as the default formatter does.
#[derive(Clone, Default)]
struct Recorded(Arc<Mutex<Vec<(&'static str, String)>>>);

impl Recorded {
    fn values(&self, name: &str) -> Vec<String> {
        let recorded = self.0.lock().unwrap();
        let named = recorded.iter().filter(|(field, _)| *field == name);
        named.map(|(_, value)| value.clone()).collect()
    }
}

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for Recorded {
    fn on_record(&self, _id: &Id, values: &Record<'_>, _ctx: Context<'_, S>) {
        let mut recorded = self.0.lock().unwrap();
        values.record(&mut |field: &Field, value: &dyn fmt::Debug| {
            recorded.push((field.name(), format!("{value:?}")));
        });
    }
}

fn token(key: &EncodingKey, kid: &str) -> String {
    let mut header = Header::new(Algorithm::ES256);
    header.kid = Some(kid.to_owned());
    let exp = jsonwebtoken::get_current_timestamp().saturating_add(60);
    let claims = serde_json::json!({"sub": "alice", "iss": ISSUER, "exp": exp});
    encode(&header, &claims, key).unwrap()
}

/// Sends a GET with `token` and returns the status line.
async fn get(addr: SocketAddr, token: &str) -> String {
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let request = format!(
        "GET /sovd/version-info HTTP/1.1\r\nHost: localhost\r\n\
         Authorization: Bearer {token}\r\nConnection: close\r\n\r\n"
    );
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();
    response.lines().next().unwrap().to_owned()
}

#[tokio::test]
async fn auth_records_subject_and_reason_on_the_request_span() {
    let recorded = Recorded::default();
    let subscriber = tracing_subscriber::registry().with(recorded.clone());
    let _guard = tracing::subscriber::set_default(subscriber);

    let pkcs8 = EcdsaKeyPair::generate(&ECDSA_P256_SHA256_FIXED_SIGNING)
        .unwrap()
        .to_pkcs8v1()
        .unwrap();
    let key = EncodingKey::from_ec_der(pkcs8.as_ref());
    let mut jwk = Jwk::from_encoding_key(&key, Algorithm::ES256).unwrap();
    jwk.common.key_id = Some("ec".to_owned());
    let jwt = JwtAuthenticator::new(&JwkSet { keys: vec![jwk] }, ISSUER, &[]).unwrap();

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = Server::builder()
        .listener(listener)
        .base_uri("/sovd")
        .unwrap()
        .auth(Auth::new(jwt, Authenticated))
        .layer(server_layer())
        .build()
        .unwrap();
    tokio::spawn(async move { server.serve().await.unwrap() });

    assert!(get(addr, &token(&key, "ec")).await.contains(" 200 "));
    assert!(
        get(addr, &token(&key, "ec\nforged line"))
            .await
            .contains(" 401 ")
    );

    assert_eq!(recorded.values("subject"), [r#""alice""#]);
    let reasons = recorded.values("reason");
    assert_eq!(reasons.len(), 1);
    assert!(
        reasons[0].contains(r"unknown kid ec\nforged line"),
        "{reasons:?}"
    );
}
