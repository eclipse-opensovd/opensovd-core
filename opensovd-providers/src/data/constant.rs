// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Helper implementations for data resources.

use async_trait::async_trait;
use opensovd_core::DataError;
use schemars::Schema;
use serde::Serialize;
use serde_json::Value;
use thiserror::Error;

use super::resource::ReadableDataResource;

/// Error returned when creating a `Constant` fails.
#[derive(Debug, Clone, Error)]
#[error("failed to serialize constant value: {0}")]
pub struct ConstantError(String);

/// Constant value with embedded schema (read-only).
///
/// # Example
///
/// ```ignore
/// use opensovd_providers::data::Constant;
///
/// let version = Constant::new("1.2.3")?;
/// ```
pub struct Constant {
    value: Value,
    stored_schema: Schema,
}

impl Constant {
    /// Create a constant that wraps a scalar in the SOVD `{"value": ...}` envelope.
    ///
    /// The value is wrapped in [`Value<T>`](super::Value), serialized to JSON,
    /// and the schema is captured from the wrapped type.
    ///
    /// # Errors
    ///
    /// Returns [`ConstantError`] if the value cannot be serialized to JSON.
    pub fn new<T: Serialize + schemars::JsonSchema>(value: T) -> Result<Self, ConstantError> {
        let wrapped = super::Value::new(value);
        Ok(Self {
            value: serde_json::to_value(&wrapped).map_err(|e| ConstantError(e.to_string()))?,
            stored_schema: schemars::schema_for!(super::Value<T>),
        })
    }
}

#[async_trait]
impl ReadableDataResource for Constant {
    type Value = Value;

    fn schema(&self) -> Schema {
        self.stored_schema.clone()
    }

    async fn read(&self) -> Result<Value, DataError> {
        Ok(self.value.clone())
    }
}
