// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! JWT authentication against a JSON Web Key Set.

use std::borrow::Cow;
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::sync::Arc;

use aws_lc_rs::encoding::AsDer;
use aws_lc_rs::error::Unspecified;
use aws_lc_rs::rsa::{PublicKey, RsaParameters};
use aws_lc_rs::signature::{
    ECDSA_P256_SHA256_FIXED, ECDSA_P384_SHA384_FIXED, ED25519, ParsedPublicKey,
    RSA_PKCS1_2048_8192_SHA256, RsaPublicKeyComponents, VerificationAlgorithm,
};
use jsonwebtoken::jwk::{
    AlgorithmParameters, CommonParameters, EllipticCurve, JwkSet, KeyOperations, PublicKeyUse,
};
use jsonwebtoken::{
    Algorithm, AlgorithmFamily, DecodingKey, DecodingKeyKind, Validation, decode, decode_header,
};
use opensovd_server::{AuthError, Authenticator, Principal};
use serde_json::{Map, Value};

type Error = Box<dyn std::error::Error + Send + Sync>;

/// Verifies Bearer tokens with the public keys of a JSON Web Key Set.
///
/// A token names its key with `kid`. An RSA key accepts the RS and PS
/// algorithms, an EC or Ed25519 key the one its curve fixes; a key with `alg`
/// accepts only that one.
#[derive(Clone)]
pub struct JwtAuthenticator {
    keys: Arc<HashMap<String, (DecodingKey, Validation)>>,
}

