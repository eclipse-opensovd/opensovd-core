// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

use std::fmt;

/// Authentication configuration for the SOVD client.
#[derive(Clone)]
#[non_exhaustive]
pub enum Auth {
    /// Send a bearer token in the `Authorization` header on every request.
    Bearer(String),
}

impl Auth {
    /// Create bearer token authentication.
    #[must_use]
    pub fn bearer(token: impl Into<String>) -> Self {
        Self::Bearer(token.into())
    }
}

impl fmt::Debug for Auth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bearer(_) => f.write_str("Bearer(****)"),
        }
    }
}

impl From<String> for Auth {
    fn from(value: String) -> Self {
        Self::Bearer(value)
    }
}

impl From<&str> for Auth {
    fn from(value: &str) -> Self {
        Self::Bearer(value.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::Auth;

    #[test]
    fn debug_redacts_bearer_token() {
        let auth = Auth::bearer("secret-token");

        let debug = format!("{auth:?}");

        assert!(!debug.contains("secret-token"));
        assert_eq!(debug, "Bearer(****)");
    }
}
