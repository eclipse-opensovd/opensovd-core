// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

use crate::GenericError;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
pub enum UpdateOrigins {
    #[default]
    #[serde(rename = "remote")]
    Remote,
    #[serde(rename = "proximity")]
    Proximity,
}

impl std::fmt::Display for UpdateOrigins {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UpdateOrigins::Remote => write!(f, "remote"),
            UpdateOrigins::Proximity => write!(f, "proximity"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
pub enum Phase {
    #[serde(rename = "prepare")]
    Prepare,
    #[serde(rename = "execute")]
    Execute,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
pub struct Progress {
    pub entity: String,
    pub status: Status,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<GenericError>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
pub enum Status {
    #[serde(rename = "pending")]
    Pending,
    #[serde(rename = "inProgress")]
    InProgress,
    #[serde(rename = "failed")]
    Failed,
    #[serde(rename = "completed")]
    Completed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
pub struct AvailableUpdatesResponse {
    pub items: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[serde(default, rename_all = "kebab-case")]
pub struct UpdateDetailQuery {
    pub include_schema: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[serde(default, rename_all = "kebab-case")]
pub struct UpdateQuery {
    pub target_version: Option<String>,
    pub origin: UpdateOrigins,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
pub struct UpdateDetail {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub update_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub automated: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin: Option<Vec<UpdateOrigins>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_translation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes_translation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_activity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_activity_translation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preconditions: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preconditions_translation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_conditions: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<u64>,
    pub size: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_components: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub affected_components: Option<Vec<String>>,
    // the following fields are not part of the UpdateDetail response and are
    // therefore only used to register the update with the server.
    #[serde(skip_serializing)]
    pub authentication: Option<String>,
    #[serde(skip_serializing)]
    pub authentication_token: Option<String>,
    #[serde(skip_serializing)]
    pub targets: Vec<Target>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
pub struct UpdateDetailResponse {
    #[serde(flatten)]
    pub data: UpdateDetail,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
pub struct Target {
    pub entity: String,
    pub file: String,
    pub file_signature: String,
    pub routine: Option<String>,
    pub routine_signature: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
pub struct UpdateStatus {
    pub phase: Phase,
    pub status: Status,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subprogress: Option<Vec<Progress>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step_translation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<GenericError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
pub struct UpdateStatusResponse {
    #[serde(flatten)]
    pub data: UpdateStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<serde_json::Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct UpdatePackagePath {
    pub update_package_id: String,
}

#[derive(Serialize)]
pub struct UpdatePackageId {
    pub id: String,
}
