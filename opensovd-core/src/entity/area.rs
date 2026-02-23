// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Area entity implementation
//!
//! Areas represent logical views of vehicle architecture and can be used to describe
//! various vehicle topologies (e.g., domain architectures, zone architectures).

use std::collections::HashMap;
use std::fmt;

use crate::data::DataProvider;
use crate::entity::EntityRef;

/// Area entity representing a logical view of vehicle architecture
pub struct Area {
    entity_ref: EntityRef,
    name: String,
    metadata: HashMap<String, String>,
    tags: Vec<String>,
    translation_id: Option<String>,
    data_provider: Option<Box<dyn DataProvider>>,
}

impl fmt::Debug for Area {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Area")
            .field("entity_ref", &self.entity_ref)
            .field("name", &self.name)
            .field("metadata", &self.metadata)
            .field("tags", &self.tags)
            .field("translation_id", &self.translation_id)
            .field("data_provider", &self.data_provider.as_ref().map(|_| "..."))
            .finish()
    }
}

impl Area {
    /// Create a new Area
    #[must_use]
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            entity_ref: EntityRef::area(id),
            name: name.into(),
            metadata: HashMap::new(),
            tags: Vec::new(),
            translation_id: None,
            data_provider: None,
        }
    }

    /// Set metadata
    #[must_use]
    pub fn with_metadata(mut self, metadata: HashMap<String, String>) -> Self {
        self.metadata = metadata;
        self
    }

    /// Set tags
    #[must_use]
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    /// Set translation ID
    #[must_use]
    pub fn with_translation_id(mut self, translation_id: impl Into<String>) -> Self {
        self.translation_id = Some(translation_id.into());
        self
    }

    /// Set data provider
    #[must_use]
    pub fn with_data_provider(mut self, provider: impl DataProvider) -> Self {
        self.data_provider = Some(Box::new(provider));
        self
    }

    /// Get area ID
    #[must_use]
    pub fn id(&self) -> &str {
        self.entity_ref.id()
    }

    /// Get area name
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get metadata
    #[must_use]
    pub const fn metadata(&self) -> &HashMap<String, String> {
        &self.metadata
    }

    /// Get tags
    #[must_use]
    pub fn tags(&self) -> &[String] {
        &self.tags
    }

    /// Get translation ID
    #[must_use]
    pub fn translation_id(&self) -> Option<&str> {
        self.translation_id.as_deref()
    }

    /// Get data provider
    #[must_use]
    pub fn data_provider(&self) -> Option<&dyn DataProvider> {
        self.data_provider.as_deref()
    }

    /// Returns a lightweight entity reference for this area.
    #[must_use]
    pub const fn entity_ref(&self) -> &EntityRef {
        &self.entity_ref
    }
}
