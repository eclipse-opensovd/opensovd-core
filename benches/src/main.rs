// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

// Benchmark code uses expect/unwrap for setup since criterion doesn't support Result returns
#![allow(clippy::expect_used, clippy::arithmetic_side_effects)]

use std::sync::LazyLock;

use axum::{Router, body::Body, http::Request, routing::get};
use criterion::{Criterion, criterion_group, criterion_main};
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use opensovd_extra::auth::jwt::Claims;
use opensovd_extra::{JwtAlgorithm, JwtAuthenticator, RegorusAuthorizer};
use opensovd_server::{AllowAll, AuthenticationLayer, AuthorizationLayer, NoAuth};
use tower::ServiceExt;

const BENCH_ISSUER: &str = "OpenSOVD";

static HMAC_SECRET: LazyLock<Vec<u8>> = LazyLock::new(|| {
    use rand::RngExt;
    let mut secret = vec![0u8; 32];
    rand::rng().fill(&mut secret[..]);
    secret
});

const REGO_POLICY_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../examples/server/auth/sovd_authz.rego"
);
const REGO_DATA_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../examples/server/auth/sovd_data.json"
);

fn bench_claims() -> Claims {
    Claims {
        sub: "bench@example.com".to_owned(),
        exp: jsonwebtoken::get_current_timestamp() + 3600,
        iat: Some(jsonwebtoken::get_current_timestamp()),
        iss: Some("OpenSOVD".to_owned()),
        roles: vec!["reader".to_owned()],
        scope: Some("read".to_owned()),
    }
}

fn make_token(claims: &Claims) -> String {
    encode(
        &Header::new(Algorithm::HS512),
        claims,
        &EncodingKey::from_secret(&HMAC_SECRET),
    )
    .expect("Failed to encode JWT")
}

async fn handler() -> &'static str {
    "ok"
}

fn bench_no_auth(c: &mut Criterion) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("Failed to build runtime");

    c.bench_function("no_auth", |b| {
        b.to_async(&rt).iter(|| async {
            let app = Router::new()
                .route("/", get(handler))
                .layer(AuthorizationLayer::<AllowAll, ()>::new(AllowAll))
                .layer(AuthenticationLayer::new(NoAuth));

            let request = Request::builder()
                .uri("/")
                .body(Body::empty())
                .expect("Failed to build request");

            app.oneshot(request).await.expect("Request failed")
        });
    });
}

fn bench_jwt_hs512(c: &mut Criterion) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("Failed to build runtime");

    let authenticator = JwtAuthenticator::new(JwtAlgorithm::HS512, &HMAC_SECRET, BENCH_ISSUER);
    let token = make_token(&bench_claims());

    c.bench_function("jwt_hs512", |b| {
        let authenticator = authenticator.clone();
        let token = token.clone();
        b.to_async(&rt).iter(|| {
            let authenticator = authenticator.clone();
            let token = token.clone();
            async move {
                let app = Router::new()
                    .route("/sovd/v1/components", get(handler))
                    .layer(AuthorizationLayer::<AllowAll, Claims>::new(AllowAll))
                    .layer(AuthenticationLayer::new(authenticator));

                let request = Request::builder()
                    .uri("/sovd/v1/components")
                    .header(http::header::AUTHORIZATION, format!("Bearer {token}"))
                    .body(Body::empty())
                    .expect("Failed to build request");

                app.oneshot(request).await.expect("Request failed")
            }
        });
    });
}

fn bench_jwt_hs512_rego(c: &mut Criterion) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("Failed to build runtime");

    let authenticator = JwtAuthenticator::new(JwtAlgorithm::HS512, &HMAC_SECRET, BENCH_ISSUER);
    let token = make_token(&bench_claims());

    let authorizer = RegorusAuthorizer::from_paths(&[REGO_POLICY_PATH], &[REGO_DATA_PATH])
        .expect("Failed to load rego policy");

    c.bench_function("jwt_hs512_rego", |b| {
        let authenticator = authenticator.clone();
        let authorizer = authorizer.clone();
        let token = token.clone();
        b.to_async(&rt).iter(|| {
            let authenticator = authenticator.clone();
            let authorizer = authorizer.clone();
            let token = token.clone();
            async move {
                let app = Router::new()
                    .route("/sovd/v1/components", get(handler))
                    .layer(AuthorizationLayer::new(authorizer))
                    .layer(AuthenticationLayer::new(authenticator));

                let request = Request::builder()
                    .uri("/sovd/v1/components")
                    .header(http::header::AUTHORIZATION, format!("Bearer {token}"))
                    .body(Body::empty())
                    .expect("Failed to build request");

                app.oneshot(request).await.expect("Request failed")
            }
        });
    });
}

fn bench_jwt_hs512_rego_concurrent(c: &mut Criterion) {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .expect("Failed to build runtime");

    let authenticator = JwtAuthenticator::new(JwtAlgorithm::HS512, &HMAC_SECRET, BENCH_ISSUER);
    let token = make_token(&bench_claims());

    let authorizer = RegorusAuthorizer::from_paths(&[REGO_POLICY_PATH], &[REGO_DATA_PATH])
        .expect("Failed to load rego policy");

    c.bench_function("jwt_hs512_rego_concurrent", |b| {
        let authenticator = authenticator.clone();
        let authorizer = authorizer.clone();
        let token = token.clone();
        b.to_async(&rt).iter(|| {
            let authenticator = authenticator.clone();
            let authorizer = authorizer.clone();
            let token = token.clone();
            async move {
                let handles: Vec<_> = (0..50)
                    .map(|_| {
                        let authenticator = authenticator.clone();
                        let authorizer = authorizer.clone();
                        let token = token.clone();
                        tokio::spawn(async move {
                            let app = Router::new()
                                .route("/sovd/v1/components", get(handler))
                                .layer(AuthorizationLayer::new(authorizer))
                                .layer(AuthenticationLayer::new(authenticator));

                            let request = Request::builder()
                                .uri("/sovd/v1/components")
                                .header(http::header::AUTHORIZATION, format!("Bearer {token}"))
                                .body(Body::empty())
                                .expect("Failed to build request");

                            app.oneshot(request).await.expect("Request failed")
                        })
                    })
                    .collect();

                for handle in handles {
                    handle.await.expect("Task panicked");
                }
            }
        });
    });
}

criterion_group!(
    benches,
    bench_no_auth,
    bench_jwt_hs512,
    bench_jwt_hs512_rego,
    bench_jwt_hs512_rego_concurrent
);
criterion_main!(benches);
