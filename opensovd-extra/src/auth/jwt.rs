// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! JWT authentication supporting HS512 and RS512 algorithms.

use std::fmt;
use std::str::FromStr;
use std::sync::Arc;

use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode};
use opensovd_server::{AuthError, Authenticator};

/// Supported JWT signing algorithms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JwtAlgorithm {
    /// HMAC-SHA512 (symmetric shared secret).
    HS512,
    /// RSA-SHA512 (asymmetric public key, DER-encoded).
    RS512,
}

impl fmt::Display for JwtAlgorithm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HS512 => f.write_str("HS512"),
            Self::RS512 => f.write_str("RS512"),
        }
    }
}

impl FromStr for JwtAlgorithm {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "HS512" => Ok(Self::HS512),
            "RS512" => Ok(Self::RS512),
            other => Err(format!(
                "unsupported JWT algorithm: {other} (expected HS512 or RS512)"
            )),
        }
    }
}

/// JWT claims extracted from a valid Bearer token.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Claims {
    /// Subject (required).
    pub sub: String,
    /// Expiration time (required, validated by jsonwebtoken).
    pub exp: u64,
    /// Issued-at time.
    #[serde(default)]
    pub iat: Option<u64>,
    /// Issuer.
    #[serde(default)]
    pub iss: Option<String>,
    /// Roles for authorization decisions.
    #[serde(default)]
    pub roles: Vec<String>,
    /// OAuth2 scope string.
    #[serde(default)]
    pub scope: Option<String>,
}

/// JWT authenticator supporting HS512 and RS512.
///
/// Extracts a Bearer token from the `Authorization` header, validates it,
/// and returns [`Claims`] as the identity.
#[derive(Clone)]
pub struct JwtAuthenticator {
    decoding_key: Arc<DecodingKey>,
    validation: Arc<Validation>,
}

impl JwtAuthenticator {
    /// Create a new authenticator with the given algorithm, key bytes, and expected issuer.
    ///
    /// - `HS512`: `key` is the raw HMAC shared secret.
    /// - `RS512`: `key` is the PKCS#1 DER-encoded RSA public key.
    /// - `issuer`: required `iss` claim value (e.g. `"OpenSOVD"`).
    #[must_use]
    pub fn new(algorithm: JwtAlgorithm, key: &[u8], issuer: &str) -> Self {
        let (algo, decoding_key) = match algorithm {
            JwtAlgorithm::HS512 => (Algorithm::HS512, DecodingKey::from_secret(key)),
            JwtAlgorithm::RS512 => (Algorithm::RS512, DecodingKey::from_rsa_der(key)),
        };
        let mut validation = Validation::new(algo);
        // Do not require or validate `aud` - the gateway is not audience-specific.
        validation.validate_aud = false;
        validation.required_spec_claims.remove("aud");
        validation.set_issuer(&[issuer]);

        Self {
            decoding_key: Arc::new(decoding_key),
            validation: Arc::new(validation),
        }
    }
}

impl Authenticator for JwtAuthenticator {
    type Identity = Claims;

    async fn authenticate(
        &self,
        parts: &http::request::Parts,
    ) -> Result<Self::Identity, AuthError> {
        let header = parts
            .headers
            .get(http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.strip_prefix("Bearer "))
            .ok_or(AuthError::Unauthenticated)?;

        let token_data =
            decode::<Claims>(header, &self.decoding_key, &self.validation).map_err(|e| {
                tracing::debug!(error = %e, "JWT validation failed");
                AuthError::Unauthenticated
            })?;

        Ok(token_data.claims)
    }
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
#[allow(
    clippy::unwrap_used,
    clippy::arithmetic_side_effects,
    clippy::ignored_unit_patterns
)]
mod tests {
    use std::sync::LazyLock;

    use http::Request;
    use jsonwebtoken::{EncodingKey, Header, encode};

    use super::*;

    const TEST_ISSUER: &str = "OpenSOVD";

    static HMAC_SECRET: LazyLock<Vec<u8>> = LazyLock::new(|| {
        use rand::RngExt;
        let mut secret = vec![0u8; 32];
        rand::rng().fill(&mut secret[..]);
        secret
    });

