// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Component entity implementation.

use std::collections::HashMap;
use std::fmt;

use crate::data::DataProvider;
use crate::entity::EntityRef;

pub struct Component {
    entity_ref: EntityRef,
    name: String,
    area_id: Option<String>,
    metadata: HashMap<String, String>,
    tags: Vec<String>,
    translation_id: Option<String>,
    data_provider: Option<Box<dyn DataProvider>>,
}

impl fmt::Debug for Component {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Component")
            .field("entity_ref", &self.entity_ref)
            .field("name", &self.name)
            .field("area_id", &self.area_id)
            .field("metadata", &self.metadata)
            .field("tags", &self.tags)
            .field("translation_id", &self.translation_id)
            .field("data_provider", &self.data_provider.as_ref().map(|_| "..."))
            .finish()
    }
}

impl Component {
    #[must_use]
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            entity_ref: EntityRef::component(id),
            name: name.into(),
            area_id: None,
            metadata: HashMap::new(),
            tags: Vec::new(),
            translation_id: None,
            data_provider: None,
        }
    }

    #[must_use]
    pub fn with_metadata(mut self, metadata: HashMap<String, String>) -> Self {
        self.metadata = metadata;
        self
    }

    #[must_use]
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    #[must_use]
    pub fn with_translation_id(mut self, translation_id: impl Into<String>) -> Self {
        self.translation_id = Some(translation_id.into());
        self
    }

    #[must_use]
    pub fn with_data_provider(mut self, provider: impl DataProvider) -> Self {
        self.data_provider = Some(Box::new(provider));
        self
    }

    #[must_use]
    pub fn with_area_id(mut self, area_id: impl Into<String>) -> Self {
        self.area_id = Some(area_id.into());
        self
    }
}

impl Component {
    #[must_use]
    pub fn id(&self) -> &str {
        self.entity_ref.id()
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub const fn metadata(&self) -> &HashMap<String, String> {
        &self.metadata
    }

    #[must_use]
    pub fn tags(&self) -> &[String] {
        &self.tags
    }

    #[must_use]
    pub fn translation_id(&self) -> Option<&str> {
        self.translation_id.as_deref()
    }

    #[must_use]
    pub fn data_provider(&self) -> Option<&dyn DataProvider> {
        self.data_provider.as_deref()
    }

    #[must_use]
    pub fn area_id(&self) -> Option<&str> {
        self.area_id.as_deref()
    }

    /// Returns a lightweight entity reference for this component.
    #[must_use]
    pub const fn entity_ref(&self) -> &EntityRef {
        &self.entity_ref
    }
}
