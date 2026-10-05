// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Data provider building blocks.
//!
//! This module provides traits and implementations for constructing SOVD data providers.

use serde::{Deserialize, Serialize};

mod builder;
mod constant;
mod resource;

pub use builder::{BuildError, BuiltDataProvider, DataProviderBuilder};
pub use constant::{Constant, ConstantError};
pub use resource::{DataResource, ReadableDataResource, WriteableDataResource};

/// Wraps a scalar `T` in `{"value": <T>}` so the data value is a JSON object.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[schemars(description = "")]
pub struct Value<T> {
    /// The wrapped value.
    #[schemars(description = "")]
    pub value: T,
}

impl<T> Value<T> {
    /// Create a new `Value` wrapping the given inner value.
    pub fn new(value: T) -> Self {
        Self { value }
    }
}

#[cfg(test)]
mod tests {
    use super::Value;

    /// Blower fan level.
    #[derive(schemars::JsonSchema)]
    struct FanLevel {
        _level: u8,
    }

    #[derive(schemars::JsonSchema)]
    struct Menu {
        _entries: Vec<Value<Menu>>,
    }

    #[test]
    fn value_schema_leaves_description_to_inner_type() {
        let schema = schemars::schema_for!(Value<FanLevel>);
        assert_eq!(schema.get("description"), None);
        assert_eq!(
            schema.as_value().pointer("/$defs/FanLevel/description"),
            Some(&"Blower fan level.".into())
        );
    }

    #[test]
    fn value_schema_of_recursive_type() {
        let schema = schemars::schema_for!(Value<Menu>);
        assert!(schema.as_value().pointer("/$defs/Menu").is_some());
    }

    #[test]
    fn value_schema_of_scalar_has_no_description() {
        assert_eq!(
            serde_json::to_value(schemars::schema_for!(Value<String>)).unwrap(),
            serde_json::json!({
                "$schema": "https://json-schema.org/draft/2020-12/schema",
                "title": "Value",
                "type": "object",
                "properties": { "value": { "type": "string" } },
                "required": ["value"],
            })
        );
    }
}
