// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use anyhow::Context;
use clap::error::ErrorKind;
use clap::{ArgMatches, Args, Command, FromArgMatches};
use tower_http::cors::{self, CorsLayer};

const HEADING: &str = "CORS Options";

/// The `--cors-*` options.
///
/// Parsing rejects invalid values and wildcards combined with
/// `--cors-credentials`.
pub struct CorsArgs {
    options: Options,
    layer: Option<CorsLayer>,
}

#[derive(Args)]
#[group(skip)]
struct Options {
    /// Allowed CORS origins. Use '*' for any origin.
    #[arg(id = "cors_origin", help_heading = HEADING, long = "cors-origin", value_name = "ORIGIN")]
    origins: Vec<String>,

    /// Allowed CORS methods. Use '*' for any method.
    #[arg(id = "cors_method", help_heading = HEADING, long = "cors-method", value_name = "METHOD")]
    methods: Vec<String>,

    /// Allowed CORS headers. Use '*' for any header.
    #[arg(id = "cors_header", help_heading = HEADING, long = "cors-header", value_name = "HEADER")]
    headers: Vec<String>,

    /// Allow credentials in CORS requests.
    #[arg(id = "cors_credentials", help_heading = HEADING, long = "cors-credentials")]
    credentials: bool,

    /// Max age for CORS preflight cache in seconds.
    #[arg(id = "cors_max_age", help_heading = HEADING, long = "cors-max-age", value_name = "SECONDS")]
    max_age: Option<u64>,
}

impl Options {
    fn layer(&self) -> Result<Option<CorsLayer>, clap::Error> {
        create_cors_layer(
            &self.origins,
            &self.methods,
            &self.headers,
            self.credentials,
            self.max_age,
        )
        .map_err(|e| clap::Error::raw(ErrorKind::ValueValidation, e))
    }
}

impl FromArgMatches for CorsArgs {
    fn from_arg_matches(matches: &ArgMatches) -> Result<Self, clap::Error> {
        let options = Options::from_arg_matches(matches)?;
        let layer = options.layer()?;
        Ok(Self { options, layer })
    }

    fn update_from_arg_matches(&mut self, matches: &ArgMatches) -> Result<(), clap::Error> {
        self.options.update_from_arg_matches(matches)?;
        self.layer = self.options.layer()?;
        Ok(())
    }
}

impl Args for CorsArgs {
    fn augment_args(cmd: Command) -> Command {
        Options::augment_args(cmd)
    }

    fn augment_args_for_update(cmd: Command) -> Command {
        Options::augment_args_for_update(cmd)
    }
}

impl CorsArgs {
    /// The CORS layer, or `None` without `--cors-origin`.
    #[must_use]
    pub fn layer(&self) -> Option<CorsLayer> {
        self.layer.clone()
    }
}

/// Creates an optional CORS layer from raw string slices.
///
/// Returns `Ok(None)` when no origins are specified (CORS disabled).
///
/// # Errors
///
/// Returns an error if any origin, method, or header string is invalid,
/// or if credentials are enabled with wildcard origins, methods, or headers.
fn create_cors_layer(
    origins: &[String],
    methods: &[String],
    headers: &[String],
    credentials: bool,
    max_age: Option<u64>,
) -> anyhow::Result<Option<CorsLayer>> {
    if origins.is_empty() {
        return Ok(None);
    }

    let wildcard_origins = origins.iter().any(|v| v == "*");
    let wildcard_methods = methods.iter().any(|v| v == "*");
    let wildcard_headers = headers.iter().any(|v| v == "*");

    if credentials {
        anyhow::ensure!(
            !wildcard_origins,
            "cannot use wildcard '*' for origins when credentials are enabled"
        );
        anyhow::ensure!(
            !wildcard_methods,
            "cannot use wildcard '*' for methods when credentials are enabled"
        );
        anyhow::ensure!(
            !wildcard_headers,
            "cannot use wildcard '*' for headers when credentials are enabled"
        );
    }

    let mut layer = CorsLayer::new();

    layer = if wildcard_origins {
        layer.allow_origin(cors::Any)
    } else {
        let parsed: Vec<http::HeaderValue> = origins
            .iter()
            .map(|o| {
                o.parse()
                    .with_context(|| format!("invalid CORS origin: {o:?}"))
            })
            .collect::<anyhow::Result<_>>()?;
        layer.allow_origin(parsed)
    };

    layer = if wildcard_methods {
        layer.allow_methods(cors::Any)
    } else {
        let parsed: Vec<http::Method> = methods
            .iter()
            .map(|m| {
                m.parse()
                    .with_context(|| format!("invalid CORS method: {m:?}"))
            })
            .collect::<anyhow::Result<_>>()?;
        layer.allow_methods(parsed)
    };

    layer = if wildcard_headers {
        layer.allow_headers(cors::Any)
    } else {
        let parsed: Vec<http::HeaderName> = headers
            .iter()
            .map(|h| {
                h.parse()
                    .with_context(|| format!("invalid CORS header: {h:?}"))
            })
            .collect::<anyhow::Result<_>>()?;
        layer.allow_headers(parsed)
    };

    if credentials {
        layer = layer.allow_credentials(true);
    }
    if let Some(s) = max_age {
        layer = layer.max_age(Duration::from_secs(s));
    }

    Ok(Some(layer))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &str) -> String {
        v.to_owned()
    }

    #[test]
    fn test_empty_origins_returns_none() {
        assert!(
            create_cors_layer(&[], &[], &[], false, None)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn test_credentials_with_wildcard_origin_fails() {
        let err = create_cors_layer(&[s("*")], &[], &[], true, None).unwrap_err();
        assert!(err.to_string().contains("origins"));
    }

    #[test]
    fn test_credentials_with_wildcard_method_fails() {
        let err =
            create_cors_layer(&[s("http://example.com")], &[s("*")], &[], true, None).unwrap_err();
        assert!(err.to_string().contains("methods"));
    }

    #[test]
    fn test_credentials_with_wildcard_header_fails() {
        let err =
            create_cors_layer(&[s("http://example.com")], &[], &[s("*")], true, None).unwrap_err();
        assert!(err.to_string().contains("headers"));
    }

    #[test]
    fn test_invalid_origin() {
        let err = create_cors_layer(&[s("\x00bad")], &[], &[], false, None).unwrap_err();
        assert!(err.to_string().contains("invalid CORS origin"));
    }

    #[test]
    fn test_invalid_method() {
        let err = create_cors_layer(
            &[s("http://example.com")],
            &[s("NOT A METHOD")],
            &[],
            false,
            None,
        )
        .unwrap_err();
        assert!(err.to_string().contains("invalid CORS method"));
    }

    #[test]
    fn test_invalid_header() {
        let err = create_cors_layer(
            &[s("http://example.com")],
            &[],
            &[s("bad\x00header")],
            false,
            None,
        )
        .unwrap_err();
        assert!(err.to_string().contains("invalid CORS header"));
    }
}
