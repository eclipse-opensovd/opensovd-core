// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Rego policy-based authorization using regorus.

use std::io::Read;
use std::path::Path;
use std::sync::{Arc, RwLock};

use opensovd_server::{AuthError, Authorizer};
use regorus::Engine;

use super::jwt::Claims;

/// Rego policy authorizer backed by the regorus engine.
#[derive(Clone)]
pub struct RegorusAuthorizer {
    engine: Arc<RwLock<Engine>>,
}

impl RegorusAuthorizer {
    /// Create an authorizer from named policy and data readers.
    ///
    /// Each policy is a `(name, reader)` tuple.
    ///
    /// # Errors
    ///
    /// Returns an error if any reader fails or content cannot be parsed.
    pub fn new(
        policies: &mut [(&str, &mut dyn Read)],
        data: &mut [&mut dyn Read],
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let mut engine = Engine::new();

        for (i, (name, reader)) in policies.iter_mut().enumerate() {
            let mut rego = String::new();
            reader.read_to_string(&mut rego)?;
            tracing::info!(index = i, policy = %name, "Loading rego policy");
            engine.add_policy((*name).to_owned(), rego)?;
        }

        for (i, reader) in data.iter_mut().enumerate() {
            let mut json = String::new();
            reader.read_to_string(&mut json)?;
            tracing::info!(index = i, "Loading rego data");
            engine.add_data(regorus::Value::from_json_str(&json)?)?;
        }

        Ok(Self {
            engine: Arc::new(RwLock::new(engine)),
        })
    }

    /// Create an authorizer from policy and data file paths.
    ///
    /// # Errors
    ///
    /// Returns an error if any file cannot be opened/read or content cannot be parsed.
    pub fn from_paths(
        policies: &[impl AsRef<Path>],
        data: &[impl AsRef<Path>],
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let mut engine = Engine::new();

        for (i, path) in policies.iter().enumerate() {
            let path = path.as_ref();
            let rego =
                std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
            tracing::info!(index = i, policy = %path.display(), "Loading rego policy");
            engine.add_policy(path.display().to_string(), rego)?;
        }

        for (i, path) in data.iter().enumerate() {
            let path = path.as_ref();
            let json =
                std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
            tracing::info!(index = i, "Loading rego data");
            engine.add_data(regorus::Value::from_json_str(&json)?)?;
        }

        Ok(Self {
            engine: Arc::new(RwLock::new(engine)),
        })
    }
}

impl Authorizer<Claims> for RegorusAuthorizer {
    async fn authorize(
        &self,
        identity: &Claims,
        parts: &http::request::Parts,
    ) -> Result<(), AuthError> {
        // Clone the engine out of the rwlock so the lock is held briefly.
        // The clone carries all loaded policies and data.
        let mut eval_engine = {
            let guard = self.engine.read().map_err(|e| {
                tracing::error!(error = %e, "Policy engine rwlock poisoned");
                AuthError::unauthorized()
            })?;
            guard.clone()
        };

        let input = serde_json::json!({
            "method": parts.method.as_str(),
            "path": parts.uri.path(),
            "identity": {
                "sub": identity.sub,
                "roles": identity.roles,
                "scope": identity.scope,
            }
        });
        eval_engine.set_input(regorus::Value::from(input));

        let allowed = match eval_engine.eval_bool_query("data.sovd.authz.allow".to_owned(), false) {
            Ok(result) => result,
            Err(e) => {
                tracing::error!(error = %e, "Rego policy evaluation failed");
                false
            }
        };

        if allowed {
            Ok(())
        } else {
            Err(AuthError::unauthorized())
        }
    }
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
#[allow(clippy::ignored_unit_patterns)]
mod tests {
    use super::*;

    const POLICY: &str = r#"
package sovd.authz
import rego.v1

default allow := false

allow if {
    input.method == "GET"
    "reader" in input.identity.roles
}

allow if {
    "admin" in input.identity.roles
}
"#;

    fn make_authorizer() -> RegorusAuthorizer {
        RegorusAuthorizer::new(
            &mut [("test", &mut POLICY.as_bytes() as &mut dyn std::io::Read)],
            &mut [],
        )
        .unwrap()
    }

    fn make_claims(roles: Vec<String>) -> Claims {
        Claims {
            sub: "test-user".to_owned(),
            exp: 9_999_999_999,
            iat: None,
            iss: None,
            roles,
            scope: None,
        }
    }

    fn make_parts(method: http::Method, uri: &str) -> http::request::Parts {
        http::Request::builder()
            .method(method)
            .uri(uri)
            .body(())
            .unwrap()
            .into_parts()
            .0
    }

    #[tokio::test]
    async fn reader_get_allowed() {
        let authz = make_authorizer();
        let claims = make_claims(vec!["reader".to_owned()]);
        let parts = make_parts(http::Method::GET, "/components");

        assert!(authz.authorize(&claims, &parts).await.is_ok());
    }

    #[tokio::test]
    async fn reader_put_denied() {
        let authz = make_authorizer();
        let claims = make_claims(vec!["reader".to_owned()]);
        let parts = make_parts(http::Method::PUT, "/components/ecu/data/speed");

        let err = authz.authorize(&claims, &parts).await.unwrap_err();
        assert!(matches!(err, AuthError::Unauthorized));
    }

    #[tokio::test]
    async fn admin_put_allowed() {
        let authz = make_authorizer();
        let claims = make_claims(vec!["admin".to_owned()]);
        let parts = make_parts(http::Method::PUT, "/components/ecu/data/speed");

        assert!(authz.authorize(&claims, &parts).await.is_ok());
    }

    #[tokio::test]
    async fn no_roles_denied() {
        let authz = make_authorizer();
        let claims = make_claims(vec![]);
        let parts = make_parts(http::Method::GET, "/components");

        let err = authz.authorize(&claims, &parts).await.unwrap_err();
        assert!(matches!(err, AuthError::Unauthorized));
    }

    #[test]
    fn invalid_rego_content_errors() {
        let result = RegorusAuthorizer::new(
            &mut [(
                "bad",
                &mut "not valid rego {{{{".as_bytes() as &mut dyn std::io::Read,
            )],
            &mut [],
        );
        assert!(result.is_err());
    }
}
