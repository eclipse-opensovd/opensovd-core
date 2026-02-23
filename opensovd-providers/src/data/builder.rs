// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Builder for constructing `DataProvider` implementations.

use std::collections::HashSet;

use async_trait::async_trait;
use indexmap::IndexMap;
use opensovd_core::{
    CategoryInfo, Data, DataError, DataFilter, DataProvider, GroupInfo, Metadata, TagInfo,
};
use opensovd_models::data::DataCategory;
use serde_json::Value;
use thiserror::Error;

use super::resource::{DataResource, ReadableDataResource, WriteableDataResource};

/// Error returned when building a `DataProvider` fails.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BuildError {
    /// A duplicate data ID was registered.
    #[error("duplicate data ID: {0}")]
    DuplicateResourceId(String),
}

/// Builder for constructing a `DataProvider` from individual data resources.
///
/// # Example
///
/// ```ignore
/// use opensovd_providers::data::{DataProviderBuilder, Constant};
/// use opensovd_models::data::DataCategory;
///
/// let provider = DataProviderBuilder::new()
///     .read_data("sw.version", "Software Version", &DataCategory::IdentData, Constant::new("1.0.0")?)
///     .build()?;
/// ```
#[derive(Default)]
pub struct DataProviderBuilder {
    resources: Vec<ResourceEntry>,
}

struct ResourceEntry {
    metadata: Metadata,
    resource: Box<dyn DataResourceDyn>,
}

/// Unified object-safe trait for dynamic dispatch of data resources.
#[async_trait]
trait DataResourceDyn: Send + Sync + 'static {
    async fn read(&self) -> Result<Value, DataError>;
    async fn write(&self, value: Value) -> Result<(), DataError>;
}

struct ReadOnlyAdapter<T>(T);

#[async_trait]
impl<T: ReadableDataResource> DataResourceDyn for ReadOnlyAdapter<T> {
    async fn read(&self) -> Result<Value, DataError> {
        let value = ReadableDataResource::read(&self.0).await?;
        serde_json::to_value(value).map_err(|e| DataError::Internal(e.to_string()))
    }

    async fn write(&self, _value: Value) -> Result<(), DataError> {
        Err(DataError::ReadOnly)
    }
}

struct WriteOnlyAdapter<T>(T);

#[async_trait]
impl<T: WriteableDataResource> DataResourceDyn for WriteOnlyAdapter<T> {
    async fn read(&self) -> Result<Value, DataError> {
        Err(DataError::Internal("resource is write-only".to_string()))
    }

    async fn write(&self, value: Value) -> Result<(), DataError> {
        let typed: T::Value =
            serde_json::from_value(value).map_err(|e| DataError::Internal(e.to_string()))?;
        WriteableDataResource::write(&self.0, &typed).await
    }
}

struct ReadWriteAdapter<T>(T);

#[async_trait]
impl<T: DataResource> DataResourceDyn for ReadWriteAdapter<T> {
    async fn read(&self) -> Result<Value, DataError> {
        let value = ReadableDataResource::read(&self.0).await?;
        serde_json::to_value(value).map_err(|e| DataError::Internal(e.to_string()))
    }

    async fn write(&self, value: Value) -> Result<(), DataError> {
        let typed: <T as WriteableDataResource>::Value =
            serde_json::from_value(value).map_err(|e| DataError::Internal(e.to_string()))?;
        WriteableDataResource::write(&self.0, &typed).await
    }
}

impl DataProviderBuilder {
    /// Create a new empty builder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a read-only data resource.
    ///
    /// The schema is derived from the resource's `Value` associated type.
    #[must_use]
    pub fn read_data<R: ReadableDataResource>(
        mut self,
        id: impl Into<String>,
        name: impl Into<String>,
        category: &DataCategory,
        resource: R,
    ) -> Self {
        let id = id.into();
        let schema = serde_json::to_value(resource.schema()).unwrap_or(Value::Null);
        self.resources.push(ResourceEntry {
            metadata: Metadata {
                id: id.clone(),
                name: name.into(),
                category: category.as_str().to_string(),
                schema: Some(schema),
                groups: Vec::new(),
                tags: Vec::new(),
                translation_id: None,
                is_readable: true,
                is_writable: false,
            },
            resource: Box::new(ReadOnlyAdapter(resource)),
        });
        self
    }