    fn make_hs512_token(claims: &Claims) -> String {
        encode(
            &Header::new(Algorithm::HS512),
            claims,
            &EncodingKey::from_secret(&HMAC_SECRET),
        )
        .unwrap()
    }

    fn valid_claims() -> Claims {
        Claims {
            sub: "user@example.com".to_owned(),
            exp: jsonwebtoken::get_current_timestamp() + 3600,
            iat: Some(jsonwebtoken::get_current_timestamp()),
            iss: Some("OpenSOVD".to_owned()),
            roles: vec!["reader".to_owned(), "admin".to_owned()],
            scope: Some("read write".to_owned()),
        }
    }

    #[tokio::test]
    async fn hs512_valid_token() {
        let auth = JwtAuthenticator::new(JwtAlgorithm::HS512, &HMAC_SECRET, TEST_ISSUER);
        let token = make_hs512_token(&valid_claims());

        let (parts, _) = Request::builder()
            .header("Authorization", format!("Bearer {token}"))
            .body(())
            .unwrap()
            .into_parts();

        let claims = auth.authenticate(&parts).await.unwrap();
        assert_eq!(claims.sub, "user@example.com");
        assert_eq!(claims.roles, vec!["reader", "admin"]);
    }

    #[tokio::test]
    async fn hs512_missing_header_returns_unauthenticated() {
        let auth = JwtAuthenticator::new(JwtAlgorithm::HS512, &HMAC_SECRET, TEST_ISSUER);

        let (parts, _) = Request::builder().body(()).unwrap().into_parts();

        let err = auth.authenticate(&parts).await.unwrap_err();
        assert!(
            matches!(err, AuthError::Unauthenticated),
            "expected Unauthenticated, got {err:?}"
        );
    }

    #[tokio::test]
    async fn hs512_invalid_token_returns_error() {
        let auth = JwtAuthenticator::new(JwtAlgorithm::HS512, &HMAC_SECRET, TEST_ISSUER);

        let (parts, _) = Request::builder()
            .header("Authorization", "Bearer not-a-valid-jwt")
            .body(())
            .unwrap()
            .into_parts();

        let err = auth.authenticate(&parts).await.unwrap_err();
        assert!(
            matches!(err, AuthError::Unauthenticated),
            "expected InvalidCredentials, got {err:?}"
        );
    }

    #[tokio::test]
    async fn hs512_expired_token_returns_error() {
        let auth = JwtAuthenticator::new(JwtAlgorithm::HS512, &HMAC_SECRET, TEST_ISSUER);
        let mut claims = valid_claims();
        claims.exp = 1; // long expired

        let token = make_hs512_token(&claims);
        let (parts, _) = Request::builder()
            .header("Authorization", format!("Bearer {token}"))
            .body(())
            .unwrap()
            .into_parts();

        let err = auth.authenticate(&parts).await.unwrap_err();
        assert!(
            matches!(err, AuthError::Unauthenticated),
            "expected InvalidCredentials, got {err:?}"
        );
    }

    #[tokio::test]
    async fn hs512_wrong_secret_returns_error() {
        let auth = JwtAuthenticator::new(JwtAlgorithm::HS512, b"different-secret", TEST_ISSUER);
        let token = make_hs512_token(&valid_claims());

        let (parts, _) = Request::builder()
            .header("Authorization", format!("Bearer {token}"))
            .body(())
            .unwrap()
            .into_parts();

        let err = auth.authenticate(&parts).await.unwrap_err();
        assert!(
            matches!(err, AuthError::Unauthenticated),
            "expected InvalidCredentials, got {err:?}"
        );
    }

    use aws_lc_rs::encoding::AsDer;
    use aws_lc_rs::rsa::KeySize;
    use aws_lc_rs::signature::{KeyPair, RsaKeyPair};

    struct RsaTestKeys {
        private_pem: Vec<u8>,
        public_der: Vec<u8>,
    }

