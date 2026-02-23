// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

use crate::error::GenericError;
use crate::{Items, JsonPointer};

/// Data category type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum DataCategory {
    /// Identifications - read access to fixed parameters (part number, VIN)
    IdentData,
    /// Measurements - read access to dynamically changing values (battery voltage)
    CurrentData,
    /// Parameters - read and write access to parameters
    StoredData,
    /// System information - read access to dynamic system resources (CPU load)
    SysInfo,
    /// Custom category with x-<ext>- prefix
    #[serde(untagged)]
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
pub struct Metadata {
    pub id: String,
    pub name: String,
    pub category: DataCategory,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub translation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub groups: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
}

/// Data list response.
pub type DataList = Items<Metadata>;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
pub struct Data {
    pub data: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errors: Option<Vec<DataErrorEntry>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
pub struct ReadResponse {
    pub id: String,
    pub data: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errors: Option<Vec<DataErrorEntry>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
pub struct WriteRequest {
    pub data: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
pub struct DataErrorEntry {
    pub path: JsonPointer,
    pub error: GenericError,
}

#[derive(Debug, Clone, Default)]
pub struct DataFilter {
    pub groups: Option<Vec<String>>,
    pub categories: Option<Vec<DataCategory>>,
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
pub struct DataError {
    pub error_code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl DataError {
    #[must_use]
    pub fn not_found(id: &str) -> Self {
        Self {
            error_code: "not-found".into(),
            message: Some(format!("Data resource not found: {id}")),
        }
    }

    #[must_use]
    pub fn read_only() -> Self {
        Self {
            error_code: "read-only".into(),
            message: Some("Data resource is read-only".into()),
        }
    }
}

impl std::fmt::Display for DataError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.error_code)
    }
}

impl std::error::Error for DataError {}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct DataQuery {
    #[serde(default)]
    pub groups: Option<Vec<String>>,
    #[serde(default)]
    pub categories: Option<Vec<String>>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    #[serde(default, rename = "include-schema")]
    pub include_schema: bool,
}

impl From<DataQuery> for DataFilter {
    fn from(query: DataQuery) -> Self {
        Self {
            groups: query.groups,
            categories: query
                .categories
                .map(|cats| cats.into_iter().map(DataCategory::from).collect()),
            tags: query.tags,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ReadDataQuery {
    #[serde(default, rename = "include-schema")]
    pub include_schema: bool,
}

/// Data categories response.
pub type DataCategories = Items<DataCategoryInformation>;

/// Data category information.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
pub struct DataCategoryInformation {
    pub item: DataCategory,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category_translation_id: Option<String>,
}

/// Data groups response.
pub type DataGroups = Items<Group>;

/// Value group information.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
pub struct Group {
    pub id: String,
    pub category: DataCategory,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category_translation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_translation_id: Option<String>,
}

/// Data groups query parameters.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct DataGroupsQuery {
    pub category: Option<DataCategory>,
}

impl From<String> for DataCategory {
    fn from(s: String) -> Self {
        match s.as_str() {
            "identData" => Self::IdentData,
            "currentData" => Self::CurrentData,
            "storedData" => Self::StoredData,
            "sysInfo" => Self::SysInfo,
            _ => Self::Custom(s),
        }
    }
}

impl DataCategory {
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::IdentData => "identData",
            Self::CurrentData => "currentData",
            Self::StoredData => "storedData",
            Self::SysInfo => "sysInfo",
            Self::Custom(s) => s,
        }
    }
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::*;

    #[test]
    fn test_data_category_conversion() {
        assert_eq!(
            DataCategory::from("identData".to_string()),
            DataCategory::IdentData
        );
        assert_eq!(
            DataCategory::from("currentData".to_string()),
            DataCategory::CurrentData
        );
        assert_eq!(
            DataCategory::from("storedData".to_string()),
            DataCategory::StoredData
        );
        assert_eq!(
            DataCategory::from("sysInfo".to_string()),
            DataCategory::SysInfo
        );
        assert_eq!(
            DataCategory::from("x-custom".to_string()),
            DataCategory::Custom("x-custom".into())
        );

        assert_eq!(DataCategory::IdentData.as_str(), "identData");
        assert_eq!(DataCategory::CurrentData.as_str(), "currentData");
        assert_eq!(DataCategory::StoredData.as_str(), "storedData");
        assert_eq!(DataCategory::SysInfo.as_str(), "sysInfo");
        assert_eq!(DataCategory::Custom("x-custom".into()).as_str(), "x-custom");
    }

    #[test]
    fn test_data_error() {
        let not_found = DataError::not_found("voltage");
        assert_eq!(not_found.error_code, "not-found");
        assert_eq!(
            not_found.message,
            Some("Data resource not found: voltage".into())
        );
        assert_eq!(not_found.to_string(), "not-found");

        let read_only = DataError::read_only();
        assert_eq!(read_only.error_code, "read-only");
        assert_eq!(read_only.message, Some("Data resource is read-only".into()));
        assert_eq!(read_only.to_string(), "read-only");
    }

    #[test]
    fn test_data_query_to_filter() {
        let query = DataQuery {
            groups: Some(vec!["sensors".into()]),
            categories: Some(vec!["currentData".into()]),
            tags: Some(vec!["temperature".into()]),
            include_schema: true,
        };

        let filter: DataFilter = query.into();

        assert_eq!(filter.groups, Some(vec!["sensors".into()]));
        assert_eq!(filter.categories, Some(vec![DataCategory::CurrentData]));
        assert_eq!(filter.tags, Some(vec!["temperature".into()]));
    }
}