impl JwtAuthenticator {
    /// Loads the signing keys of the JSON Web Key Set `jwks` for tokens issued by `issuer`.
    ///
    /// With an `audience`, a token's `aud` must name one of its entries;
    /// without, `aud` is not checked.
    ///
    /// Keys that cannot verify tokens here are skipped with a warning: keys for
    /// encryption by `use` or `key_ops`, keys without `kid`, and unsupported,
    /// malformed or invalid keys, including one whose `alg` does not fit it.
    ///
    /// # Errors
    ///
    /// Returns an error if `jwks` holds a symmetric key, two usable keys with
    /// the same `kid`, or no usable key.
    pub fn new(jwks: &JwkSet, issuer: &str, audience: &[String]) -> Result<Self, Error> {
        let mut validation = Validation {
            validate_nbf: true,
            ..Validation::default()
        };
        validation.set_issuer(&[issuer]);
        if audience.is_empty() {
            validation.validate_aud = false;
            validation.set_required_spec_claims(&["exp", "iss", "sub"]);
        } else {
            validation.set_audience(audience);
            validation.set_required_spec_claims(&["exp", "iss", "sub", "aud"]);
        }

        let mut keys = HashMap::new();
        for jwk in &jwks.keys {
            let kid = jwk.common.key_id.as_deref().filter(|kid| !kid.is_empty());
            if matches!(jwk.algorithm, AlgorithmParameters::OctetKey(_)) {
                let kid = kid.unwrap_or("without kid");
                return Err(format!("JWKS key {kid} is symmetric").into());
            }
            let skip = |reason: &str| {
                let kid = kid.unwrap_or_default();
                tracing::warn!(target: "srv", kid, reason, "JWKS key skipped");
            };
            if !verifies_signatures(&jwk.common) {
                skip("not a signing key");
                continue;
            }
            let (supported, verifier): (&[Algorithm], &'static dyn VerificationAlgorithm) =
                match &jwk.algorithm {
                    AlgorithmParameters::RSA(_) => (
                        AlgorithmFamily::Rsa.algorithms(),
                        &RSA_PKCS1_2048_8192_SHA256,
                    ),
                    AlgorithmParameters::EllipticCurve(key) => match key.curve {
                        EllipticCurve::P256 => (&[Algorithm::ES256], &ECDSA_P256_SHA256_FIXED),
                        EllipticCurve::P384 => (&[Algorithm::ES384], &ECDSA_P384_SHA384_FIXED),
                        _ => {
                            skip("unsupported curve");
                            continue;
                        }
                    },
                    AlgorithmParameters::OctetKeyPair(key)
                        if key.curve == EllipticCurve::Ed25519 =>
                    {
                        (&[Algorithm::EdDSA], &ED25519)
                    }
                    _ => {
                        skip("unsupported or malformed key");
                        continue;
                    }
                };
            let Some(kid) = kid else {
                skip("no kid");
                continue;
            };
            let algorithms = match jwk.common.key_algorithm.map(Algorithm::try_from) {
                None => supported.to_vec(),
                Some(Ok(alg)) if supported.contains(&alg) => vec![alg],
                Some(Ok(_)) => {
                    skip("alg does not fit the key");
                    continue;
                }
                Some(Err(_)) => {
                    skip("unsupported alg");
                    continue;
                }
            };
            let Ok(key) = DecodingKey::from_jwk(jwk) else {
                skip("invalid key material");
                continue;
            };
            match parse_key(&key, verifier) {
                Ok(true) => {}
                Ok(false) => {
                    skip("unsupported key size");
                    continue;
                }
                Err(_) => {
                    skip("invalid key material");
                    continue;
                }
            }
            let Entry::Vacant(entry) = keys.entry(kid.to_owned()) else {
                return Err(format!("JWKS key {kid} appears twice").into());
            };
            tracing::info!(target: "srv", kid, ?algorithms, "JWKS key loaded");
            let validation = Validation {
                algorithms,
                ..validation.clone()
            };
            entry.insert((key, validation));
        }
        if keys.is_empty() {
            return Err("JWKS holds no usable signing key".into());
        }
        Ok(Self {
            keys: Arc::new(keys),
        })
    }
}

/// Parses `key` as aws-lc does when it verifies a token, which `from_jwk` leaves
/// to the first token, and returns whether aws-lc supports its size.
fn parse_key(
    key: &DecodingKey,
    verifier: &'static dyn VerificationAlgorithm,
) -> Result<bool, Unspecified> {
    match key.kind() {
        DecodingKeyKind::SecretOrDer(bytes) => {
            ParsedPublicKey::new(verifier, bytes)?;
            Ok(true)
        }
        DecodingKeyKind::RsaModulusExponent { n, e } => {
            let der = RsaPublicKeyComponents { n, e }.as_der()?;
            let key = PublicKey::from_der(der.as_ref())?;
            let bits = RsaParameters::public_modulus_len(key.as_ref())?;
            let rsa = &RSA_PKCS1_2048_8192_SHA256;
            Ok((rsa.min_modulus_len()..=rsa.max_modulus_len()).contains(&bits))
        }
    }
}

/// Whether `use` and `key_ops` allow a key to verify signatures.
fn verifies_signatures(common: &CommonParameters) -> bool {
    matches!(common.public_key_use, None | Some(PublicKeyUse::Signature))
        && common
            .key_operations
            .as_ref()
            .is_none_or(|ops| ops.contains(&KeyOperations::Verify))
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
fn bearer(parts: &http::request::Parts) -> Result<Option<&str>, &'static str> {
    let Some(value) = parts.headers.get(http::header::AUTHORIZATION) else {
        return Ok(None);
    };
    let value = value
        .to_str()
        .map_err(|_| "authorization header is not visible ASCII")?;
    let (scheme, token) = value.split_once(' ').unwrap_or((value, ""));
    if !scheme.eq_ignore_ascii_case("bearer") {
        return Ok(None);
    }
    match token.trim_start() {
        "" => Err("empty bearer token"),
        token => Ok(Some(token)),
    }
}

fn invalid(reason: impl Into<Cow<'static, str>>) -> AuthError {
    AuthError::InvalidToken(reason.into())
}

impl Authenticator for JwtAuthenticator {
    fn authenticate(&self, parts: &http::request::Parts) -> Result<Option<Principal>, AuthError> {
        let Some(token) = bearer(parts).map_err(invalid)? else {
            return Ok(None);
        };
        let header = decode_header(token).map_err(|e| invalid(e.to_string()))?;
        if header.crit.is_some() {
            return Err(invalid("critical extension"));
        }
        let kid = header.kid.ok_or_else(|| invalid("no kid"))?;
        let (key, validation) = self
            .keys
            .get(&kid)
            .ok_or_else(|| invalid(format!("unknown kid {kid}")))?;
        let token_data = decode::<Map<String, Value>>(token, key, validation)
            .map_err(|e| invalid(e.to_string()))?;
        Ok(Some(principal(token_data.claims)))
    }
}

#[cfg(test)]
#[expect(clippy::arithmetic_side_effects)]
mod tests {
    use std::sync::LazyLock;