    static RSA_KEYS: LazyLock<RsaTestKeys> = LazyLock::new(|| {
        use base64::Engine;
        let key_pair = RsaKeyPair::generate(KeySize::Rsa2048).unwrap();
        let public_der = key_pair.public_key().as_ref().to_vec();
        let pkcs8_der = key_pair.as_der().unwrap();
        let b64 = base64::engine::general_purpose::STANDARD.encode(pkcs8_der.as_ref());
        let mut pem = String::from("-----BEGIN PRIVATE KEY-----\n");
        for chunk in b64.as_bytes().chunks(64) {
            pem.push_str(std::str::from_utf8(chunk).unwrap());
            pem.push('\n');
        }
        pem.push_str("-----END PRIVATE KEY-----\n");
        RsaTestKeys {
            private_pem: pem.into_bytes(),
            public_der,
        }
    });

    fn make_rs512_token(claims: &Claims) -> String {
        encode(
            &Header::new(Algorithm::RS512),
            &claims,
            &EncodingKey::from_rsa_pem(&RSA_KEYS.private_pem).unwrap(),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn rs512_valid_token() {
        let auth = JwtAuthenticator::new(JwtAlgorithm::RS512, &RSA_KEYS.public_der, TEST_ISSUER);
        let token = make_rs512_token(&valid_claims());

        let (parts, _) = Request::builder()
            .header("Authorization", format!("Bearer {token}"))
            .body(())
            .unwrap()
            .into_parts();

        let claims = auth.authenticate(&parts).await.unwrap();
        assert_eq!(claims.sub, "user@example.com");
        assert_eq!(claims.roles, vec!["reader", "admin"]);
    }

    #[tokio::test]
    async fn rs512_expired_token_returns_error() {
        let auth = JwtAuthenticator::new(JwtAlgorithm::RS512, &RSA_KEYS.public_der, TEST_ISSUER);
        let mut claims = valid_claims();
        claims.exp = 1;

        let token = make_rs512_token(&claims);
        let (parts, _) = Request::builder()
            .header("Authorization", format!("Bearer {token}"))
            .body(())
            .unwrap()
            .into_parts();

        let err = auth.authenticate(&parts).await.unwrap_err();
        assert!(
            matches!(err, AuthError::Unauthenticated),
            "expected InvalidCredentials, got {err:?}"
        );
    }

    #[tokio::test]
    async fn rs512_invalid_token_returns_error() {
        let auth = JwtAuthenticator::new(JwtAlgorithm::RS512, &RSA_KEYS.public_der, TEST_ISSUER);

        let (parts, _) = Request::builder()
            .header("Authorization", "Bearer not-a-valid-jwt")
            .body(())
            .unwrap()
            .into_parts();

        let err = auth.authenticate(&parts).await.unwrap_err();
        assert!(
            matches!(err, AuthError::Unauthenticated),
            "expected InvalidCredentials, got {err:?}"
        );
    }

    #[tokio::test]
    async fn hs512_wrong_issuer_returns_error() {
        let auth = JwtAuthenticator::new(JwtAlgorithm::HS512, &HMAC_SECRET, TEST_ISSUER);
        let mut claims = valid_claims();
        claims.iss = Some("wrong-issuer".to_owned());

        let token = make_hs512_token(&claims);
        let (parts, _) = Request::builder()
            .header("Authorization", format!("Bearer {token}"))
            .body(())
            .unwrap()
            .into_parts();

        let err = auth.authenticate(&parts).await.unwrap_err();
        assert!(
            matches!(err, AuthError::Unauthenticated),
            "expected Unauthenticated, got {err:?}"
        );
    }

    #[test]
    fn algorithm_display() {
        assert_eq!(JwtAlgorithm::HS512.to_string(), "HS512");
        assert_eq!(JwtAlgorithm::RS512.to_string(), "RS512");
    }

    #[test]
    fn algorithm_parse() {
        assert_eq!(
            "HS512".parse::<JwtAlgorithm>().unwrap(),
            JwtAlgorithm::HS512
        );
        assert_eq!(
            "RS512".parse::<JwtAlgorithm>().unwrap(),
            JwtAlgorithm::RS512
        );
        assert!("HS256".parse::<JwtAlgorithm>().is_err());
    }
}
