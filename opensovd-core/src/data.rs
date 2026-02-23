// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Data provider trait and types.

use async_trait::async_trait;

/// Filter criteria for listing data values.
#[derive(Debug, Clone, Default)]
pub struct DataFilter {
    pub groups: Vec<String>,
    pub categories: Vec<String>,
    pub tags: Vec<String>,
}

/// Metadata about a data value.
#[derive(Debug, Clone)]
pub struct Metadata {
    pub id: String,
    pub name: String,
    pub category: String,
    pub translation_id: Option<String>,
    pub groups: Vec<String>,
    pub tags: Vec<String>,
    pub schema: Option<serde_json::Value>,
    /// Whether this resource supports read operations.
    pub is_readable: bool,
    /// Whether this resource supports write operations.
    pub is_writable: bool,
}

/// A data value with optional schema.
#[derive(Debug, Clone)]
pub struct Data {
    pub data: serde_json::Value,
    pub schema: Option<serde_json::Value>,
}

/// Errors that can occur when accessing data.
#[derive(Debug, Clone, thiserror::Error)]
pub enum DataError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("read only")]
    ReadOnly,
    #[error("internal error: {0}")]
    Internal(String),
}

/// A `Result` alias where the `Err` variant is [`DataError`].
pub type Result<T> = std::result::Result<T, DataError>;

/// Category information returned by `categories()`.
#[derive(Debug, Clone)]
pub struct CategoryInfo {
    pub category: String,
    pub translation_id: Option<String>,
}

/// Group information returned by `groups()`.
#[derive(Debug, Clone)]
pub struct GroupInfo {
    pub id: String,
    pub category: String,
    pub category_translation_id: Option<String>,
    pub group: Option<String>,
    pub group_translation_id: Option<String>,
}

/// Tag information returned by `tags()`.
#[derive(Debug, Clone)]
pub struct TagInfo {
    pub id: String,
    pub description: Option<String>,
    pub translation_id: Option<String>,
}

#[async_trait]
pub trait DataProvider: Send + Sync + 'static {
    async fn list(&self, filter: DataFilter) -> Result<Vec<Metadata>>;

    async fn read(&self, data_id: &str, include_schema: bool) -> Result<Data>;

    async fn write(&self, data_id: &str, value: serde_json::Value) -> Result<()>;

    /// Returns unique categories from all data values.
    /// Default implementation extracts from `list()`.
    async fn categories(&self) -> Result<Vec<CategoryInfo>> {
        let all = self.list(DataFilter::default()).await?;
        let mut seen = std::collections::HashSet::new();
        Ok(all
            .into_iter()
            .filter(|m| seen.insert(m.category.clone()))
            .map(|m| CategoryInfo {
                category: m.category,
                translation_id: m.translation_id,
            })
            .collect())
    }

    /// Returns groups with their categories.
    /// Default implementation extracts from `list()`.
    async fn groups(&self, category_filter: Option<&str>) -> Result<Vec<GroupInfo>> {
        let all = self.list(DataFilter::default()).await?;
        let mut seen = std::collections::HashSet::new();
        Ok(all
            .into_iter()
            .filter(|m| category_filter.is_none_or(|c| m.category == c))
            .flat_map(|m| {
                let category = m.category;
                m.groups.into_iter().map(move |g| (g, category.clone()))
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

    /// Returns unique tags from all data values.
    /// Default implementation extracts from `list()`.
    async fn tags(&self) -> Result<Vec<TagInfo>> {
        let all = self.list(DataFilter::default()).await?;
        let mut seen = std::collections::HashSet::new();
        Ok(all
            .into_iter()
            .flat_map(|m| m.tags)
            .filter(|t| seen.insert(t.clone()))
            .map(|id| TagInfo {
                id,
                description: None,
                translation_id: None,
            })
            .collect())
    }
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::*;

    struct MockProvider {
        items: Vec<Metadata>,
    }

    #[async_trait]
    impl DataProvider for MockProvider {
        async fn list(&self, _filter: DataFilter) -> Result<Vec<Metadata>> {
            Ok(self.items.clone())
        }

        async fn read(&self, _data_id: &str, _include_schema: bool) -> Result<Data> {
            unimplemented!()
        }

        async fn write(&self, _data_id: &str, _value: serde_json::Value) -> Result<()> {
            unimplemented!()
        }
    }

    fn make_metadata(id: &str, category: &str, groups: Vec<&str>) -> Metadata {
        Metadata {
            id: id.into(),
            name: id.into(),
            category: category.into(),
            translation_id: None,
            groups: groups.into_iter().map(String::from).collect(),
            tags: vec![],
            schema: None,
            is_readable: true,
            is_writable: false,
        }
    }

    #[tokio::test]
    async fn test_categories_returns_unique() {
        let provider = MockProvider {
            items: vec![
                make_metadata("a", "currentData", vec![]),
                make_metadata("b", "currentData", vec![]),
                make_metadata("c", "identData", vec![]),
            ],
        };

        let cats = provider.categories().await.unwrap();
        assert_eq!(cats.len(), 2);
        assert!(cats.iter().any(|c| c.category == "currentData"));
        assert!(cats.iter().any(|c| c.category == "identData"));
    }

    #[tokio::test]
    async fn test_groups_returns_unique() {
        let provider = MockProvider {
            items: vec![
                make_metadata("a", "currentData", vec!["front"]),
                make_metadata("b", "currentData", vec!["front", "rear"]),
            ],
        };

        let groups = provider.groups(None).await.unwrap();
        assert_eq!(groups.len(), 2);
        assert!(groups.iter().any(|g| g.id == "front"));
        assert!(groups.iter().any(|g| g.id == "rear"));
    }

    #[tokio::test]
    async fn test_groups_filters_by_category() {
        let provider = MockProvider {
            items: vec![
                make_metadata("a", "currentData", vec!["front"]),
                make_metadata("b", "identData", vec!["rear"]),
            ],
        };

        let groups = provider.groups(Some("currentData")).await.unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups.first().map(|g| g.id.as_str()), Some("front"));
    }
}
