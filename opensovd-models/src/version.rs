// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

use crate::UriReference;

/// Version info response
#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
pub struct VersionInfo<V> {
    pub sovd_info: Vec<SovdInfo<V>>,
}

/// Single SOVD instance info
#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
pub struct SovdInfo<V> {
    pub version: String,
    pub base_uri: UriReference,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vendor_info: Option<V>,
}

/// Default vendor info type
#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
pub struct VendorInfo {
    pub version: String,
    pub name: String,
}

/// Query parameters for version-info endpoint
#[derive(Debug, Deserialize)]
pub struct VersionInfoQuery {
    #[serde(default, rename = "include-schema")]
    pub include_schema: bool,
}