    /// Register a write-only data resource.
    ///
    /// The schema is derived from the resource's `Value` associated type.
    #[must_use]
    pub fn write_data<W: WriteableDataResource>(
        mut self,
        id: impl Into<String>,
        name: impl Into<String>,
        category: &DataCategory,
        resource: W,
    ) -> Self {
        let id = id.into();
        let schema = serde_json::to_value(resource.schema()).unwrap_or(Value::Null);
        self.resources.push(ResourceEntry {
            metadata: Metadata {
                id: id.clone(),
                name: name.into(),
                category: category.as_str().to_string(),
                schema: Some(schema),
                groups: Vec::new(),
                tags: Vec::new(),
                translation_id: None,
                is_readable: false,
                is_writable: true,
            },
            resource: Box::new(WriteOnlyAdapter(resource)),
        });
        self
    }

    /// Register a read-write data resource.
    ///
    /// The schema is derived from the resource's `Value` associated type
    /// (from `ReadableDataResource`).
    #[must_use]
    pub fn data<RW: DataResource>(
        mut self,
        id: impl Into<String>,
        name: impl Into<String>,
        category: &DataCategory,
        resource: RW,
    ) -> Self {
        let id = id.into();
        let schema =
            serde_json::to_value(ReadableDataResource::schema(&resource)).unwrap_or(Value::Null);
        self.resources.push(ResourceEntry {
            metadata: Metadata {
                id: id.clone(),
                name: name.into(),
                category: category.as_str().to_string(),
                schema: Some(schema),
                groups: Vec::new(),
                tags: Vec::new(),
                translation_id: None,
                is_readable: true,
                is_writable: true,
            },
            resource: Box::new(ReadWriteAdapter(resource)),
        });
        self
    }

    /// Configure groups for the last added resource.
    #[must_use]
    pub fn groups(mut self, groups: impl IntoIterator<Item = impl Into<String>>) -> Self {
        if let Some(last) = self.resources.last_mut() {
            last.metadata.groups = groups.into_iter().map(Into::into).collect();
        }
        self
    }

    /// Configure tags for the last added resource.
    #[must_use]
    pub fn tags(mut self, tags: impl IntoIterator<Item = impl Into<String>>) -> Self {
        if let Some(last) = self.resources.last_mut() {
            last.metadata.tags = tags.into_iter().map(Into::into).collect();
        }
        self
    }

    /// Configure translation ID for the last added resource.
    #[must_use]
    pub fn translation_id(mut self, id: impl Into<String>) -> Self {
        if let Some(last) = self.resources.last_mut() {
            last.metadata.translation_id = Some(id.into());
        }
        self
    }

    /// Override the schema for the last added resource.
    ///
    /// Replaces the auto-derived schema entirely. Use this for complex
    /// schemas that cannot be expressed with the builder methods.
    #[must_use]
    pub fn schema(mut self, schema: impl Into<Value>) -> Self {
        if let Some(last) = self.resources.last_mut() {
            last.metadata.schema = Some(schema.into());
        }
        self
    }

    /// Build the `DataProvider`.
    ///
    /// # Errors
    ///
    /// Returns [`BuildError::DuplicateResourceId`] if any data ID is registered more than once.
    pub fn build(self) -> Result<BuiltDataProvider, BuildError> {
        let mut resources = IndexMap::with_capacity(self.resources.len());
        for entry in self.resources {
            let id = entry.metadata.id.clone();
            if resources.insert(id.clone(), entry).is_some() {
                return Err(BuildError::DuplicateResourceId(id));
            }
        }
        Ok(BuiltDataProvider { resources })
    }
}

/// A `DataProvider` built from registered resources.
pub struct BuiltDataProvider {
    resources: IndexMap<String, ResourceEntry>,
}

impl BuiltDataProvider {
    fn find_resource(&self, data_id: &str) -> Option<&ResourceEntry> {
        self.resources.get(data_id)
    }

    fn matches_filter(metadata: &Metadata, filter: &DataFilter) -> bool {
        // Check categories filter
        if !filter.categories.is_empty()
            && !filter.categories.iter().any(|c| c == &metadata.category)
        {
            return false;
        }

        // Check groups filter
        if !filter.groups.is_empty()
            && !filter
                .groups
                .iter()
                .any(|g| metadata.groups.iter().any(|mg| mg == g))
        {
            return false;
        }

        // Check tags filter
        if !filter.tags.is_empty()
            && !filter
                .tags
                .iter()
                .any(|t| metadata.tags.iter().any(|mt| mt == t))
        {
            return false;
        }

        true
    }
}

#[async_trait]
impl DataProvider for BuiltDataProvider {
    async fn list(&self, filter: DataFilter) -> Result<Vec<Metadata>, DataError> {
        Ok(self
            .resources
            .values()
            .filter(|r| Self::matches_filter(&r.metadata, &filter))
            .map(|r| r.metadata.clone())
            .collect())
    }

