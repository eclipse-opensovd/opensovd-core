// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;
use std::sync::Arc;

use rustls::ServerConfig;
use rustls::crypto::CryptoProvider;
use rustls::server::WebPkiClientVerifier;

use super::{TlsError, load_certs, load_private_key, load_roots, provider_or_default};

/// Builds a [`ServerConfig`] from PEM files; any client CA enables mTLS.
#[derive(Debug, Clone)]
#[must_use]
pub struct ServerTlsConfig {
    cert: PathBuf,
    key: PathBuf,
    client_cas: Vec<PathBuf>,
    provider: Option<Arc<CryptoProvider>>,
}

impl ServerTlsConfig {
    pub fn new(cert: impl Into<PathBuf>, key: impl Into<PathBuf>) -> Self {
        Self {
            cert: cert.into(),
            key: key.into(),
            client_cas: Vec::new(),
            provider: None,
        }
    }

    pub fn client_ca(mut self, path: impl Into<PathBuf>) -> Self {
        self.client_cas.push(path.into());
        self
    }

    pub fn client_cas<I, P>(mut self, paths: I) -> Self
    where
        I: IntoIterator<Item = P>,
        P: Into<PathBuf>,
    {
        self.client_cas.extend(paths.into_iter().map(Into::into));
        self
    }

    /// Defaults to the installed rustls provider, else aws-lc-rs.
    pub fn provider(mut self, provider: Arc<CryptoProvider>) -> Self {
        self.provider = Some(provider);
        self
    }

    #[must_use]
    pub fn is_mtls(&self) -> bool {
        !self.client_cas.is_empty()
    }

    /// # Errors
    ///
    /// Any file fails to load or rustls rejects the material.
    pub fn build(self) -> Result<ServerConfig, TlsError> {
        let certs = load_certs(&self.cert)?;
        let key = load_private_key(&self.key)?;
        let provider = provider_or_default(self.provider);

        let verifier = if self.client_cas.is_empty() {
            WebPkiClientVerifier::no_client_auth()
        } else {
            let roots = load_roots(&self.client_cas)?;
            WebPkiClientVerifier::builder_with_provider(Arc::new(roots), Arc::clone(&provider))
                .build()?
        };

        Ok(ServerConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()?
            .with_client_cert_verifier(verifier)
            .with_single_cert(certs, key)?)
    }
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::super::testutil::{ca, pem_file, server};
    use super::*;

    #[test]
    fn builds_without_client_auth() {
        let server = server();
        let cert = pem_file(&server.cert);
        let key = pem_file(&server.key);
        let config = ServerTlsConfig::new(cert.path(), key.path());
        assert!(!config.is_mtls());
        config.build().unwrap();
    }

    #[test]
    fn builds_with_client_ca() {
        let server = server();
        let cert = pem_file(&server.cert);
        let key = pem_file(&server.key);
        let ca = pem_file(&ca().cert);
        let config = ServerTlsConfig::new(cert.path(), key.path()).client_ca(ca.path());
        assert!(config.is_mtls());
        config.build().unwrap();
    }

    #[test]
    fn builds_with_explicit_provider() {
        let server = server();
        let cert = pem_file(&server.cert);
        let key = pem_file(&server.key);
        let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
        let config = ServerTlsConfig::new(cert.path(), key.path())
            .provider(Arc::clone(&provider))
            .build()
            .unwrap();
        assert!(Arc::ptr_eq(config.crypto_provider(), &provider));
    }

    #[test]
    fn client_cas_extends() {
        let config = ServerTlsConfig::new("a", "b")
            .client_cas(["c", "d"])
            .client_ca("e");
        assert_eq!(config.client_cas.len(), 3);
    }

    #[test]
    fn mismatched_key_is_config_error() {
        let cert = pem_file(&ca().cert);
        let key = pem_file(&server().key);
        let err = ServerTlsConfig::new(cert.path(), key.path())
            .build()
            .unwrap_err();
        assert!(matches!(err, TlsError::Config(_)), "{err}");
    }

    #[test]
    fn empty_ca_file_is_no_certificates() {
        let server = server();
        let cert = pem_file(&server.cert);
        let key = pem_file(&server.key);
        let ca = pem_file("");
        let err = ServerTlsConfig::new(cert.path(), key.path())
            .client_ca(ca.path())
            .build()
            .unwrap_err();
        assert!(matches!(err, TlsError::NoCertificates(_)), "{err}");
    }
}
