// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Discovery types.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::{Items, UriReference};

/// Entity reference.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
pub struct EntityReference {
    /// Identifier for the Entity
    pub id: String,
    /// Name of the Entity
    pub name: String,
    /// Identifier for translating the name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub translation_id: Option<String>,
    /// URI of the Entity including `base_uri`
    pub href: UriReference,
    /// List of tags for the Entity
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
}

/// Entity collection response.
pub type Entities = Items<EntityReference>;

/// Entity collection query parameters.
#[derive(Debug, Deserialize)]
pub struct EntitiesQuery {
    /// Specifies whether the response should include schema information
    #[serde(default, rename = "include-schema")]
    pub include_schema: bool,

    /// Filter entities by tags (OR logic).
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Entity capabilities query parameters.
#[derive(Debug, Deserialize)]
pub struct EntityCapabilitiesQuery {
    #[serde(default, rename = "include-schema")]
    pub include_schema: bool,
}

/// Entity capabilities response.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
pub struct EntityCapabilities {
    // Required (M)
    /// Entity identifier (may be empty for `SOVDServer`)
    pub id: String,
    /// Name of the Entity (may be empty for `SOVDServer`)
    pub name: String,

    // Optional (O)
    /// Identifier for translating the name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub translation_id: Option<String>,

    // C3: variant identification
    /// Identification of the variant
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variant: Option<HashMap<String, String>>,

    // C1: resource collections (if entity supports)
    /// Reference to the configurations collection
    #[serde(skip_serializing_if = "Option::is_none")]
    pub configurations: Option<UriReference>,
    /// Reference to the bulk-data collection
    #[serde(skip_serializing_if = "Option::is_none", rename = "bulk-data")]
    pub bulk_data: Option<UriReference>,
    /// Reference to the data collection
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<UriReference>,
    /// Reference to the data-lists collection
    #[serde(skip_serializing_if = "Option::is_none", rename = "data-lists")]
    pub data_lists: Option<UriReference>,
    /// Reference to the faults collection
    #[serde(skip_serializing_if = "Option::is_none")]
    pub faults: Option<UriReference>,
    /// Reference to the operations collection
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operations: Option<UriReference>,
    /// Reference to the updates collection
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updates: Option<UriReference>,
    /// Reference to the modes collection
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modes: Option<UriReference>,

    // C4: root-level entity collections (only at /)
    /// Reference to the Areas collection (root level only)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub areas: Option<UriReference>,
    /// Reference to the Components collection (root level only)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub components: Option<UriReference>,
    /// Reference to the Apps collection (root level only)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub apps: Option<UriReference>,
    /// Reference to the Functions collection (root level only)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub functions: Option<UriReference>,

    // C5: child entity collections
    /// Reference to the subcomponents collection (only for Components)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subcomponents: Option<UriReference>,
    /// Reference to the subareas collection (only for Areas)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subareas: Option<UriReference>,

    // C2: reference collections for entity relationships
    /// Reference to the locks collection
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locks: Option<UriReference>,
    /// Reference to the logs resource
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logs: Option<UriReference>,
    /// Reference to belongs-to collection
    #[serde(skip_serializing_if = "Option::is_none", rename = "belongs-to")]
    pub belongs_to: Option<UriReference>,
    /// Reference to contains collection (only for Areas)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contains: Option<UriReference>,
    /// Reference to the communication-logs collection
    #[serde(skip_serializing_if = "Option::is_none", rename = "communication-logs")]
    pub communication_logs: Option<UriReference>,
    /// Reference to the cyclic-subscriptions collection
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "cyclic-subscriptions"
    )]
    pub cyclic_subscriptions: Option<UriReference>,
    /// Reference to depends-on collection
    #[serde(skip_serializing_if = "Option::is_none", rename = "depends-on")]
    pub depends_on: Option<UriReference>,
    /// Reference to hosts collection (only for Components)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hosts: Option<UriReference>,
    /// Reference to the Component where the App is located (only for Apps)
    #[serde(skip_serializing_if = "Option::is_none", rename = "is-located-on")]
    pub is_located_on: Option<UriReference>,
    /// Reference to the scripts collection
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scripts: Option<UriReference>,
    /// Reference to the triggers collection
    #[serde(skip_serializing_if = "Option::is_none")]
    pub triggers: Option<UriReference>,
}
