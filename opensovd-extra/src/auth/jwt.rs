// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! JWT authentication supporting HS512 and RS512 algorithms.

use std::fmt;
use std::str::FromStr;
use std::sync::Arc;

use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode};
use opensovd_server::{AuthError, Authenticator, Principal};
use serde_json::{Map, Value};

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

/// JWT authenticator supporting HS512 and RS512.
///
/// Verifies the Bearer token of the `Authorization` header and returns its
/// claims as a [`Principal`].
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
        validation.validate_nbf = true;
        validation.set_issuer(&[issuer]);
        validation.set_required_spec_claims(&["exp", "iss", "sub"]);

        Self {
            decoding_key: Arc::new(decoding_key),
            validation: Arc::new(validation),
        }
    }
}

/// Builds a principal from `sub`, the `roles` array and `scope` (space-separated
/// or an array).
fn principal(claims: Map<String, Value>) -> Principal {
    let list = |value: Option<&Value>| -> Vec<String> {
        match value {
            Some(Value::Array(items)) => items
                .iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect(),
            _ => Vec::new(),
        }
    };
    Principal {
        subject: claims
            .get("sub")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        roles: list(claims.get("roles")),
        scopes: match claims.get("scope") {
            Some(Value::String(scope)) => scope.split_whitespace().map(str::to_owned).collect(),
            other => list(other),
        },
        claims,
    }
}

/// The token of a `Bearer` authorization; other schemes count as no credentials.
fn bearer(parts: &http::request::Parts) -> Result<Option<&str>, AuthError> {
    let Some(value) = parts.headers.get(http::header::AUTHORIZATION) else {
        return Ok(None);
    };
    let value = value.to_str().map_err(|_| AuthError::InvalidToken)?;
    let (scheme, token) = value.split_once(' ').unwrap_or((value, ""));
    if !scheme.eq_ignore_ascii_case("bearer") {
        return Ok(None);
    }
    match token.trim_start() {
        "" => Err(AuthError::InvalidToken),
        token => Ok(Some(token)),
    }
}

impl Authenticator for JwtAuthenticator {
    fn authenticate(&self, parts: &http::request::Parts) -> Result<Option<Principal>, AuthError> {
        let Some(token) = bearer(parts)? else {
            return Ok(None);
        };
        let token_data = decode::<Map<String, Value>>(token, &self.decoding_key, &self.validation)
            .map_err(|e| {
                tracing::debug!(target: "srv", error = %e, "JWT validation failed");
                AuthError::InvalidToken
            })?;
        Ok(Some(principal(token_data.claims)))
    }
}

