// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UriReference(pub String);

impl From<String> for UriReference {
    fn from(s: String) -> Self {
        Self(s)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct JsonPointer(pub String);

impl From<String> for JsonPointer {
    fn from(s: String) -> Self {
        Self(s)
    }
}

#[cfg(feature = "jsonschema")]
mod schema {
    use schemars::{JsonSchema, Schema, SchemaGenerator};

    use super::{JsonPointer, UriReference};

    impl JsonSchema for UriReference {
        fn schema_name() -> std::borrow::Cow<'static, str> {
            "UriReference".into()
        }

        fn json_schema(_gen: &mut SchemaGenerator) -> Schema {
            schemars::json_schema!({
                "type": "string",
                "format": "uri-reference"
            })
        }
    }

    impl JsonSchema for JsonPointer {
        fn schema_name() -> std::borrow::Cow<'static, str> {
            "JsonPointer".into()
        }

        fn json_schema(_gen: &mut SchemaGenerator) -> Schema {
            schemars::json_schema!({
                "type": "string",
                "format": "json-pointer"
            })
        }
    }
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::*;

    #[test]
    fn uri_reference_from_string() {
        let uri: UriReference = String::from("https://example.com/api/v1").into();
        assert_eq!(uri.0, "https://example.com/api/v1");
    }

    #[test]
    fn json_pointer_from_string() {
        let s = String::from("/data/0");
        let ptr: JsonPointer = s.into();
        assert_eq!(ptr.0, "/data/0");
    }
}

#[cfg(all(test, feature = "jsonschema"))]
#[cfg_attr(coverage_nightly, coverage(off))]
mod schema_tests {
    use super::*;

    #[test]
    fn uri_reference_schema() {
        let schema = schemars::schema_for!(UriReference);
        let json = serde_json::to_value(&schema).unwrap();
        assert_eq!(json["format"], "uri-reference");
    }

    #[test]
    fn json_pointer_schema() {
        let schema = schemars::schema_for!(JsonPointer);
        let json = serde_json::to_value(&schema).unwrap();
        assert_eq!(json["format"], "json-pointer");
    }
}
