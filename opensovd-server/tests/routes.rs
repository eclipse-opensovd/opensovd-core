// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

#![cfg_attr(coverage_nightly, feature(coverage_attribute))]
#![allow(unsafe_code)]

use axum::Router;
use axum::extract::ConnectInfo as AxumConnectInfo;
use axum::routing::get;
use opensovd_server::ConnectInfo;
use tokio::net::TcpListener;
#[cfg(target_os = "linux")]
use tokio::net::UnixListener;

async fn connect_info_handler(info: AxumConnectInfo<ConnectInfo>) -> String {
    match info.0 {
        ConnectInfo::Tcp(tcp) => format!("tcp:{}", tcp.remote_addr.port()),
        #[cfg(unix)]
        ConnectInfo::Uds(uds) => {
            format!("uds:{}", uds.peer_cred.map_or(0, |c| c.uid()))
        }
    }
}

#[cfg_attr(coverage_nightly, coverage(off))]
#[tokio::test]
async fn test_tcp_connect_info() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let app = Router::new()
        .route("/", get(connect_info_handler))
        .into_make_service_with_connect_info::<ConnectInfo>();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let client = hyper_util::client::legacy::Client::builder(hyper_util::rt::TokioExecutor::new())
        .build_http();
    let response = client
        .request(
            hyper::Request::builder()
                .uri(format!("http://{addr}/"))
                .body(http_body_util::Empty::<bytes::Bytes>::new())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = String::from_utf8(
        http_body_util::BodyExt::collect(response.into_body())
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
    .unwrap();

    assert!(body.starts_with("tcp:"));
}

#[cfg(target_os = "linux")]
#[cfg_attr(coverage_nightly, coverage(off))]
#[tokio::test]
async fn test_uds_connect_info() {
    use tokio::net::UnixStream;

    let socket_name = format!("\0opensovd-test-{}", std::process::id());
    let listener = UnixListener::bind(&socket_name).unwrap();

    let app = Router::new()
        .route("/", get(connect_info_handler))
        .into_make_service_with_connect_info::<ConnectInfo>();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let stream = UnixStream::connect(&socket_name).await.unwrap();
    let (mut sender, conn) =
        hyper::client::conn::http1::handshake(hyper_util::rt::TokioIo::new(stream))
            .await
            .unwrap();

    tokio::spawn(async move {
        conn.await.ok();
    });

    let response = sender
        .send_request(
            hyper::Request::builder()
                .uri("/")
                .body(http_body_util::Empty::<bytes::Bytes>::new())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = String::from_utf8(
        http_body_util::BodyExt::collect(response.into_body())
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
    .unwrap();

    let uid = unsafe { libc::getuid() };
    assert_eq!(body, format!("uds:{uid}"));
}