#[cfg(test)]
#[expect(clippy::arithmetic_side_effects)]
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

    fn make_hs512_token(claims: &Value) -> String {
        encode(
            &Header::new(Algorithm::HS512),
            claims,
            &EncodingKey::from_secret(&HMAC_SECRET),
        )
        .unwrap()
    }

    fn valid_claims() -> Value {
        let now = jsonwebtoken::get_current_timestamp();
        serde_json::json!({
            "sub": "user@example.com",
            "exp": now + 3600,
            "iat": now,
            "iss": TEST_ISSUER,
            "roles": ["reader", "admin"],
            "scope": "read write",
        })
    }

    fn parts(authorization: Option<&str>) -> http::request::Parts {
        let mut request = Request::builder();
        if let Some(value) = authorization {
            request = request.header("Authorization", value);
        }
        request.body(()).unwrap().into_parts().0
    }

    fn hs512() -> JwtAuthenticator {
        JwtAuthenticator::new(JwtAlgorithm::HS512, &HMAC_SECRET, TEST_ISSUER)
    }

    fn assert_invalid(result: &Result<Option<Principal>, AuthError>) {
        assert!(
            matches!(result, Err(AuthError::InvalidToken)),
            "expected InvalidToken, got {result:?}"
        );
    }

    #[test]
    fn hs512_valid_token() {
        let token = make_hs512_token(&valid_claims());
        let principal = hs512()
            .authenticate(&parts(Some(&format!("Bearer {token}"))))
            .unwrap()
            .unwrap();
        assert_eq!(principal.subject, "user@example.com");
        assert_eq!(principal.roles, vec!["reader", "admin"]);
        assert_eq!(principal.scopes, vec!["read", "write"]);
        assert_eq!(principal.claims["iss"], TEST_ISSUER);
    }

    #[test]
    fn bearer_scheme_is_case_insensitive() {
        let token = make_hs512_token(&valid_claims());
        let principal = hs512().authenticate(&parts(Some(&format!("bearer {token}"))));
        assert!(principal.unwrap().is_some());
    }

    #[test]
    fn missing_header_is_anonymous() {
        assert!(hs512().authenticate(&parts(None)).unwrap().is_none());
    }

    #[test]
    fn other_scheme_is_anonymous() {
        let principal = hs512().authenticate(&parts(Some("Basic dXNlcjpwYXNz")));
        assert!(principal.unwrap().is_none());
    }

    #[test]
    fn bearer_without_token_is_rejected() {
        assert_invalid(&hs512().authenticate(&parts(Some("Bearer"))));
        assert_invalid(&hs512().authenticate(&parts(Some("Bearer   "))));
    }

    #[test]
    fn non_ascii_header_is_rejected() {
        let value = http::HeaderValue::from_bytes(b"Bearer \xff").unwrap();
        let parts = Request::builder()
            .header("Authorization", value)
            .body(())
            .unwrap()
            .into_parts()
            .0;
        assert_invalid(&hs512().authenticate(&parts));
    }

    #[test]
    fn hs512_invalid_token_is_rejected() {
        assert_invalid(&hs512().authenticate(&parts(Some("Bearer not-a-valid-jwt"))));
    }

    #[test]
    fn hs512_expired_token_is_rejected() {
        let mut claims = valid_claims();
        claims["exp"] = 1.into();
        let token = make_hs512_token(&claims);
        assert_invalid(&hs512().authenticate(&parts(Some(&format!("Bearer {token}")))));
    }

    #[test]
    fn hs512_wrong_secret_is_rejected() {
        let auth = JwtAuthenticator::new(JwtAlgorithm::HS512, b"different-secret", TEST_ISSUER);
        let token = make_hs512_token(&valid_claims());
        assert_invalid(&auth.authenticate(&parts(Some(&format!("Bearer {token}")))));
    }

    #[test]
    fn hs512_wrong_issuer_is_rejected() {
        let mut claims = valid_claims();
        claims["iss"] = "wrong-issuer".into();
        let token = make_hs512_token(&claims);
        assert_invalid(&hs512().authenticate(&parts(Some(&format!("Bearer {token}")))));
    }

    #[test]
    fn hs512_missing_subject_is_rejected() {
        let mut claims = valid_claims();
        claims.as_object_mut().unwrap().remove("sub");
        let token = make_hs512_token(&claims);
        assert_invalid(&hs512().authenticate(&parts(Some(&format!("Bearer {token}")))));
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

    fn make_rs512_token(claims: &Value) -> String {
        encode(
            &Header::new(Algorithm::RS512),
            claims,
            &EncodingKey::from_rsa_pem(&RSA_KEYS.private_pem).unwrap(),
        )
        .unwrap()
    }

    fn rs512() -> JwtAuthenticator {
        JwtAuthenticator::new(JwtAlgorithm::RS512, &RSA_KEYS.public_der, TEST_ISSUER)
    }

    #[test]
    fn rs512_valid_token() {
        let token = make_rs512_token(&valid_claims());
        let principal = rs512()
            .authenticate(&parts(Some(&format!("Bearer {token}"))))
            .unwrap()
            .unwrap();
        assert_eq!(principal.subject, "user@example.com");
        assert_eq!(principal.roles, vec!["reader", "admin"]);
    }

    #[test]
    fn rs512_expired_token_is_rejected() {
        let mut claims = valid_claims();
        claims["exp"] = 1.into();
        let token = make_rs512_token(&claims);
        assert_invalid(&rs512().authenticate(&parts(Some(&format!("Bearer {token}")))));
    }

    #[test]
    fn rs512_invalid_token_is_rejected() {
        assert_invalid(&rs512().authenticate(&parts(Some("Bearer not-a-valid-jwt"))));
    }

    #[test]
    fn rs512_rejects_hs512_tokens() {
        let token = make_hs512_token(&valid_claims());
        assert_invalid(&rs512().authenticate(&parts(Some(&format!("Bearer {token}")))));
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
