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
            errors: Vec::new(),
            schema: None,
        });
        self
    }

    /// Configures environment data for the last added fault.
    #[must_use]
    pub fn environment_data(mut self, environment_data: impl Into<Value>) -> Self {
        if let Some(last) = self.faults.last_mut() {
            last.environment_data = Some(environment_data.into());
        }
        self
    }

    /// Configures the environment data schema for the last added fault.
    #[must_use]
    pub fn environment_data_schema(mut self, schema: impl Into<Value>) -> Self {
        if let Some(last) = self.faults.last_mut() {
            last.schema = Some(schema.into());
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
    fn matches(fault: &Fault, filter: &FaultFilter) -> bool {
        let severity_matches = filter
            .severity
            .is_none_or(|maximum| fault.severity.is_some_and(|severity| severity <= maximum));
        let scope_matches = filter
            .scope
            .as_ref()
            .is_none_or(|scope| fault.scope.as_ref() == Some(scope));
        let tags_match =
            filter.tags.is_empty() || filter.tags.iter().any(|tag| fault.tags.contains(tag));
        let status_matches = filter.status.is_empty()
            || filter
                .status
                .iter()
                .any(|(key, expected)| Self::has_status(fault, key, expected));

        severity_matches && scope_matches && tags_match && status_matches
    }

    /// Whether the fault has the status attribute `key` with the `expected`
    /// value. A string must equal it, any other value its compact JSON.
    fn has_status(fault: &Fault, key: &str, expected: &str) -> bool {
        match fault.status.as_ref().and_then(|status| status.get(key)) {
            Some(Value::String(actual)) => actual == expected,
            Some(actual) => serde_json::to_string(actual).is_ok_and(|json| json == expected),
            None => false,
        }
    }
}

#[async_trait]
impl FaultProvider for BuiltFaultProvider {
    async fn fault_list(&self, filter: FaultFilter) -> Result<Vec<Fault>, FaultError> {
        Ok(self
            .faults
            .values()
            .filter(|detail| Self::matches(&detail.fault, &filter))
            .map(|detail| detail.fault.clone())
            .collect())
    }

    async fn fault_read(
        &self,
        fault_code: &str,
        include_schema: bool,
    ) -> Result<FaultDetail, FaultError> {
        let mut detail = self
            .faults
            .get(fault_code)
            .cloned()
            .ok_or_else(|| FaultError::NotFound(fault_code.to_owned()))?;
        if !include_schema {
            detail.schema = None;
        }
        Ok(detail)
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
            .environment_data_schema(serde_json::json!({"type": "object"}))
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

    fn codes(faults: &[Fault]) -> Vec<&str> {
        faults.iter().map(|fault| fault.code.as_str()).collect()
    }

    #[tokio::test]
    async fn lists_all_faults_in_registration_order_without_filters() {
        let faults = provider().fault_list(FaultFilter::default()).await.unwrap();
        assert_eq!(codes(&faults), ["active", "pending", "unknown-severity"]);
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
        assert_eq!(codes(&faults), ["pending"]);
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
        assert_eq!(codes(&faults), ["active", "pending"]);

        let faults = provider()
            .fault_list(FaultFilter {
                severity: Some(3),
                ..FaultFilter::default()
            })
            .await
            .unwrap();
        assert_eq!(codes(&faults), ["active"]);
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

        assert_eq!(codes(&faults), ["active", "pending"]);
    }

    #[tokio::test]
    async fn string_status_must_equal_the_predicate() {
        let faults = provider()
            .fault_list(FaultFilter {
                status: vec![("state".into(), "act".into())],
                ..FaultFilter::default()
            })
            .await
            .unwrap();
        assert!(faults.is_empty());
    }

    #[tokio::test]
    async fn status_predicate_does_not_match_another_key_with_the_same_value() {
        let provider = FaultProviderBuilder::new()
            .fault(fault("only-a", "ecu", Some(2), Some(("a", "same")), &[]))
            .build()
            .unwrap();
        let faults = provider
            .fault_list(FaultFilter {
                status: vec![("b".into(), "same".into())],
                ..FaultFilter::default()
            })
            .await
            .unwrap();

        assert!(faults.is_empty());
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
        assert_eq!(codes(&faults), ["active"]);
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
        let result = provider().fault_read("missing", false).await;
        assert!(matches!(result, Err(FaultError::NotFound(code)) if code == "missing"));
    }

    #[tokio::test]
    async fn returns_environment_data_schema_only_when_requested() {
        let provider = provider();

        let without_schema = provider.fault_read("active", false).await.unwrap();
        assert!(without_schema.schema.is_none());

        let with_schema = provider.fault_read("active", true).await.unwrap();
        assert_eq!(
            with_schema.schema,
            Some(serde_json::json!({"type": "object"}))
        );
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
