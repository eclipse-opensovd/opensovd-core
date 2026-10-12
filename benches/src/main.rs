// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

// Benchmark code uses expect/unwrap for setup since criterion doesn't support Result returns
#![expect(clippy::expect_used, clippy::arithmetic_side_effects)]

use std::collections::BTreeMap;
use std::sync::{Arc, LazyLock};

use aws_lc_rs::signature::{ECDSA_P256_SHA256_FIXED_SIGNING, EcdsaKeyPair};
use criterion::{Criterion, criterion_group, criterion_main};
use jsonwebtoken::jwk::{Jwk, JwkSet};
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use opensovd_extra::{JwtAuthenticator, RegorusAuthorizer};
use opensovd_server::{Access, Authenticator, Authorizer, Principal};

const BENCH_ISSUER: &str = "OpenSOVD";
const KID: &str = "bench";

static SIGNING_KEY: LazyLock<EncodingKey> = LazyLock::new(|| {
    let pkcs8 = EcdsaKeyPair::generate(&ECDSA_P256_SHA256_FIXED_SIGNING)
        .and_then(|key| key.to_pkcs8v1())
        .expect("Failed to generate key");
    EncodingKey::from_ec_der(pkcs8.as_ref())
});

const REGO_POLICY_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../examples/server/auth/sovd_authz.rego"
);
const REGO_DATA_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../examples/server/auth/sovd_data.json"
);

const ROUTE: &str = "/v1/components";

fn make_token() -> String {
    let now = jsonwebtoken::get_current_timestamp();
    let claims = serde_json::json!({
        "sub": "bench@example.com",
        "exp": now + 3600,
        "iat": now,
        "iss": BENCH_ISSUER,
        "roles": ["reader"],
        "scope": "read",
    });
    let mut header = Header::new(Algorithm::ES256);
    header.kid = Some(KID.to_owned());
    encode(&header, &claims, &SIGNING_KEY).expect("Failed to encode JWT")
}

fn request(token: &str) -> http::request::Parts {
    http::Request::get(ROUTE)
        .header(http::header::AUTHORIZATION, format!("Bearer {token}"))
        .body(())
        .expect("Failed to build request")
        .into_parts()
        .0
}

fn access(principal: &Principal) -> Access<'_> {
    Access {
        method: "GET",
        route: Some(ROUTE),
        params: BTreeMap::new(),
        principal: Some(principal),
    }
}

fn authenticator() -> JwtAuthenticator {
    let mut jwk =
        Jwk::from_encoding_key(&SIGNING_KEY, Algorithm::ES256).expect("Failed to build JWK");
    jwk.common.key_id = Some(KID.to_owned());
    JwtAuthenticator::new(&JwkSet { keys: vec![jwk] }, BENCH_ISSUER, &[])
        .expect("Failed to load JWKS")
}

fn authorizer() -> RegorusAuthorizer {
    RegorusAuthorizer::from_paths(&[REGO_POLICY_PATH], &[REGO_DATA_PATH])
        .expect("Failed to load rego policy")
}

fn authenticate(authenticator: &JwtAuthenticator, parts: &http::request::Parts) -> Principal {
    authenticator
        .authenticate(parts)
        .expect("Authentication failed")
        .expect("No principal")
}

fn bench_authenticate_es256(c: &mut Criterion) {
    let authenticator = authenticator();
    let parts = request(&make_token());

    c.bench_function("authenticate_es256", |b| {
        b.iter(|| authenticate(&authenticator, &parts));
    });
}

fn bench_authenticate_es256_authorize_rego(c: &mut Criterion) {
    let authenticator = authenticator();
    let authorizer = authorizer();
    let parts = request(&make_token());

    c.bench_function("authenticate_es256_authorize_rego", |b| {
        b.iter(|| {
            let principal = authenticate(&authenticator, &parts);
            assert!(authorizer.authorize(&access(&principal)));
        });
    });
}

fn bench_authenticate_es256_authorize_rego_concurrent(c: &mut Criterion) {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .expect("Failed to build runtime");

    let authenticator = Arc::new(authenticator());
    let authorizer = Arc::new(authorizer());
    let parts = Arc::new(request(&make_token()));

    c.bench_function("authenticate_es256_authorize_rego_concurrent", |b| {
        b.to_async(&rt).iter(|| {
            let handles: Vec<_> = (0..50)
                .map(|_| {
                    let authenticator = Arc::clone(&authenticator);
                    let authorizer = Arc::clone(&authorizer);
                    let parts = Arc::clone(&parts);
                    tokio::spawn(async move {
                        let principal = authenticate(&authenticator, &parts);
                        assert!(authorizer.authorize(&access(&principal)));
                    })
                })
                .collect();
            async move {
                for handle in handles {
                    handle.await.expect("Task panicked");
                }
            }
        });
    });
}

criterion_group!(
    benches,
    bench_authenticate_es256,
    bench_authenticate_es256_authorize_rego,
    bench_authenticate_es256_authorize_rego_concurrent
);
criterion_main!(benches);