    async fn read(&self, data_id: &str, include_schema: bool) -> Result<Data, DataError> {
        let resource = self
            .find_resource(data_id)
            .ok_or_else(|| DataError::NotFound(data_id.to_string()))?;

        let data = resource.resource.read().await?;

        Ok(Data {
            data,
            schema: if include_schema {
                resource.metadata.schema.clone()
            } else {
                None
            },
        })
    }

    async fn write(&self, data_id: &str, value: Value) -> Result<(), DataError> {
        let resource = self
            .find_resource(data_id)
            .ok_or_else(|| DataError::NotFound(data_id.to_string()))?;

        resource.resource.write(value).await
    }

    async fn categories(&self) -> Result<Vec<CategoryInfo>, DataError> {
        let mut seen = HashSet::new();
        Ok(self
            .resources
            .values()
            .filter(|r| seen.insert(r.metadata.category.clone()))
            .map(|r| CategoryInfo {
                category: r.metadata.category.clone(),
                translation_id: r.metadata.translation_id.clone(),
            })
            .collect())
    }

    async fn groups(&self, category_filter: Option<&str>) -> Result<Vec<GroupInfo>, DataError> {
        let mut seen = HashSet::new();
        Ok(self
            .resources
            .values()
            .filter(|r| category_filter.is_none_or(|c| r.metadata.category == c))
            .flat_map(|r| {
                let category = r.metadata.category.clone();
                r.metadata
                    .groups
                    .iter()
                    .map(move |g| (g.clone(), category.clone()))
            })
            .filter(|(g, _)| seen.insert(g.clone()))
            .map(|(id, category)| GroupInfo {
                id,
                category,
                category_translation_id: None,
                group: None,
                group_translation_id: None,
            })
            .collect())
    }

    async fn tags(&self) -> Result<Vec<TagInfo>, DataError> {
        let mut seen = HashSet::new();
        Ok(self
            .resources
            .values()
            .flat_map(|r| r.metadata.tags.iter())
            .filter(|t| seen.insert((*t).clone()))
            .map(|id| TagInfo {
                id: id.clone(),
                description: None,
                translation_id: None,
            })
            .collect())
    }
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::data::Constant;

    #[tokio::test]
    async fn test_build_with_constant() {
        let provider = DataProviderBuilder::new()
            .read_data(
                "sw.version",
                "Software Version",
                &DataCategory::IdentData,
                Constant::new("1.0.0").unwrap(),
            )
            .build()
            .unwrap();

        let result = provider.read("sw.version", false).await.unwrap();
        assert_eq!(result.data, json!({"value": "1.0.0"}));
        assert!(result.schema.is_none());
    }

    #[tokio::test]
    async fn test_build_with_schema() {
        let provider = DataProviderBuilder::new()
            .read_data(
                "sw.version",
                "Software Version",
                &DataCategory::IdentData,
                Constant::new("1.0.0").unwrap(),
            )
            .build()
            .unwrap();

        let result = provider.read("sw.version", true).await.unwrap();
        assert_eq!(result.data, json!({"value": "1.0.0"}));
        assert!(result.schema.is_some());
    }

    #[tokio::test]
    async fn test_list_resources() {
        let provider = DataProviderBuilder::new()
            .read_data(
                "sw.version",
                "Software Version",
                &DataCategory::IdentData,
                Constant::new("1.0.0").unwrap(),
            )
            .read_data(
                "system.uptime",
                "System Uptime",
                &DataCategory::SysInfo,
                Constant::new(12345u64).unwrap(),
            )
            .tags(["monitoring"])
            .build()
            .unwrap();

        let items = provider.list(DataFilter::default()).await.unwrap();
        assert_eq!(items.len(), 2);
    }