    use aws_lc_rs::rsa::KeySize;
    use aws_lc_rs::signature::{ECDSA_P256_SHA256_FIXED_SIGNING, EcdsaKeyPair, RsaKeyPair};
    use base64::Engine;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use http::Request;
    use jsonwebtoken::jwk::{Jwk, KeyAlgorithm};
    use jsonwebtoken::{EncodingKey, Header, encode};

    use super::*;

    const TEST_ISSUER: &str = "OpenSOVD";

    struct Key {
        kid: &'static str,
        alg: Algorithm,
        encoding: EncodingKey,
    }

    impl Key {
        fn jwk(&self) -> Jwk {
            let mut jwk = Jwk::from_encoding_key(&self.encoding, self.alg).unwrap();
            jwk.common.key_id = Some(self.kid.to_owned());
            jwk
        }

        fn token(&self, claims: &Value) -> String {
            let mut header = Header::new(self.alg);
            header.kid = Some(self.kid.to_owned());
            encode(&header, claims, &self.encoding).unwrap()
        }
    }

    static RSA: LazyLock<Key> = LazyLock::new(|| {
        let pkcs8 = RsaKeyPair::generate(KeySize::Rsa2048)
            .unwrap()
            .as_der()
            .unwrap();
        // `from_rsa_der` wants PKCS#1; the PEM path also takes PKCS#8.
        let b64 = base64::engine::general_purpose::STANDARD.encode(pkcs8.as_ref());
        let pem = format!("-----BEGIN PRIVATE KEY-----\n{b64}\n-----END PRIVATE KEY-----\n");
        Key {
            kid: "rsa",
            alg: Algorithm::RS512,
            encoding: EncodingKey::from_rsa_pem(pem.as_bytes()).unwrap(),
        }
    });

    static EC: LazyLock<Key> = LazyLock::new(|| ec_key("ec"));

    fn ec_key(kid: &'static str) -> Key {
        let pkcs8 = EcdsaKeyPair::generate(&ECDSA_P256_SHA256_FIXED_SIGNING)
            .unwrap()
            .to_pkcs8v1()
            .unwrap();
        Key {
            kid,
            alg: Algorithm::ES256,
            encoding: EncodingKey::from_ec_der(pkcs8.as_ref()),
        }
    }

    fn jwks(keys: &[Jwk]) -> JwkSet {
        JwkSet {
            keys: keys.to_vec(),
        }
    }

