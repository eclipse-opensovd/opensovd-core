// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use rustls::ServerConfig;
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;
use tokio_rustls::server::TlsStream;

// max number of TLS handshakes that can be made at the same time;
const MAX_PENDING_HANDSHAKES: usize = 256;
// how long to wait for a TLS handshake before dropping the connection
const TLS_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

/*
    Wraps a TcpListener with TLS acceptance.
    Each TLS handshake runs in its own spawned task so a slow or stalling client
    cannot block new TCP connections from being accepted.
*/
pub(crate) struct TlsListener {
    inner: TcpListener,
    acceptor: TlsAcceptor,
    // limits concurrent in-flight handshakes to MAX_PENDING_HANDSHAKES
    semaphore: Arc<tokio::sync::Semaphore>,
    // completed handshakes waiting to be returned to axum
    done_tx: tokio::sync::mpsc::Sender<(TlsStream<tokio::net::TcpStream>, SocketAddr)>,
    done_rx: tokio::sync::mpsc::Receiver<(TlsStream<tokio::net::TcpStream>, SocketAddr)>,
}

impl TlsListener {
    pub(crate) fn wrap(listener: TcpListener, config: ServerConfig) -> Self {
        let acceptor = TlsAcceptor::from(Arc::new(config));
        let semaphore = Arc::new(tokio::sync::Semaphore::new(MAX_PENDING_HANDSHAKES));
        let (done_tx, done_rx) = tokio::sync::mpsc::channel(MAX_PENDING_HANDSHAKES);
        Self {
            inner: listener,
            acceptor,
            semaphore,
            done_tx,
            done_rx,
        }
    }
}

impl axum::serve::Listener for TlsListener {
    type Io = TlsStream<tokio::net::TcpStream>;
    type Addr = SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        loop {
            tokio::select! {
                // accept a new TCP connection and immediately spawn its handshake
                tcp = self.inner.accept() => {
                    match tcp {
                        Ok((stream, addr)) => {
                            tracing::debug!(peer = %addr, "TCP connection accepted");
                            // try to grab a slot, if all 256 are taken, drop the connection
                            match Arc::clone(&self.semaphore).try_acquire_owned() {
                                Ok(permit) => {
                                    let acceptor = self.acceptor.clone();
                                    let tx = self.done_tx.clone();
                                    tokio::spawn(async move {
                                        // permit is dropped when this task ends, freeing the slot
                                        let _permit = permit;
                                        let result = tokio::time::timeout(
                                            TLS_HANDSHAKE_TIMEOUT,
                                            acceptor.accept(stream),
                                        ).await;
                                        match result {
                                            Ok(Ok(tls)) => {
                                                tracing::debug!(peer = %addr, "TLS handshake complete");
                                                let _ = tx.send((tls, addr)).await;
                                            }
                                            Ok(Err(e)) => tracing::warn!(peer = %addr, error = %e, "TLS handshake failed"),
                                            Err(_) => tracing::warn!(peer = %addr, "TLS handshake timed out"),
                                        }
                                    });
                                }
                                Err(_) => {
                                    // handshake queue full — drop the stream, TCP RST sent to client
                                    tracing::warn!(peer = %addr, "handshake queue full, dropping connection");
                                }
                            }
                        }
                        Err(e) => {
                            tracing::error!(error = %e, "TCP accept error");
                            // brief pause so we don't spin at 100% CPU on persistent errors
                            tokio::time::sleep(Duration::from_millis(10)).await;
                        }
                    }
                }
                // return the next completed handshake to axum
                Some(pair) = self.done_rx.recv() => {
                    return pair;
                }
            }
        }
    }

    fn local_addr(&self) -> io::Result<Self::Addr> {
        self.inner.local_addr()
    }
}
