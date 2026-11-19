// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Downstream-style use of the public proxy and federation API only.

use std::convert::Infallible;
use std::io::Write;
use std::time::Duration;

use axum::body::Body;
use bytes::Bytes;
use http::{Request, Response, StatusCode};
use http_body_util::{BodyExt, Full};
use hyper_util::rt::TokioIo;
use opensovd_extra::{Federation, Upstream, UpstreamProxy};
use opensovd_server::Server;
use tokio::net::TcpListener;

#[tokio::test]
#[expect(
    clippy::default_trait_access,
    reason = "Exercise the API without a direct dependency on the topology crate"
)]
async fn a_public_http_proxy_is_installed_through_federation() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let upstream_server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let service = hyper::service::service_fn(|request: Request<hyper::body::Incoming>| async {
            assert_eq!(request.method(), "PUT");
            assert_eq!(request.uri(), "/vehicle/v15/components/remote/data?x=1");
            assert_eq!(
                request.headers().get("authorization").unwrap(),
                "Bearer client"
            );
            let body = request.into_body().collect().await.unwrap().to_bytes();
            assert_eq!(body, Bytes::from_static(b"request body"));

            let json = br#"{ "/vehicle/v15/key":"http:\/\/cda\/vehicle\/v15\/components\/remote", "n":1.10 }"#;
            let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
            gzip.write_all(json).unwrap();
            let encoded = gzip.finish().unwrap();
            Ok::<_, Infallible>(
                Response::builder()
                    .status(StatusCode::ACCEPTED)
                    .header("content-type", "application/json")
                    .header("content-encoding", "gzip")
                    .header("content-length", encoded.len())
                    .header("location", "http://cda/vehicle/v15/components/remote")
                    .header("connection", "x-hop")
                    .header("x-hop", "private")
                    .body(Full::new(Bytes::from(encoded)))
                    .unwrap(),
            )
        });
        hyper::server::conn::http1::Builder::new()
            .serve_connection(TokioIo::new(stream), service)
            .await
            .unwrap();
    });

    let base = format!("http://{addr}/vehicle/v15/").parse().unwrap();
    let proxy = UpstreamProxy::new(&base, Duration::from_secs(1)).unwrap();
    let upstream = Upstream::new(proxy.into_service());
    let layer = Federation::new(vec![upstream], Default::default())
        .unwrap()
        .into_layer();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let gateway_addr = listener.local_addr().unwrap();
    let gateway = Server::builder()
        .listener(listener)
        .base_uri("/sovd")
        .unwrap()
        .inner_layer(layer)
        .build()
        .unwrap();
    let gateway_server = tokio::spawn(async move { gateway.serve().await.unwrap() });
    let client = hyper_util::client::legacy::Client::builder(hyper_util::rt::TokioExecutor::new())
        .build_http();
    let request = Request::builder()
        .method("PUT")
        .uri(format!(
            "http://{gateway_addr}/sovd/v1/components/remote/data?x=1"
        ))
        .header("host", "gw:7690")
        .header("authorization", "Bearer client")
        .header("accept-encoding", "gzip, zstd")
        .body(Body::from("request body"))
        .unwrap();
    let response = tokio::time::timeout(Duration::from_secs(5), client.request(request))
        .await
        .unwrap()
        .unwrap();

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    assert_eq!(
        response.headers().get("location").unwrap(),
        "http://gw:7690/sovd/v1/components/remote"
    );
    assert!(!response.headers().contains_key("content-encoding"));
    let content_length = response
        .headers()
        .get("content-length")
        .map(|length| length.to_str().unwrap().parse::<usize>().unwrap());
    assert!(!response.headers().contains_key("x-hop"));
    let body = response.into_body().collect().await.unwrap().to_bytes();
    if let Some(length) = content_length {
        assert_eq!(length, body.len());
    }
    assert_eq!(
        body,
        Bytes::from_static(
            br#"{ "/vehicle/v15/key":"http://gw:7690/sovd/v1/components/remote", "n":1.10 }"#
        )
    );
    gateway_server.abort();
    upstream_server.abort();
}
