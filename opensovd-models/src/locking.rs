// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Lock types.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::DateTime;

/// A lock in the answers of the locks collection: Table 242 after acquiring it
/// holds its `id`, Table 245 in a list adds `owned`, and Table 248 for its
/// details has no `id`, but a `lock_expiration`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
pub struct Lock {
    /// Identifier of the lock
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// When the lock expires
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lock_expiration: Option<DateTime>,
    /// Whether the client of the request owns the lock
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owned: Option<bool>,
    /// Scopes of the lock, if it does not cover the whole entity
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scopes: Option<Vec<String>>,
    /// What a server adds to the lock
    #[serde(flatten)]
    pub extensions: Map<String, Value>,
}
