// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! In-memory fault provider building blocks.

use async_trait::async_trait;
use indexmap::IndexMap;
use opensovd_core::{Fault, FaultDetail, FaultError, FaultFilter, FaultProvider};
use serde_json::Value;
use thiserror::Error;

/// Error returned when building a [`FaultProvider`] fails.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BuildError {
    /// A duplicate fault code was registered.
    #[error("duplicate fault code: {0}")]
    DuplicateFaultCode(String),
}

/// Builder for an in-memory [`FaultProvider`].
#[derive(Default)]
pub struct FaultProviderBuilder {
    faults: Vec<FaultDetail>,
}

impl FaultProviderBuilder {
    /// Creates a new empty builder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a fault.
    #[must_use]
    pub fn fault(mut self, fault: Fault) -> Self {
        self.faults.push(FaultDetail {
            fault,
            environment_data: None,
        });
        self
    }

    /// Configures environment data for the last added fault.
    #[must_use]
    pub fn environment_data(mut self, environment_data: Value) -> Self {
        if let Some(last) = self.faults.last_mut() {
            last.environment_data = Some(environment_data);
        }
        self
    }

    /// Builds the [`FaultProvider`].
    ///
    /// # Errors
    ///
    /// Returns [`BuildError::DuplicateFaultCode`] if a fault code is registered more than once.
    pub fn build(self) -> Result<BuiltFaultProvider, BuildError> {
        let mut faults = IndexMap::with_capacity(self.faults.len());
        for detail in self.faults {
            let code = detail.fault.code.clone();
            if faults.insert(code.clone(), detail).is_some() {
                return Err(BuildError::DuplicateFaultCode(code));
            }
        }
        Ok(BuiltFaultProvider { faults })
    }
}

/// A [`FaultProvider`] built from registered faults.
pub struct BuiltFaultProvider {
    faults: IndexMap<String, FaultDetail>,
}

impl BuiltFaultProvider {
    fn matches(fault: &Fault, filter: &FaultFilter, status_values: &[Option<Value>]) -> bool {
        let severity_matches = filter
            .severity
            .is_none_or(|maximum| fault.severity.is_some_and(|severity| severity <= maximum));
        let scope_matches = filter
            .scope
            .as_ref()
            .is_none_or(|scope| fault.scope.as_ref() == Some(scope));
        let tags_match =
            filter.tags.is_empty() || filter.tags.iter().any(|tag| fault.tags.contains(tag));
        let status_filter_matches = filter.status.is_empty()
            || fault.status.as_ref().is_some_and(|status| {
                filter
                    .status
                    .iter()
                    .zip(status_values)
                    .any(|((key, value), expected_json)| {
                        status.get(key).is_some_and(|actual| {
                            Self::status_matches(actual, value, expected_json.as_ref())
                        })
                    })
            });

        severity_matches && scope_matches && tags_match && status_filter_matches
    }

    fn status_matches(actual: &Value, expected: &str, expected_json: Option<&Value>) -> bool {
        match actual {
            Value::String(actual) => actual == expected,
            other => expected_json == Some(other),
        }
    }
}

#[async_trait]
impl FaultProvider for BuiltFaultProvider {
    async fn fault_list(&self, filter: FaultFilter) -> Result<Vec<Fault>, FaultError> {
        let status_values: Vec<_> = filter
            .status
            .iter()
            .map(|(_, expected)| {
                let value = serde_json::from_str::<Value>(expected).ok()?;
                let serialized = serde_json::to_vec(&value).ok()?;
                (serialized == expected.as_bytes()).then_some(value)
            })
            .collect();
        Ok(self
            .faults
            .values()
            .filter(|detail| Self::matches(&detail.fault, &filter, &status_values))
            .map(|detail| detail.fault.clone())
            .collect())
    }

