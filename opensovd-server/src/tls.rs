// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

use std::io;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use rustls::ServerConfig;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::server::WebPkiClientVerifier;
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;
use tokio_rustls::server::TlsStream;

// max number of TLS handshakes that can be made at the same time;
const MAX_PENDING_HANDSHAKES: usize = 256;
// how long to wait for a TLS handshake before dropping the connection
const TLS_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, thiserror::Error)]
pub enum TlsConfigError {
    #[error("failed to read {path}: {source}")]
    Io { path: String, source: io::Error },
    #[error("no certificates found in {0}")]
    NoCerts(String),
    #[error("no private key found in {0}")]
    NoKey(String),
    #[error("TLS config error: {0}")]
    Rustls(#[from] rustls::Error),
    #[error("client verifier error: {0}")]
    Verifier(String),
}

// holds the paths needed to set up TLS. Call build() to get a TlsListener.
pub struct TlsConfig {
    cert: PathBuf,
    key: PathBuf,
    // client CAs for mTLS; if empty, client certs are not required
    client_cas: Vec<PathBuf>,
}

impl TlsConfig {
    pub fn new(cert: impl Into<PathBuf>, key: impl Into<PathBuf>) -> Self {
        Self {
            cert: cert.into(),
            key: key.into(),
            client_cas: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_client_ca(mut self, ca: impl Into<PathBuf>) -> Self {
        self.client_cas.push(ca.into());
        self
    }

    // Returns true if mTLS is configured (client cert required).
    #[must_use]
    pub fn has_client_ca(&self) -> bool {
        !self.client_cas.is_empty()
    }

    /// Build a [`TlsListener`] from this config and the given TCP listener.
    ///
    /// # Errors
    ///
    /// Returns a [`TlsConfigError`] if the certificate or key files cannot be read,
    /// contain no valid PEM entries, or if rustls rejects the TLS configuration.
    pub fn build(self, listener: TcpListener) -> Result<TlsListener, TlsConfigError> {
        let certs = load_certs(&self.cert)?;
        let key = load_key(&self.key)?;

        let provider = Arc::new(rustls::crypto::ring::default_provider());

        let config = if self.client_cas.is_empty() {
            // plain TLS: no client cert required
            ServerConfig::builder_with_provider(Arc::clone(&provider))
                .with_safe_default_protocol_versions()
                .map_err(TlsConfigError::Rustls)?
                .with_no_client_auth()
                .with_single_cert(certs, key)
                .map_err(TlsConfigError::Rustls)?
        } else {
            // mTLS: client must present a cert signed by one of the CAs
            let mut root_store = rustls::RootCertStore::empty();
            for ca_path in &self.client_cas {
                for cert in load_certs(ca_path)? {
                    root_store.add(cert).map_err(TlsConfigError::Rustls)?;
                }
            }
            let verifier = WebPkiClientVerifier::builder_with_provider(
                Arc::new(root_store),
                Arc::clone(&provider),
            )
            .build()
            .map_err(|e| TlsConfigError::Verifier(e.to_string()))?;
            ServerConfig::builder_with_provider(provider)
                .with_safe_default_protocol_versions()
                .map_err(TlsConfigError::Rustls)?
                .with_client_cert_verifier(verifier)
                .with_single_cert(certs, key)
                .map_err(TlsConfigError::Rustls)?
        };

        let acceptor = TlsAcceptor::from(Arc::new(config));
        let semaphore = Arc::new(tokio::sync::Semaphore::new(MAX_PENDING_HANDSHAKES));
        let (done_tx, done_rx) = tokio::sync::mpsc::channel(MAX_PENDING_HANDSHAKES);
        Ok(TlsListener {
            inner: listener,
            acceptor,
            semaphore,
            done_tx,
            done_rx,
        })
    }
}

fn read_file(path: &Path) -> Result<Vec<u8>, TlsConfigError> {
    std::fs::read(path).map_err(|e| TlsConfigError::Io {
        path: path.display().to_string(),
        source: e,
    })
}

fn load_certs(path: &Path) -> Result<Vec<CertificateDer<'static>>, TlsConfigError> {
    let data = read_file(path)?;
    let certs: Vec<_> = rustls_pemfile::certs(&mut data.as_slice())
        .collect::<Result<_, _>>()
        .map_err(|e| TlsConfigError::Io {
            path: path.display().to_string(),
            source: e,
        })?;
    if certs.is_empty() {
        return Err(TlsConfigError::NoCerts(path.display().to_string()));
    }
    Ok(certs)
}

fn load_key(path: &Path) -> Result<PrivateKeyDer<'static>, TlsConfigError> {
    let data = read_file(path)?;
    rustls_pemfile::private_key(&mut data.as_slice())
        .map_err(|e| TlsConfigError::Io {
            path: path.display().to_string(),
            source: e,
        })?
        .ok_or_else(|| TlsConfigError::NoKey(path.display().to_string()))
}

/*
    Wraps a TcpListener with TLS acceptance.
    Each TLS handshake runs in its own spawned task so a slow or stalling client
    cannot block new TCP connections from being accepted.
*/
pub struct TlsListener {
    inner: TcpListener,
    acceptor: TlsAcceptor,
    // limits concurrent in-flight handshakes to MAX_PENDING_HANDSHAKES
    semaphore: Arc<tokio::sync::Semaphore>,
    // completed handshakes waiting to be returned to axum
    done_tx: tokio::sync::mpsc::Sender<(TlsStream<tokio::net::TcpStream>, SocketAddr)>,
    done_rx: tokio::sync::mpsc::Receiver<(TlsStream<tokio::net::TcpStream>, SocketAddr)>,
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

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::*;

    #[test]
    fn tls_config_no_client_ca() {
        let cfg = TlsConfig::new("/tmp/cert.pem", "/tmp/key.pem");
        assert!(!cfg.has_client_ca());
    }

    #[test]
    fn tls_config_with_client_ca() {
        let cfg = TlsConfig::new("/tmp/cert.pem", "/tmp/key.pem").with_client_ca("/tmp/ca.crt");
        assert!(cfg.has_client_ca());
    }

    #[tokio::test]
    async fn tls_config_build_missing_file() {
        let cfg = TlsConfig::new("/nonexistent/cert.pem", "/nonexistent/key.pem");
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let result = cfg.build(listener);
        assert!(matches!(result, Err(TlsConfigError::Io { .. })));
    }
}
