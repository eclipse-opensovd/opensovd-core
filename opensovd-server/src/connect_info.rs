// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Connection information types.

use std::net::SocketAddr;

use axum::extract::connect_info::Connected;
use axum::serve::IncomingStream;
use tokio::net::TcpListener;
#[cfg(unix)]
use tokio::net::UnixListener;
#[cfg(unix)]
use tokio::net::unix::UCred;

#[derive(Clone, Debug)]
pub struct TcpConnectInfo {
    pub remote_addr: SocketAddr,
}

#[cfg(unix)]
#[derive(Clone, Debug)]
pub struct UdsConnectInfo {
    pub peer_addr: Option<tokio::net::unix::SocketAddr>,
    pub peer_cred: Option<UCred>,
}

#[derive(Clone, Debug)]
pub enum ConnectInfo {
    Tcp(TcpConnectInfo),
    #[cfg(unix)]
    Uds(UdsConnectInfo),
}

impl Connected<IncomingStream<'_, TcpListener>> for ConnectInfo {
    fn connect_info(target: IncomingStream<'_, TcpListener>) -> Self {
        ConnectInfo::Tcp(TcpConnectInfo {
            remote_addr: *target.remote_addr(),
        })
    }
}

#[cfg(unix)]
impl Connected<IncomingStream<'_, UnixListener>> for ConnectInfo {
    fn connect_info(target: IncomingStream<'_, UnixListener>) -> Self {
        ConnectInfo::Uds(UdsConnectInfo {
            peer_addr: target.io().peer_addr().ok(),
            peer_cred: target.io().peer_cred().ok(),
        })
    }
}

#[cfg(feature = "tls")]
impl Connected<IncomingStream<'_, crate::tls::TlsListener>> for ConnectInfo {
    fn connect_info(target: IncomingStream<'_, crate::tls::TlsListener>) -> Self {
        ConnectInfo::Tcp(TcpConnectInfo {
            remote_addr: *target.remote_addr(),
        })
    }
}
