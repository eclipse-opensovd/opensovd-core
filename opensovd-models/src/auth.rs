// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Models of the `authorize` resource (ISO 17978-3 §7.23.4).

use std::fmt;

use serde::{Deserialize, Serialize};

/// Body of `POST /authorize`: the credentials of the client.
#[derive(Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
pub struct AuthorizeRequest {
    pub client_id: String,
    pub client_secret: String,
}

// The secret stays out of logs and panics.
impl fmt::Debug for AuthorizeRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AuthorizeRequest")
            .field("client_id", &self.client_id)
            .field("client_secret", &"<redacted>")
            .finish()
    }
}

/// Answer of `POST /authorize`: the token to send as `Authorization: Bearer <access_token>`.
#[derive(Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
pub struct AuthorizeResponse {
    pub access_token: String,
    /// The type of the token, `Bearer`.
    pub token_type: String,
    /// Lifetime of the token in seconds.
    pub expires_in: u64,
}

// The token stays out of logs and panics.
impl fmt::Debug for AuthorizeResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AuthorizeResponse")
            .field("access_token", &"<redacted>")
            .field("token_type", &self.token_type)
            .field("expires_in", &self.expires_in)
            .finish()
    }
}
