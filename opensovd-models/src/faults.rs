// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Fault resource types.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::Collection;

/// A fault reported by an entity.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
pub struct Fault {
    /// Fault code in the native representation of the entity.
    pub code: String,
    /// Scope of this fault.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    /// Display code for this fault.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_code: Option<String>,
    /// Name or description of the fault.
    pub fault_name: String,
    /// Optional identifier for translating the fault name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fault_translation_id: Option<String>,
    /// Severity, where a lower number is more severe.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub severity: Option<u32>,
    /// OEM-specific status attributes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<Map<String, Value>>,
    /// Symptom associated with this fault.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symptom: Option<String>,
    /// Optional identifier for translating the symptom.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symptom_translation_id: Option<String>,
    /// Tags assigned to this fault.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
}

/// Fault collection response.
pub type FaultList = Collection<Fault>;

/// Detail response for one fault, including its environment data.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
pub struct FaultDetail {
    /// The requested fault.
    pub item: Fault,
    /// Optional environment information supplied with the fault.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment_data: Option<Value>,
    /// Schema of the response, sent for `include-schema=true`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<Value>,
}

/// Query parameters for listing faults.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct FaultListQuery {
    /// Include faults at this severity and all more severe levels.
    #[serde(default)]
    pub severity: Option<u32>,
    /// Filter faults by scope.
    #[serde(default)]
    pub scope: Option<String>,
    /// Filter faults by any of these tags.
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    /// Include the response schema.
    #[serde(default, rename = "include-schema")]
    pub include_schema: bool,
}

/// Query parameters for reading one fault.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct FaultDetailQuery {
    /// Include the response schema.
    #[serde(default, rename = "include-schema")]
    pub include_schema: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fault() -> Fault {
        Fault {
            code: "P0001".into(),
            scope: None,
            display_code: None,
            fault_name: "Fault P0001".into(),
            fault_translation_id: None,
            severity: None,
            status: None,
            symptom: None,
            symptom_translation_id: None,
            tags: None,
        }
    }

    #[test]
    fn fault_serializes_required_fields_and_omits_absent_optional_fields() {
        let value = serde_json::to_value(fault()).unwrap();
        assert_eq!(value["code"], "P0001");
        assert_eq!(value["fault_name"], "Fault P0001");
        assert!(value.get("fault_translation_id").is_none());
        assert!(value.get("status").is_none());
    }

    #[test]
    fn detail_keeps_environment_data_outside_the_fault_item() {
        let detail = FaultDetail {
            item: fault(),
            environment_data: Some(Value::Object(Map::from_iter([(
                "temperature".into(),
                Value::from(93),
            )]))),
            schema: None,
        };
        let value = serde_json::to_value(detail).unwrap();
        assert_eq!(value["environment_data"]["temperature"], 93);
    }

    #[test]
    fn detail_omits_absent_environment_data() {
        let detail = FaultDetail {
            item: fault(),
            environment_data: None,
            schema: None,
        };
        let value = serde_json::to_value(detail).unwrap();
        assert!(value.get("environment_data").is_none());
    }
}
