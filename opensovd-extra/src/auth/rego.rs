// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Rego policy-based authorization using regorus.

use std::path::Path;

use opensovd_server::{Access, Authorizer};
use regorus::{CompiledPolicy, Engine, Value};

type Error = Box<dyn std::error::Error + Send + Sync>;

/// The rule a policy defines to allow a request.
pub const ENTRYPOINT: &str = "data.sovd.authz.allow";

/// Rego policy authorizer backed by the regorus engine.
///
/// Evaluates [`ENTRYPOINT`] with the [`Access`] as `input`. Fields that do not
/// apply, such as `principal` for anonymous callers, are left out.
#[derive(Clone)]
pub struct RegorusAuthorizer {
    policy: CompiledPolicy,
}

impl RegorusAuthorizer {
    /// Compiles `(name, source)` policies together with JSON data documents.
    ///
    /// # Errors
    ///
    /// Returns an error if a policy or data document cannot be parsed.
    pub fn new<'a>(
        policies: impl IntoIterator<Item = (&'a str, &'a str)>,
        data: impl IntoIterator<Item = &'a str>,
    ) -> Result<Self, Error> {
        let mut engine = Engine::new();
        // Builtin type errors make the rule undefined instead of failing the evaluation.
        engine.set_strict_builtin_errors(false);
        for (name, source) in policies {
            tracing::info!(target: "srv", policy = %name, "Loading rego policy");
            engine.add_policy(name.to_owned(), source.to_owned())?;
        }
        for json in data {
            engine.add_data(Value::from_json_str(json)?)?;
        }
        let policy = engine.compile_with_entrypoint(&ENTRYPOINT.into())?;
        Ok(Self { policy })
    }

    /// Like [`new`](Self::new), reading policies and data from files.
    ///
    /// # Errors
    ///
    /// Returns an error if a file cannot be read or its content cannot be parsed.
    pub fn from_paths(
        policies: &[impl AsRef<Path>],
        data: &[impl AsRef<Path>],
    ) -> Result<Self, Error> {
        let read = |path: &Path| {
            std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
        };
        let policies = policies
            .iter()
            .map(|p| Ok((p.as_ref().display().to_string(), read(p.as_ref())?)))
            .collect::<Result<Vec<_>, Error>>()?;
        let data = data
            .iter()
            .map(|p| read(p.as_ref()))
            .collect::<Result<Vec<_>, _>>()?;
        Self::new(
            policies.iter().map(|(n, s)| (n.as_str(), s.as_str())),
            data.iter().map(String::as_str),
        )
    }
}

impl Authorizer for RegorusAuthorizer {
    fn authorize(&self, access: &Access<'_>) -> bool {
        let input = match serde_json::to_value(access) {
            Ok(input) => Value::from(input),
            Err(e) => {
                tracing::error!(target: "srv", error = %e, "Rego input serialization failed");
                return false;
            }
        };
        match self.policy.eval_with_input(input) {
            Ok(allow) => allow == Value::Bool(true),
            Err(e) => {
                tracing::error!(target: "srv", error = %e, "Rego policy evaluation failed");
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use opensovd_server::Principal;

    use super::*;

    const POLICY: &str = r#"
package sovd.authz
import rego.v1

default allow := false

allow if input.route == "/version-info"

allow if {
    input.method == "GET"
    "reader" in input.principal.roles
    glob.match("/v1/**", ["/"], input.route)
}

allow if "admin" in input.principal.roles

allow if {
    input.method == "PUT"
    input.params.component_id in data.assignments[input.principal.subject]
}
"#;

    const DATA: &str = r#"{"assignments": {"tech@example.com": ["cabin"]}}"#;

    fn authorizer() -> RegorusAuthorizer {
        RegorusAuthorizer::new([("test.rego", POLICY)], [DATA]).unwrap()
    }

    fn principal(subject: &str, roles: &[&str]) -> Principal {
        Principal {
            subject: subject.to_owned(),
            roles: roles.iter().map(|r| (*r).to_owned()).collect(),
            scopes: Vec::new(),
            claims: serde_json::Map::new(),
        }
    }

    fn access<'a>(
        method: &'a str,
        route: Option<&'a str>,
        params: &[(&'a str, &'a str)],
        principal: Option<&'a Principal>,
    ) -> Access<'a> {
        Access {
            method,
            route,
            params: params.iter().copied().collect::<BTreeMap<_, _>>(),
            principal,
        }
    }

    const DATA_ROUTE: Option<&str> = Some("/v1/components/{component_id}/data/{data_id}");

    #[test]
    fn anonymous_version_info_allowed() {
        let authz = authorizer();
        assert!(authz.authorize(&access("GET", Some("/version-info"), &[], None)));
    }

    #[test]
    fn anonymous_data_denied() {
        let authz = authorizer();
        assert!(!authz.authorize(&access("GET", DATA_ROUTE, &[], None)));
    }

    #[test]
    fn reader_get_allowed() {
        let authz = authorizer();
        let reader = principal("reader@example.com", &["reader"]);
        assert!(authz.authorize(&access("GET", DATA_ROUTE, &[], Some(&reader))));
    }

    #[test]
    fn reader_put_denied() {
        let authz = authorizer();
        let reader = principal("reader@example.com", &["reader"]);
        assert!(!authz.authorize(&access("PUT", DATA_ROUTE, &[], Some(&reader))));
    }

    #[test]
    fn admin_put_allowed() {
        let authz = authorizer();
        let admin = principal("admin@example.com", &["admin"]);
        assert!(authz.authorize(&access("PUT", DATA_ROUTE, &[], Some(&admin))));
    }

    #[test]
    fn params_select_assigned_components() {
        let authz = authorizer();
        let tech = principal("tech@example.com", &[]);
        let put = |id| access("PUT", DATA_ROUTE, &[("component_id", id)], Some(&tech));
        assert!(authz.authorize(&put("cabin")));
        assert!(!authz.authorize(&put("display")));
    }

    #[test]
    fn unmatched_route_does_not_fail_other_rules() {
        let authz = authorizer();
        let admin = principal("admin@example.com", &["admin"]);
        assert!(authz.authorize(&access("GET", None, &[], Some(&admin))));
    }

    #[test]
    fn invalid_rego_content_errors() {
        assert!(RegorusAuthorizer::new([("bad", "not valid rego {{{{")], []).is_err());
    }
}
