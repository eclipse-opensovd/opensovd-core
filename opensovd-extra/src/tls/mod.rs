// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! rustls configs from PEM files.

mod server;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use rustls::RootCertStore;
use rustls::crypto::CryptoProvider;
use rustls::pki_types::pem::{self, PemObject};
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
pub use server::ServerTlsConfig;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum TlsError {
    #[error("failed to load {}", path.display())]
    Pem {
        path: PathBuf,
        #[source]
        source: pem::Error,
    },
    #[error("no certificates found in {}", .0.display())]
    NoCertificates(PathBuf),
    #[error("no private key found in {}", .0.display())]
    NoPrivateKey(PathBuf),
    #[error("invalid CA certificate in {}", path.display())]
    InvalidCa {
        path: PathBuf,
        #[source]
        source: rustls::Error,
    },
    #[error("failed to build client certificate verifier")]
    Verifier(#[from] rustls::server::VerifierBuilderError),
    #[error("failed to build rustls config")]
    Config(#[from] rustls::Error),
}

/// # Errors
///
/// File unreadable, malformed, or without certificates.
pub fn load_certs(path: &Path) -> Result<Vec<CertificateDer<'static>>, TlsError> {
    let pem = |source| TlsError::Pem {
        path: path.to_path_buf(),
        source,
    };
    let certs: Vec<_> = CertificateDer::pem_file_iter(path)
        .map_err(pem)?
        .collect::<Result<_, _>>()
        .map_err(pem)?;
    if certs.is_empty() {
        return Err(TlsError::NoCertificates(path.to_path_buf()));
    }
    Ok(certs)
}

/// # Errors
///
/// File unreadable, malformed, or without a supported key.
pub fn load_private_key(path: &Path) -> Result<PrivateKeyDer<'static>, TlsError> {
    match PrivateKeyDer::from_pem_file(path) {
        Ok(key) => Ok(key),
        Err(pem::Error::NoItemsFound) => Err(TlsError::NoPrivateKey(path.to_path_buf())),
        Err(source) => Err(TlsError::Pem {
            path: path.to_path_buf(),
            source,
        }),
    }
}

/// Trust anchors from one or more CA bundles.
///
/// # Errors
///
/// Any bundle fails to load or holds a certificate rustls rejects.
pub fn load_roots(paths: &[PathBuf]) -> Result<RootCertStore, TlsError> {
    let mut roots = RootCertStore::empty();
    for path in paths {
        for cert in load_certs(path)? {
            roots.add(cert).map_err(|source| TlsError::InvalidCa {
                path: path.clone(),
                source,
            })?;
        }
    }
    Ok(roots)
}

fn provider_or_default(provider: Option<Arc<CryptoProvider>>) -> Arc<CryptoProvider> {
    provider
        .or_else(|| CryptoProvider::get_default().cloned())
        .unwrap_or_else(|| Arc::new(rustls::crypto::aws_lc_rs::default_provider()))
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
pub(crate) mod testutil {
    use std::io::Write;

    use tempfile::NamedTempFile;

    pub struct Pem {
        pub cert: String,
        pub key: String,
    }

    pub fn self_signed(name: &str, ca: bool) -> Pem {
        let key = rcgen::KeyPair::generate().unwrap();
        let mut params = rcgen::CertificateParams::new(vec![name.into()]).unwrap();
        if ca {
            params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
        }
        let cert = params.self_signed(&key).unwrap();
        Pem {
            cert: cert.pem(),
            key: key.serialize_pem(),
        }
    }

    pub fn server() -> Pem {
        self_signed("localhost", false)
    }

    pub fn ca() -> Pem {
        self_signed("test-ca", true)
    }

    pub fn pem_file(content: &str) -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();
        file
    }
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::testutil::{ca, pem_file, server};
    use super::*;

    #[test]
    fn certs_missing_file_is_pem_error() {
        let err = load_certs(Path::new("/nonexistent/server.crt")).unwrap_err();
        assert!(matches!(err, TlsError::Pem { .. }), "{err}");
    }

    #[test]
    fn certs_empty_file_is_no_certificates() {
        let file = pem_file("");
        let err = load_certs(file.path()).unwrap_err();
        assert!(matches!(err, TlsError::NoCertificates(_)), "{err}");
    }

    #[test]
    fn certs_truncated_pem_is_pem_error() {
        let pem = server().cert;
        let file = pem_file(pem.split("-----END").next().unwrap());
        let err = load_certs(file.path()).unwrap_err();
        assert!(matches!(err, TlsError::Pem { .. }), "{err}");
    }

    #[test]
    fn key_from_cert_file_is_no_private_key() {
        let file = pem_file(&server().cert);
        let err = load_private_key(file.path()).unwrap_err();
        assert!(matches!(err, TlsError::NoPrivateKey(_)), "{err}");
    }

    #[test]
    fn key_empty_file_is_no_private_key() {
        let file = pem_file("");
        let err = load_private_key(file.path()).unwrap_err();
        assert!(matches!(err, TlsError::NoPrivateKey(_)), "{err}");
    }

    #[test]
    fn roots_union_of_bundles() {
        let a = pem_file(&ca().cert);
        let b = pem_file(&ca().cert);
        let roots = load_roots(&[a.path().into(), b.path().into()]).unwrap();
        assert_eq!(roots.len(), 2);
    }

    #[test]
    fn roots_empty_bundle_is_no_certificates() {
        let a = pem_file(&ca().cert);
        let b = pem_file("");
        let err = load_roots(&[a.path().into(), b.path().into()]).unwrap_err();
        assert!(matches!(err, TlsError::NoCertificates(_)), "{err}");
    }

    #[test]
    fn roots_garbage_cert_is_invalid_ca() {
        let file = pem_file("-----BEGIN CERTIFICATE-----\naGVsbG8=\n-----END CERTIFICATE-----\n");
        let err = load_roots(&[file.path().into()]).unwrap_err();
        assert!(matches!(err, TlsError::InvalidCa { .. }), "{err}");
    }

    #[test]
    fn provider_explicit_wins() {
        let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
        let chosen = provider_or_default(Some(Arc::clone(&provider)));
        assert!(Arc::ptr_eq(&chosen, &provider));
    }
}
