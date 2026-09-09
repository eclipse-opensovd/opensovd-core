// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{GenericError, types::SupportedTags};

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct BulkDataCategoriesQuery {
    pub include_schema: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct BulkDataDescriptorsQuery {
    pub include_schema: bool,
    pub created_before: Option<DateTime<Utc>>,
    pub created_after: Option<DateTime<Utc>>,
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
pub struct BulkDataCategory(pub String);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
pub struct BulkDataDescriptor {
    pub id: String,
    pub mimetype: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub translation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creation_date: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_modified: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hash_algorithm: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<SupportedTags>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
pub struct AvailableBulkDataCategories {
    pub items: Vec<BulkDataCategory>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
pub struct BulkDataMetadata {
    pub items: Vec<BulkDataDescriptor>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
pub struct BulkDataDownload {
    pub signature: String,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
pub struct BulkDataUpload {
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
pub struct DeleteBulkDataError {
    pub id: String,
    pub error: GenericError,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
pub struct DeleteBulkDataResult {
    pub deleted_ids: Vec<String>,
    pub errors: Vec<DeleteBulkDataError>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ErrorCode, GenericError};

    #[test]
    fn bulk_data_descriptor_minimal_deserialize() {
        let json = r#"{"id":"file1","mimetype":"application/octet-stream"}"#;
        let desc: BulkDataDescriptor = serde_json::from_str(json).unwrap();
        assert_eq!(desc.id, "file1");
        assert_eq!(desc.mimetype, "application/octet-stream");
        assert!(desc.name.is_none());
        assert!(desc.translation_id.is_none());
        assert!(desc.size.is_none());
        assert!(desc.creation_date.is_none());
        assert!(desc.last_modified.is_none());
        assert!(desc.hash.is_none());
        assert!(desc.hash_algorithm.is_none());
        assert!(desc.tags.is_none());
    }

    #[test]
    fn bulk_data_descriptor_full_serialize() {
        let desc = BulkDataDescriptor {
            id: "file2".into(),
            mimetype: "image/png".into(),
            name: Some("picture.png".into()),
            translation_id: Some("t.id.1".into()),
            size: Some(1024),
            creation_date: Some("2026-01-01T00:00:00Z".parse::<DateTime<Utc>>().unwrap()),
            last_modified: Some("2026-06-01T00:00:00Z".parse::<DateTime<Utc>>().unwrap()),
            hash: Some("abc123".into()),
            hash_algorithm: Some("sha256".into()),
            tags: Some(SupportedTags(vec!["sensor".into(), "camera".into()])),
        };
        let json = serde_json::to_value(&desc).unwrap();
        assert_eq!(json["id"], "file2");
        assert_eq!(json["mimetype"], "image/png");
        assert_eq!(json["name"], "picture.png");
        assert_eq!(json["translation_id"], "t.id.1");
        assert_eq!(json["size"], 1024);
        assert_eq!(json["creation_date"], "2026-01-01T00:00:00Z");
        assert_eq!(json["last_modified"], "2026-06-01T00:00:00Z");
        assert_eq!(json["hash"], "abc123");
        assert_eq!(json["hash_algorithm"], "sha256");
        assert_eq!(json["tags"], serde_json::json!(["sensor", "camera"]));
    }

    #[test]
    fn optional_fields_omitted_not_null() {
        let desc = BulkDataDescriptor {
            id: "f".into(),
            mimetype: "application/octet-stream".into(),
            name: None,
            translation_id: None,
            size: None,
            creation_date: None,
            last_modified: None,
            hash: None,
            hash_algorithm: None,
            tags: None,
        };
        let json = serde_json::to_value(&desc).unwrap();
        let obj = json.as_object().unwrap();
        assert!(!obj.contains_key("name"));
        assert!(!obj.contains_key("translation_id"));
        assert!(!obj.contains_key("size"));
        assert!(!obj.contains_key("creation_date"));
        assert!(!obj.contains_key("last_modified"));
        assert!(!obj.contains_key("hash"));
        assert!(!obj.contains_key("hash_algorithm"));
        assert!(!obj.contains_key("tags"));
    }

    #[test]
    fn bulk_data_metadata_and_categories_empty_items() {
        let metadata = BulkDataMetadata { items: vec![] };
        let json = serde_json::to_value(&metadata).unwrap();
        assert_eq!(json["items"], serde_json::json!([]));

        let categories = AvailableBulkDataCategories { items: vec![] };
        let json = serde_json::to_value(&categories).unwrap();
        assert_eq!(json["items"], serde_json::json!([]));
    }

    #[test]
    fn delete_bulk_data_result_mixed_deleted_and_errors() {
        let result = DeleteBulkDataResult {
            deleted_ids: vec!["a".into(), "b".into()],
            errors: vec![DeleteBulkDataError {
                id: "c".into(),
                error: GenericError::new(ErrorCode::ErrorResponse, "could not delete c"),
            }],
        };
        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["deleted_ids"][0], "a");
        assert_eq!(json["deleted_ids"][1], "b");
        assert_eq!(json["errors"][0]["id"], "c");
        assert_eq!(json["errors"][0]["error"]["message"], "could not delete c");
    }

    #[test]
    fn bulk_data_descriptors_query_kebab_case_deserialization() {
        let json = r#"{
            "include-schema": true,
            "created-before": "2026-01-01T00:00:00Z",
            "created-after": "2025-01-01T00:00:00Z",
            "tags": ["sensor", "camera"]
        }"#;
        let query: BulkDataDescriptorsQuery = serde_json::from_str(json).unwrap();
        assert!(query.include_schema);
        assert_eq!(
            query.created_before,
            Some("2026-01-01T00:00:00Z".parse::<DateTime<Utc>>().unwrap())
        );
        assert_eq!(
            query.created_after,
            Some("2025-01-01T00:00:00Z".parse::<DateTime<Utc>>().unwrap())
        );
        assert_eq!(
            query.tags.as_deref(),
            Some(&["sensor".to_string(), "camera".to_string()][..])
        );
    }

    #[test]
    fn bulk_data_descriptors_query_all_defaults() {
        let query = BulkDataDescriptorsQuery::default();
        assert!(!query.include_schema);
        assert!(query.created_before.is_none());
        assert!(query.created_after.is_none());
        assert!(query.tags.is_none());
    }
}
