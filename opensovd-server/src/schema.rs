// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! JSON schemas of the responses, generated with [`schemars`].
//!
//! The schemas follow JSON Schema 2020-12, the dialect of OpenAPI 3.1. They
//! inline the schemas of other types and leave out `$schema`, `title` and
//! `description`.

#[cfg(feature = "jsonschema")]
use schemars::{
    generate::{Contract, SchemaSettings},
    transform::{RecursiveTransform, Transform},
};
use serde_json::Value;

/// Trait for types that can produce a JSON Schema
pub trait JsonSchema {
    /// Returns the JSON Schema for this type, e.g. the `schema` of a response
    /// for `include-schema=true`.
    fn schema() -> Value;
}

#[cfg(feature = "jsonschema")]
impl<T: schemars::JsonSchema> JsonSchema for T {
    fn schema() -> Value {
        SchemaSettings::draft2020_12()
            .with(|settings| {
                settings.meta_schema = None;
                settings.contract = Contract::Serialize;
                settings.inline_subschemas = true;
            })
            .with_transform(RecursiveTransform(strip))
            .into_generator()
            .into_root_schema_for::<T>()
            .to_value()
    }
}

#[cfg(not(feature = "jsonschema"))]
impl<T: serde::Serialize> JsonSchema for T {
    fn schema() -> Value {
        serde_json::json!({ "type": "object" })
    }
}

/// Leaves out of a provider's schema of a value what the schemas of the
/// responses leave out.
pub(crate) fn normalize(value: Value) -> Value {
    #[cfg(feature = "jsonschema")]
    if let Value::Object(object) = value {
        let mut schema = schemars::Schema::from(object);
        RecursiveTransform(strip).transform(&mut schema);
        return schema.to_value();
    }
    value
}

/// Leaves out `$schema`, `title` and `description`, keeping the order of the
/// other keywords.
#[cfg(feature = "jsonschema")]
fn strip(schema: &mut schemars::Schema) {
    if let Some(object) = schema.as_object_mut() {
        object
            .retain(|keyword, _| !matches!(keyword.as_str(), "$schema" | "title" | "description"));
    }
}

#[cfg(all(test, feature = "jsonschema"))]
mod tests {
    use opensovd_models::{DateTime, GenericError, Tag};
    use serde::Serialize;
    use serde_json::json;

    use super::*;

    /// A temperature sample.
    #[derive(Serialize, schemars::JsonSchema)]
    struct Sample {
        /// When the sample was taken.
        taken: DateTime,
        /// Only present if the sensor failed.
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<GenericError>,
        #[serde(skip_serializing_if = "Option::is_none")]
        note: Option<String>,
    }

    #[test]
    fn schema_leaves_out_annotations() {
        let schema = Sample::schema();
        assert_eq!(schema.get("$schema"), None);
        assert_eq!(schema.get("title"), None);
        assert_eq!(schema.get("description"), None);
        assert_eq!(
            schema["properties"]["taken"],
            json!({ "type": "string", "format": "date-time" })
        );
        let error = &schema["properties"]["error"];
        assert_eq!(error.get("description"), None);
        assert_eq!(error["type"], json!(["object", "null"]));
        assert_eq!(
            schema["properties"]["note"],
            json!({ "type": ["string", "null"] })
        );
        assert_eq!(schema["required"], json!(["taken"]));
    }

    #[test]
    fn schema_keeps_attributes_named_like_annotations() {
        let schema = Tag::schema();
        assert_eq!(
            schema["properties"]["description"],
            json!({ "type": ["string", "null"] })
        );
    }

    #[test]
    fn normalize_leaves_out_annotations_of_subschemas() {
        let value = json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "title": "Fan",
            "description": "Blower fan.",
            "type": "object",
            "properties": {
                "level": { "description": "Fan level.", "type": "integer" },
                "title": { "type": "string" },
            },
            "$defs": { "Mode": { "title": "Mode", "enum": ["Auto"] } },
        });
        assert_eq!(
            normalize(value),
            json!({
                "type": "object",
                "properties": {
                    "level": { "type": "integer" },
                    "title": { "type": "string" },
                },
                "$defs": { "Mode": { "enum": ["Auto"] } },
            })
        );
        assert_eq!(normalize(json!(true)), json!(true));
    }

    #[cfg(feature = "json-preserve-order")]
    #[test]
    fn normalize_keeps_the_order_of_keywords() {
        let value = json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "title": "Fan",
            "properties": {},
            "required": [],
        });
        let normalized = normalize(value);
        let keywords: Vec<&String> = normalized.as_object().unwrap().keys().collect();
        assert_eq!(keywords, ["type", "properties", "required"]);
    }
}

#[cfg(all(test, not(feature = "jsonschema")))]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn test_fallback_schema() {
        let schema = <String as JsonSchema>::schema();
        assert_eq!(schema, json!({ "type": "object" }));
    }

    #[test]
    fn normalize_keeps_value_schemas() {
        let value = json!({ "title": "Fan", "type": "integer" });
        assert_eq!(normalize(value.clone()), value);
    }
}