    #[tokio::test]
    async fn test_filter_by_category() {
        let provider = DataProviderBuilder::new()
            .read_data(
                "sw.version",
                "Software Version",
                &DataCategory::IdentData,
                Constant::new("1.0.0").unwrap(),
            )
            .read_data(
                "system.uptime",
                "System Uptime",
                &DataCategory::SysInfo,
                Constant::new(12345u64).unwrap(),
            )
            .build()
            .unwrap();

        let filter = DataFilter {
            categories: vec!["identData".to_string()],
            ..Default::default()
        };
        let items = provider.list(filter).await.unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items.first().map(|m| m.id.as_str()), Some("sw.version"));
    }

    #[tokio::test]
    async fn test_filter_by_tags() {
        let provider = DataProviderBuilder::new()
            .read_data(
                "sw.version",
                "Software Version",
                &DataCategory::IdentData,
                Constant::new("1.0.0").unwrap(),
            )
            .read_data(
                "system.uptime",
                "System Uptime",
                &DataCategory::SysInfo,
                Constant::new(12345u64).unwrap(),
            )
            .tags(["monitoring"])
            .build()
            .unwrap();

        let filter = DataFilter {
            tags: vec!["monitoring".to_string()],
            ..Default::default()
        };
        let items = provider.list(filter).await.unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items.first().map(|m| m.id.as_str()), Some("system.uptime"));
    }

    #[tokio::test]
    async fn test_filter_by_groups() {
        let provider = DataProviderBuilder::new()
            .read_data(
                "sw.version",
                "Software Version",
                &DataCategory::IdentData,
                Constant::new("1.0.0").unwrap(),
            )
            .groups(["info"])
            .read_data(
                "system.uptime",
                "System Uptime",
                &DataCategory::SysInfo,
                Constant::new(12345u64).unwrap(),
            )
            .groups(["monitoring"])
            .build()
            .unwrap();

        let filter = DataFilter {
            groups: vec!["info".to_string()],
            ..Default::default()
        };
        let items = provider.list(filter).await.unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items.first().map(|m| m.id.as_str()), Some("sw.version"));
    }

    #[tokio::test]
    async fn test_not_found() {
        let provider = DataProviderBuilder::new().build().unwrap();

        let result = provider.read("nonexistent", false).await;
        assert!(matches!(result, Err(DataError::NotFound(_))));
    }

    #[tokio::test]
    async fn test_read_only_write_fails() {
        let provider = DataProviderBuilder::new()
            .read_data(
                "sw.version",
                "Software Version",
                &DataCategory::IdentData,
                Constant::new("1.0.0").unwrap(),
            )
            .build()
            .unwrap();

        let result = provider
            .write("sw.version", json!({"value": "2.0.0"}))
            .await;
        assert!(matches!(result, Err(DataError::ReadOnly)));
    }

    #[tokio::test]
    async fn test_categories_override() {
        let provider = DataProviderBuilder::new()
            .read_data(
                "sw.version",
                "Software Version",
                &DataCategory::IdentData,
                Constant::new("1.0.0").unwrap(),
            )
            .read_data(
                "hw.version",
                "Hardware Version",
                &DataCategory::IdentData,
                Constant::new("2.0.0").unwrap(),
            )
            .read_data(
                "system.uptime",
                "System Uptime",
                &DataCategory::SysInfo,
                Constant::new(12345u64).unwrap(),
            )
            .build()
            .unwrap();

        let categories = provider.categories().await.unwrap();
        assert_eq!(categories.len(), 2);
        assert_eq!(categories[0].category, "identData");
        assert_eq!(categories[1].category, "sysInfo");
    }

    #[tokio::test]
    async fn test_groups_override() {
        let provider = DataProviderBuilder::new()
            .read_data(
                "sw.version",
                "Software Version",
                &DataCategory::IdentData,
                Constant::new("1.0.0").unwrap(),
            )
            .groups(["info", "core"])
            .read_data(
                "system.uptime",
                "System Uptime",
                &DataCategory::SysInfo,
                Constant::new(12345u64).unwrap(),
            )
            .groups(["monitoring"])
            .build()
            .unwrap();

        // All groups
        let groups = provider.groups(None).await.unwrap();
        assert_eq!(groups.len(), 3);

        // Filter by category
        let groups = provider.groups(Some("identData")).await.unwrap();
        assert_eq!(groups.len(), 2);
        assert!(groups.iter().all(|g| g.category == "identData"));
    }

    #[tokio::test]
    async fn test_tags_override() {
        let provider = DataProviderBuilder::new()
            .read_data(
                "sw.version",
                "Software Version",
                &DataCategory::IdentData,
                Constant::new("1.0.0").unwrap(),
            )
            .tags(["info", "version"])
            .read_data(
                "system.uptime",
                "System Uptime",
                &DataCategory::SysInfo,
                Constant::new(12345u64).unwrap(),
            )
            .tags(["info", "monitoring"])
            .build()
            .unwrap();

        let tags = provider.tags().await.unwrap();
        assert_eq!(tags.len(), 3);
        let tag_ids: Vec<&str> = tags.iter().map(|t| t.id.as_str()).collect();
        assert!(tag_ids.contains(&"info"));
        assert!(tag_ids.contains(&"version"));
        assert!(tag_ids.contains(&"monitoring"));
    }

    #[tokio::test]
    async fn test_duplicate_resource_id() {
        let result = DataProviderBuilder::new()
            .read_data(
                "duplicate.id",
                "Duplicate",
                &DataCategory::IdentData,
                Constant::new("first").unwrap(),
            )
            .read_data(
                "duplicate.id",
                "Duplicate",
                &DataCategory::IdentData,
                Constant::new("second").unwrap(),
            )
            .build();

        assert!(matches!(
            result,
            Err(BuildError::DuplicateResourceId(id)) if id == "duplicate.id"
        ));
    }
}
