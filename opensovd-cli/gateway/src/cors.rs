// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use tower_http::cors::{self, CorsLayer};

/// Creates an optional CORS layer from raw string slices.
///
/// Returns `Ok(None)` when no origins are specified (CORS disabled).
///
/// # Errors
///
/// Returns an error if any origin, method, or header string is invalid,
/// or if credentials are enabled with wildcard origins, methods, or headers.
pub fn create_cors_layer(
    origins: &[String],
    methods: &[String],
    headers: &[String],
    credentials: bool,
    max_age: Option<u64>,
) -> Result<Option<CorsLayer>, String> {
    if origins.is_empty() {
        return Ok(None);
    }

    let wildcard_origins = origins.iter().any(|v| v == "*");
    let wildcard_methods = methods.iter().any(|v| v == "*");
    let wildcard_headers = headers.iter().any(|v| v == "*");

    if credentials {
        if wildcard_origins {
            return Err("cannot use wildcard '*' for origins when credentials are enabled".into());
        }
        if wildcard_methods {
            return Err("cannot use wildcard '*' for methods when credentials are enabled".into());
        }
        if wildcard_headers {
            return Err("cannot use wildcard '*' for headers when credentials are enabled".into());
        }
    }

    let mut layer = CorsLayer::new();

    layer = if wildcard_origins {
        layer.allow_origin(cors::Any)
    } else {
        let parsed: Vec<http::HeaderValue> = origins
            .iter()
            .map(|o| o.parse().map_err(|_| format!("invalid CORS origin: {o:?}")))
            .collect::<Result<_, _>>()?;
        layer.allow_origin(parsed)
    };

    layer = if wildcard_methods {
        layer.allow_methods(cors::Any)
    } else {
        let parsed: Vec<http::Method> = methods
            .iter()
            .map(|m| m.parse().map_err(|_| format!("invalid CORS method: {m:?}")))
            .collect::<Result<_, _>>()?;
        layer.allow_methods(parsed)
    };

    layer = if wildcard_headers {
        layer.allow_headers(cors::Any)
    } else {
        let parsed: Vec<http::HeaderName> = headers
            .iter()
            .map(|h| h.parse().map_err(|_| format!("invalid CORS header: {h:?}")))
            .collect::<Result<_, _>>()?;
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
        assert!(err.contains("origins"));
    }

    #[test]
    fn test_credentials_with_wildcard_method_fails() {
        let err =
            create_cors_layer(&[s("http://example.com")], &[s("*")], &[], true, None).unwrap_err();
        assert!(err.contains("methods"));
    }

    #[test]
    fn test_credentials_with_wildcard_header_fails() {
        let err =
            create_cors_layer(&[s("http://example.com")], &[], &[s("*")], true, None).unwrap_err();
        assert!(err.contains("headers"));
    }

    #[test]
    fn test_invalid_origin() {
        let err = create_cors_layer(&[s("\x00bad")], &[], &[], false, None).unwrap_err();
        assert!(err.contains("invalid CORS origin"));
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
        assert!(err.contains("invalid CORS method"));
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
        assert!(err.contains("invalid CORS header"));
    }
}