    fn authenticator() -> JwtAuthenticator {
        JwtAuthenticator::new(&jwks(&[RSA.jwk(), EC.jwk()]), TEST_ISSUER, &[]).unwrap()
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

    fn bearer(token: &str) -> http::request::Parts {
        parts(Some(&format!("Bearer {token}")))
    }

    fn assert_invalid(result: &Result<Option<Principal>, AuthError>) {
        assert!(
            matches!(result, Err(AuthError::InvalidToken(_))),
            "expected InvalidToken, got {result:?}"
        );
    }

    fn load_error(keys: &[Jwk]) -> String {
        match JwtAuthenticator::new(&jwks(keys), TEST_ISSUER, &[]) {
            Ok(_) => panic!("expected the key set to be refused"),
            Err(e) => e.to_string(),
        }
    }

    #[test]
    fn rsa_token_is_accepted() {
        let token = RSA.token(&valid_claims());
        let principal = authenticator()
            .authenticate(&bearer(&token))
            .unwrap()
            .unwrap();
        assert_eq!(principal.subject, "user@example.com");
        assert_eq!(principal.roles, vec!["reader", "admin"]);
        assert_eq!(principal.scopes, vec!["read", "write"]);
        assert_eq!(principal.claims["iss"], TEST_ISSUER);
    }

    #[test]
    fn ec_token_is_accepted() {
        let token = EC.token(&valid_claims());
        let principal = authenticator().authenticate(&bearer(&token)).unwrap();
        assert!(principal.is_some());
    }

    #[test]
    fn token_without_kid_is_rejected() {
        let token = encode(&Header::new(RSA.alg), &valid_claims(), &RSA.encoding).unwrap();
        assert_invalid(&authenticator().authenticate(&bearer(&token)));
    }

    #[test]
    fn unknown_kid_is_rejected() {
        let mut header = Header::new(RSA.alg);
        header.kid = Some("other".to_owned());
        let token = encode(&header, &valid_claims(), &RSA.encoding).unwrap();
        assert_invalid(&authenticator().authenticate(&bearer(&token)));
    }

    #[test]
    fn token_signed_by_another_key_is_rejected() {
        let token = ec_key(EC.kid).token(&valid_claims());
        assert_invalid(&authenticator().authenticate(&bearer(&token)));
    }

    fn audience_authenticator() -> JwtAuthenticator {
        let audience = ["sovd".to_owned(), "diag".to_owned()];
        JwtAuthenticator::new(&jwks(&[RSA.jwk()]), TEST_ISSUER, &audience).unwrap()
    }

    fn bearer_for(aud: Option<Value>) -> http::request::Parts {
        let mut claims = valid_claims();
        if let Some(aud) = aud {
            claims["aud"] = aud;
        }
        bearer(&RSA.token(&claims))
    }

    #[test]
    fn token_for_the_audience_is_accepted() {
        let auth = audience_authenticator();
        let single = auth.authenticate(&bearer_for(Some("diag".into())));
        assert!(single.unwrap().is_some());
        let multiple = auth.authenticate(&bearer_for(Some(serde_json::json!(["other", "sovd"]))));
        assert!(multiple.unwrap().is_some());
    }

    #[test]
    fn token_for_another_audience_is_rejected() {
        let auth = audience_authenticator();
        assert_invalid(&auth.authenticate(&bearer_for(Some("other".into()))));
        assert_invalid(&auth.authenticate(&bearer_for(Some(serde_json::json!(["other"])))));
    }

    #[test]
    fn token_without_audience_is_rejected_when_one_is_set() {
        assert_invalid(&audience_authenticator().authenticate(&bearer_for(None)));
    }

    #[test]
    fn audience_is_not_checked_without_one() {
        let mut claims = valid_claims();
        claims["aud"] = "other".into();
        let principal = authenticator().authenticate(&bearer(&RSA.token(&claims)));
        assert!(principal.unwrap().is_some());
    }

    #[test]
    fn critical_extension_is_rejected() {
        let mut header = Header::new(RSA.alg);
        header.kid = Some(RSA.kid.to_owned());
        header.crit = Some(vec!["b64".to_owned()]);
        let token = encode(&header, &valid_claims(), &RSA.encoding).unwrap();
        assert_invalid(&authenticator().authenticate(&bearer(&token)));
    }

    #[test]
    fn token_of_another_key_type_is_rejected() {
        let mut header = Header::new(EC.alg);
        header.kid = Some(RSA.kid.to_owned());
        let token = encode(&header, &valid_claims(), &EC.encoding).unwrap();
        assert_invalid(&authenticator().authenticate(&bearer(&token)));
    }

    #[test]
    fn hmac_token_naming_a_public_key_is_rejected() {
        let mut header = Header::new(Algorithm::HS256);
        header.kid = Some(RSA.kid.to_owned());
        let secret = EncodingKey::from_secret(b"guessed");
        let token = encode(&header, &valid_claims(), &secret).unwrap();
        assert_invalid(&authenticator().authenticate(&bearer(&token)));
    }

    #[test]
    fn key_alg_is_the_only_accepted_algorithm() {
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some(RSA.kid.to_owned());
        let token = encode(&header, &valid_claims(), &RSA.encoding).unwrap();
        assert_invalid(&authenticator().authenticate(&bearer(&token)));
    }

    #[test]
    fn key_without_alg_accepts_its_key_type() {
        let mut jwk = RSA.jwk();
        jwk.common.key_algorithm = None;
        let auth = JwtAuthenticator::new(&jwks(&[jwk]), TEST_ISSUER, &[]).unwrap();
        let mut header = Header::new(Algorithm::PS256);
        header.kid = Some(RSA.kid.to_owned());
        let token = encode(&header, &valid_claims(), &RSA.encoding).unwrap();
        assert!(auth.authenticate(&bearer(&token)).unwrap().is_some());
    }

    #[test]
    fn symmetric_key_is_refused() {
        let secret = EncodingKey::from_secret(b"shared");
        let mut jwk = Jwk::from_encoding_key(&secret, Algorithm::HS256).unwrap();
        jwk.common.key_id = Some("hmac".to_owned());
        assert!(load_error(&[RSA.jwk(), jwk.clone()]).contains("symmetric"));
        jwk.common.public_key_use = Some(PublicKeyUse::Encryption);
        assert!(load_error(&[RSA.jwk(), jwk]).contains("symmetric"));
    }

    #[test]
    fn duplicate_kid_is_refused() {
        assert!(load_error(&[RSA.jwk(), RSA.jwk()]).contains("twice"));
    }

    #[test]
    fn keys_that_cannot_verify_are_skipped() {
        let mut no_kid = RSA.jwk();
        no_kid.common.key_id = None;
        let mut empty_kid = RSA.jwk();
        empty_kid.common.key_id = Some(String::new());
        let mut alg_of_another_type = RSA.jwk();
        alg_of_another_type.common.key_algorithm = Some(KeyAlgorithm::ES256);
        let mut alg_of_another_curve = EC.jwk();
        alg_of_another_curve.common.key_algorithm = Some(KeyAlgorithm::ES384);
        let mut off_curve = EC.jwk();
        if let AlgorithmParameters::EllipticCurve(key) = &mut off_curve.algorithm {
            let mut y = URL_SAFE_NO_PAD.decode(&key.y).unwrap();
            *y.last_mut().unwrap() ^= 1;
            key.y = URL_SAFE_NO_PAD.encode(y);
        }
        let mut without_y = serde_json::to_value(EC.jwk()).unwrap();
        without_y.as_object_mut().unwrap().remove("y");
        let malformed: Jwk = serde_json::from_value(without_y).unwrap();
        let mut encryption = EC.jwk();
        encryption.common.public_key_use = Some(PublicKeyUse::Encryption);
        let mut encrypt_only = EC.jwk();
        encrypt_only.common.key_operations = Some(vec![KeyOperations::Encrypt]);
        let mut oaep = RSA.jwk();
        oaep.common.key_algorithm = Some(KeyAlgorithm::RSA_OAEP);
        let mut p521 = EC.jwk();
        if let AlgorithmParameters::EllipticCurve(key) = &mut p521.algorithm {
            key.curve = EllipticCurve::P521;
        }
        let mut rsa1024 = RSA.jwk();
        if let AlgorithmParameters::RSA(key) = &mut rsa1024.algorithm {
            let mut n = URL_SAFE_NO_PAD.decode(&key.n).unwrap();
            n.truncate(128);
            *n.last_mut().unwrap() |= 1;
            key.n = URL_SAFE_NO_PAD.encode(n);
        }
        let mut unknown_alg = EC.jwk();
        unknown_alg.common.key_algorithm = Some(KeyAlgorithm::UNKNOWN_ALGORITHM);
        let unusable = [
            no_kid,
            empty_kid,
            alg_of_another_type,
            alg_of_another_curve,
            off_curve,
            malformed,
            encryption,
            encrypt_only,
            oaep,
            p521,
            rsa1024,
            unknown_alg,
        ];
        let mut usable = unusable.to_vec();
        usable.push(RSA.jwk());
        let auth = JwtAuthenticator::new(&jwks(&usable), TEST_ISSUER, &[]).unwrap();
        assert!(
            auth.authenticate(&bearer(&RSA.token(&valid_claims())))
                .unwrap()
                .is_some()
        );
        assert!(load_error(&unusable).contains("no usable signing key"));
    }

    #[test]
    fn bearer_scheme_is_case_insensitive() {
        let token = RSA.token(&valid_claims());
        let principal = authenticator().authenticate(&parts(Some(&format!("bearer {token}"))));
        assert!(principal.unwrap().is_some());
    }

    #[test]
    fn missing_header_is_anonymous() {
        assert!(
            authenticator()
                .authenticate(&parts(None))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn other_scheme_is_anonymous() {
        let principal = authenticator().authenticate(&parts(Some("Basic dXNlcjpwYXNz")));
        assert!(principal.unwrap().is_none());
    }

    #[test]
    fn bearer_without_token_is_rejected() {
        assert_invalid(&authenticator().authenticate(&parts(Some("Bearer"))));
        assert_invalid(&authenticator().authenticate(&parts(Some("Bearer   "))));
    }

    fn rejection_reason(parts: &http::request::Parts) -> String {
        match authenticator().authenticate(parts) {
            Err(AuthError::InvalidToken(reason)) => reason.into_owned(),
            other => panic!("expected InvalidToken, got {other:?}"),
        }
    }

    #[test]
    fn rejection_names_the_reason() {
        let mut claims = valid_claims();
        claims["exp"] = 1.into();
        assert_eq!(
            rejection_reason(&bearer(&RSA.token(&claims))),
            "ExpiredSignature"
        );
        assert_eq!(
            rejection_reason(&parts(Some("Bearer"))),
            "empty bearer token"
        );
        let mut header = Header::new(RSA.alg);
        header.kid = Some("other".to_owned());
        let token = encode(&header, &valid_claims(), &RSA.encoding).unwrap();
        assert_eq!(rejection_reason(&bearer(&token)), "unknown kid other");
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
        assert_invalid(&authenticator().authenticate(&parts));
    }

    #[test]
    fn malformed_token_is_rejected() {
        assert_invalid(&authenticator().authenticate(&bearer("not-a-valid-jwt")));
    }

    #[test]
    fn expired_token_is_rejected() {
        let mut claims = valid_claims();
        claims["exp"] = 1.into();
        assert_invalid(&authenticator().authenticate(&bearer(&RSA.token(&claims))));
    }

    #[test]
    fn wrong_issuer_is_rejected() {
        let mut claims = valid_claims();
        claims["iss"] = "wrong-issuer".into();
        assert_invalid(&authenticator().authenticate(&bearer(&RSA.token(&claims))));
    }

    #[test]
    fn missing_subject_is_rejected() {
        let mut claims = valid_claims();
        claims.as_object_mut().unwrap().remove("sub");
        assert_invalid(&authenticator().authenticate(&bearer(&RSA.token(&claims))));
    }
}
