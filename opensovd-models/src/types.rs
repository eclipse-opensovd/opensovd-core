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

impl From<&serde_path_to_error::Error<serde_json::Error>> for JsonPointer {
    /// Points at the element where deserialization failed, or at the field
    /// itself if it is missing.
    fn from(error: &serde_path_to_error::Error<serde_json::Error>) -> Self {
        use serde_path_to_error::Segment;

        let mut pointer = String::new();
        let mut push = |token: &str| {
            pointer.push('/');
            pointer.push_str(&token.replace('~', "~0").replace('/', "~1"));
        };
        for segment in error.path() {
            match segment {
                Segment::Seq { index } => push(&index.to_string()),
                Segment::Map { key } => push(key),
                Segment::Enum { variant } => push(variant),
                Segment::Unknown => {}
            }
        }
        let message = error.inner().to_string();
        if let Some(field) = message
            .strip_prefix("missing field `")
            .and_then(|rest| rest.split('`').next())
        {
            push(field);
        }
        Self(pointer)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SupportedTags(pub Vec<String>);

impl From<Vec<String>> for SupportedTags {
    fn from(v: Vec<String>) -> Self {
        Self(v)
    }
}

#[cfg(feature = "jsonschema")]
mod schema {
    use schemars::{JsonSchema, Schema, SchemaGenerator};

    use super::{JsonPointer, SupportedTags, UriReference};

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

    impl JsonSchema for SupportedTags {
        fn schema_name() -> std::borrow::Cow<'static, str> {
            "SupportedTags".into()
        }

        fn json_schema(_gen: &mut SchemaGenerator) -> Schema {
            schemars::json_schema!({
                "type": "array",
                "items": {
                    "type": "string"
                }
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

    fn pointer_of<T: serde::de::DeserializeOwned + std::fmt::Debug>(json: &str) -> String {
        let deserializer = &mut serde_json::Deserializer::from_str(json);
        let error = serde_path_to_error::deserialize::<_, T>(deserializer).unwrap_err();
        JsonPointer::from(&error).0
    }

    #[test]
    fn json_pointer_from_path_error() {
        #[derive(Debug, Deserialize)]
        #[allow(dead_code)]
        struct Inner {
            level: u8,
        }
        #[derive(Debug, Deserialize)]
        #[allow(dead_code)]
        enum Mode {
            Manual { level: u8 },
        }
        #[derive(Debug, Deserialize)]
        #[allow(dead_code)]
        struct Outer {
            data: Vec<Inner>,
            mode: Option<Mode>,
            #[serde(rename = "a/b~c")]
            escaped: Option<u8>,
        }

        assert_eq!(
            pointer_of::<Outer>(r#"{"data": [{"level": "x"}]}"#),
            "/data/0/level"
        );
        assert_eq!(pointer_of::<Outer>(r#"{"data": [{}]}"#), "/data/0/level");
        assert_eq!(pointer_of::<Outer>(r"{}"), "/data");
        assert_eq!(
            pointer_of::<Outer>(r#"{"data": [], "a/b~c": "x"}"#),
            "/a~1b~0c"
        );
        assert_eq!(
            pointer_of::<Outer>(r#"{"data": [], "mode": {"Manual": {"level": "x"}}}"#),
            "/mode/Manual/level"
        );
        assert_eq!(pointer_of::<Outer>("[]"), "");
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