    async fn fault_read(&self, fault_code: &str) -> Result<FaultDetail, FaultError> {
        self.faults
            .get(fault_code)
            .cloned()
            .ok_or_else(|| FaultError::NotFound(fault_code.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Map;

    use super::*;

    fn fault(
        code: &str,
        scope: &str,
        severity: Option<u32>,
        status: Option<(&str, &str)>,
        tags: &[&str],
    ) -> Fault {
        Fault {
            code: code.into(),
            scope: Some(scope.into()),
            display_code: None,
            fault_name: format!("Fault {code}"),
            fault_translation_id: Some(format!("fault.{code}")),
            severity,
            status: status.map(|(key, value)| Map::from_iter([(key.into(), Value::from(value))])),
            tags: tags.iter().map(|tag| (*tag).into()).collect(),
            symptom: None,
            symptom_translation_id: None,
        }
    }

    fn provider() -> BuiltFaultProvider {
        FaultProviderBuilder::new()
            .fault(fault(
                "active",
                "ecu",
                Some(2),
                Some(("state", "active")),
                &["engine"],
            ))
            .environment_data(serde_json::json!({"temp": 93}))
            .fault(fault(
                "pending",
                "body",
                Some(4),
                Some(("state", "pending")),
                &["body"],
            ))
            .fault(fault(
                "unknown-severity",
                "ecu",
                None,
                Some(("state", "unknown")),
                &["other"],
            ))
            .build()
            .unwrap()
    }

    #[tokio::test]
    async fn lists_all_faults_in_registration_order_without_filters() {
        let faults = provider().fault_list(FaultFilter::default()).await.unwrap();
        let codes: Vec<_> = faults.iter().map(|fault| fault.code.as_str()).collect();
        assert_eq!(codes, ["active", "pending", "unknown-severity"]);
    }

    #[tokio::test]
    async fn filters_by_scope() {
        let faults = provider()
            .fault_list(FaultFilter {
                scope: Some("body".into()),
                ..FaultFilter::default()
            })
            .await
            .unwrap();
        assert_eq!(
            faults
                .iter()
                .map(|fault| fault.code.as_str())
                .collect::<Vec<_>>(),
            ["pending"]
        );
    }

    #[tokio::test]
    async fn filters_by_severity_and_excludes_faults_without_severity() {
        let faults = provider()
            .fault_list(FaultFilter {
                severity: Some(4),
                ..FaultFilter::default()
            })
            .await
            .unwrap();
        assert_eq!(
            faults
                .iter()
                .map(|fault| fault.code.as_str())
                .collect::<Vec<_>>(),
            ["active", "pending"]
        );

        let faults = provider()
            .fault_list(FaultFilter {
                severity: Some(3),
                ..FaultFilter::default()
            })
            .await
            .unwrap();
        assert_eq!(
            faults
                .iter()
                .map(|fault| fault.code.as_str())
                .collect::<Vec<_>>(),
            ["active"]
        );
    }

    #[tokio::test]
    async fn filters_by_status_with_or_semantics_for_repeated_keys() {
        let faults = provider()
            .fault_list(FaultFilter {
                status: vec![
                    ("state".into(), "active".into()),
                    ("state".into(), "pending".into()),
                ],
                ..FaultFilter::default()
            })
            .await
            .unwrap();

        assert_eq!(
            faults
                .iter()
                .map(|fault| fault.code.as_str())
                .collect::<Vec<_>>(),
            ["active", "pending"]
        );
    }

    #[tokio::test]
    async fn matches_non_string_status_by_its_exact_json_representation() {
        let numeric_fault = Fault {
            status: Some(Map::from_iter([("count".into(), Value::from(1))])),
            ..fault("numeric", "ecu", Some(2), None, &[])
        };
        let provider = FaultProviderBuilder::new()
            .fault(numeric_fault)
            .build()
            .unwrap();

        let matching = provider
            .fault_list(FaultFilter {
                status: vec![("count".into(), "1".into())],
                ..FaultFilter::default()
            })
            .await
            .unwrap();
        assert_eq!(matching.len(), 1);

        let non_matching = provider
            .fault_list(FaultFilter {
                status: vec![("count".into(), " 1".into())],
                ..FaultFilter::default()
            })
            .await
            .unwrap();
        assert!(non_matching.is_empty());
    }

    #[tokio::test]
    async fn filters_by_tag_without_other_filters() {
        let faults = provider()
            .fault_list(FaultFilter {
                tags: vec!["engine".into()],
                ..FaultFilter::default()
            })
            .await
            .unwrap();
        assert_eq!(
            faults
                .iter()
                .map(|fault| fault.code.as_str())
                .collect::<Vec<_>>(),
            ["active"]
        );
    }

    #[tokio::test]
    async fn status_and_tag_filters_are_and_combined() {
        let faults = provider()
            .fault_list(FaultFilter {
                status: vec![("state".into(), "pending".into())],
                tags: vec!["engine".into()],
                ..FaultFilter::default()
            })
            .await
            .unwrap();
        assert!(faults.is_empty());
    }

    #[tokio::test]
    async fn reading_unknown_fault_returns_not_found() {
        let result = provider().fault_read("missing").await;
        assert!(matches!(result, Err(FaultError::NotFound(code)) if code == "missing"));
    }

    #[test]
    fn rejects_duplicate_fault_codes() {
        let result = FaultProviderBuilder::new()
            .fault(fault("same", "ecu", None, None, &[]))
            .fault(fault("same", "ecu", None, None, &[]))
            .build();

        assert!(matches!(
            result,
            Err(BuildError::DuplicateFaultCode(code)) if code == "same"
        ));
    }
}
