// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! JSON Schema generation trait.

/// Trait for types that can produce a JSON Schema
pub trait JsonSchema {
    /// Returns the JSON Schema for this type
    fn schema() -> serde_json::Value;
}

#[cfg(feature = "jsonschema")]
impl<T: schemars::JsonSchema> JsonSchema for T {
    fn schema() -> serde_json::Value {
        serde_json::to_value(schemars::schema_for!(T)).unwrap_or_else(|e| {
            tracing::warn!(error = %e, type_name = std::any::type_name::<T>(), "Failed to generate JSON schema");
            serde_json::Value::Null
        })
    }
}

#[cfg(not(feature = "jsonschema"))]
impl<T: serde::Serialize> JsonSchema for T {
    fn schema() -> serde_json::Value {
        serde_json::json!({ "type": "object" })
    }
}

#[cfg(all(test, not(feature = "jsonschema")))]
mod tests {
    use super::*;

    #[test]
    fn test_fallback_schema() {
        let schema = <String as JsonSchema>::schema();
        assert_eq!(schema, serde_json::json!({ "type": "object" }));
    }
}
