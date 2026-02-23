// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! App entity implementation.

use std::collections::HashMap;
use std::fmt;

use crate::data::DataProvider;
use crate::entity::EntityRef;

pub struct App {
    entity_ref: EntityRef,
    name: String,
    is_located_on: String,
    area_id: Option<String>,
    metadata: HashMap<String, String>,
    tags: Vec<String>,
    translation_id: Option<String>,
    data_provider: Option<Box<dyn DataProvider>>,
}

impl fmt::Debug for App {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("App")
            .field("entity_ref", &self.entity_ref)
            .field("name", &self.name)
            .field("is_located_on", &self.is_located_on)
            .field("area_id", &self.area_id)
            .field("metadata", &self.metadata)
            .field("tags", &self.tags)
            .field("translation_id", &self.translation_id)
            .field("data_provider", &self.data_provider.as_ref().map(|_| "..."))
            .finish()
    }
}

impl App {
    #[must_use]
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        is_located_on: impl Into<String>,
    ) -> Self {
        let id_str = id.into();
        Self {
            entity_ref: EntityRef::app(&id_str),
            name: name.into(),
            is_located_on: is_located_on.into(),
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

impl App {
    #[must_use]
    pub fn id(&self) -> &str {
        self.entity_ref.id()
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn component_id(&self) -> Option<&str> {
        Some(&self.is_located_on)
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

    /// Returns a lightweight entity reference for this app.
    #[must_use]
    pub const fn entity_ref(&self) -> &EntityRef {
        &self.entity_ref
    }
}
