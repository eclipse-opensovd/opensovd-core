// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

use std::path::{Path, PathBuf};

use anyhow::Context;
use clap::Args;
use clap::builder::NonEmptyStringValueParser;
use opensovd_extra::{JwtAuthenticator, RegorusAuthorizer};
use opensovd_server::{Auth, Authenticated};

use crate::TARGET;

const HEADING: &str = "Authentication & Authorization";

/// The `--auth-*` options.
#[derive(Args)]
#[group(skip)]
#[command(about = None, long_about = None)]
pub struct AuthArgs {
    /// JSON Web Key Set with the public keys that sign tokens; a token names its key with `kid`.
    #[arg(
        id = "auth_jwks",
        help_heading = HEADING,
        long = "auth-jwks",
        value_name = "FILE",
        env = "SOVD_AUTH_JWKS",
        requires = "auth_issuer"
    )]
    jwks: Option<PathBuf>,

    /// The `iss` (issuer) claim tokens must carry.
    #[arg(
        id = "auth_issuer",
        help_heading = HEADING,
        long = "auth-issuer",
        value_name = "ISSUER",
        env = "SOVD_AUTH_ISSUER",
        value_parser = NonEmptyStringValueParser::new(),
        requires = "auth_jwks"
    )]
    issuer: Option<String>,

    /// An `aud` (audience) claim value accepted in tokens, repeatable; without one, `aud` is not checked.
    #[arg(
        id = "auth_audience",
        help_heading = HEADING,
        long = "auth-audience",
        value_name = "AUDIENCE",
        env = "SOVD_AUTH_AUDIENCE",
        value_delimiter = ',',
        value_parser = NonEmptyStringValueParser::new(),
        requires = "auth_jwks"
    )]
    audience: Vec<String>,

    /// Rego policy file defining `data.sovd.authz.allow`.
    #[arg(
        id = "auth_policy",
        help_heading = HEADING,
        long = "auth-policy",
        value_name = "FILE",
        env = "SOVD_AUTH_POLICY",
        requires = "auth_jwks"
    )]
    policy: Vec<PathBuf>,

    /// JSON data file for Rego policies.
    #[arg(
        id = "auth_policy_data",
        help_heading = HEADING,
        long = "auth-policy-data",
        value_name = "FILE",
        env = "SOVD_AUTH_POLICY_DATA",
        requires = "auth_policy"
    )]
    policy_data: Vec<PathBuf>,
}

impl AuthArgs {
    /// Whether `--auth-jwks` is set.
    #[must_use]
    pub fn enabled(&self) -> bool {
        self.jwks.is_some()
    }

    /// Authentication and authorization, or `None` without `--auth-jwks`.
    ///
    /// # Errors
    ///
    /// Returns an error if the key set or a policy cannot be loaded.
    pub fn auth(&self) -> anyhow::Result<Option<Auth>> {
        let Some(jwks) = &self.jwks else {
            return Ok(None);
        };
        let jwt = self.jwt_authenticator(jwks)?;
        Ok(Some(if self.policy.is_empty() {
            Auth::new(jwt, Authenticated)
        } else {
            Auth::new(jwt, self.rego_authorizer()?)
        }))
    }

    fn jwt_authenticator(&self, jwks: &Path) -> anyhow::Result<JwtAuthenticator> {
        let text = std::fs::read_to_string(jwks)
            .with_context(|| format!("failed to read --auth-jwks {}", jwks.display()))?;
        let invalid = || format!("invalid --auth-jwks {}", jwks.display());
        let set: serde_json::Value = serde_json::from_str(&text).with_context(invalid)?;
        if let Some(kid) = private_key(&set) {
            anyhow::bail!("--auth-jwks {} holds the private key {kid}", jwks.display());
        }
        let set = serde_json::from_value(set).with_context(invalid)?;
        let issuer = self
            .issuer
            .as_deref()
            .context("--auth-jwks requires --auth-issuer")?;
        let audience = &self.audience;
        let authenticator = JwtAuthenticator::new(&set, issuer, audience)
            .map_err(anyhow::Error::from_boxed)
            .with_context(invalid)?;

        tracing::info!(target: TARGET, jwks = %jwks.display(), %issuer, ?audience, "JWT authentication enabled");
        Ok(authenticator)
    }

    fn rego_authorizer(&self) -> anyhow::Result<RegorusAuthorizer> {
        let authorizer = RegorusAuthorizer::from_paths(&self.policy, &self.policy_data)
            .map_err(anyhow::Error::from_boxed)
            .context("failed to load Rego authorization policies")?;
        tracing::info!(target: TARGET, count = self.policy.len(), "Rego policy authorization enabled");
        Ok(authorizer)
    }
}

/// The `kid` of the first key in `jwks` with private key material.
fn private_key(jwks: &serde_json::Value) -> Option<&str> {
    let keys = jwks.get("keys")?.as_array()?;
    let key = keys.iter().find(|key| key.get("d").is_some())?;
    Some(
        key.get("kid")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("without kid"),
    )
}
