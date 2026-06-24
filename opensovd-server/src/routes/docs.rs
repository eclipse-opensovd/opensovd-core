// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! SOVD online capability descriptions.
//!
//! `GET /{path}/docs` returns a self-contained OpenAPI 3.1 document for a single
//! endpoint: its methods, payload schemas and status codes. A client can use it
//! without an offline capability description for the whole vehicle.
//!
//! Data collection endpoints are wired up so far; see [`data_collection_docs`].

use opensovd_models::data::{DataList, Metadata};
use serde_json::{Value, json};

use crate::schema::JsonSchema;

/// Wrap a single path item into a self-contained OpenAPI 3.1 document.
///
/// The `server_url` is emitted as the sole `servers` entry so clients know the
/// runtime base URL (e.g. `http://host/sovd/v1`). Paths in the document are
/// relative to that base.
///
/// Embedded JSON Schemas (e.g. from `schemars`) carry their reusable
/// definitions under `$defs` and reference them with `#/$defs/...`. Inside an
/// OpenAPI document such fragment references resolve against the document root,
/// where no `$defs` exists. To keep the result a valid OpenAPI 3.1 document, the
/// definitions are hoisted into the document-level `components/schemas` and the
/// references are rewritten to `#/components/schemas/...`.
pub fn build_openapi_doc(title: &str, path: &str, path_item: Value, server_url: &str) -> Value {
    let mut path_item = path_item;
    let mut schemas = serde_json::Map::new();
    hoist_schema_defs(&mut path_item, &mut schemas);

    let mut paths = serde_json::Map::new();
    paths.insert(path.to_owned(), path_item);

    let mut doc = serde_json::Map::new();
    doc.insert("openapi".to_owned(), json!("3.1.0"));
    doc.insert(
        "info".to_owned(),
        json!({ "title": title, "version": "1.0.0" }),
    );
    doc.insert("paths".to_owned(), Value::Object(paths));
    doc.insert("servers".to_owned(), json!([{"url": server_url}]));
    if !schemas.is_empty() {
        doc.insert(
            "components".to_owned(),
            json!({ "schemas": Value::Object(schemas) }),
        );
    }

    let mut doc = Value::Object(doc);
    rewrite_defs_refs(&mut doc);
    doc
}

/// Move every embedded `$defs` map into a shared schema map.
fn hoist_schema_defs(value: &mut Value, schemas: &mut serde_json::Map<String, Value>) {
    match value {
        Value::Object(map) => {
            if let Some(Value::Object(defs)) = map.remove("$defs") {
                for (name, def) in defs {
                    schemas.entry(name).or_insert(def);
                }
            }
            for child in map.values_mut() {
                hoist_schema_defs(child, schemas);
            }
        }
        Value::Array(items) => {
            for child in items {
                hoist_schema_defs(child, schemas);
            }
        }
        _ => {}
    }
}

/// Rewrite JSON Schema `#/$defs/...` references to OpenAPI `#/components/schemas/...`.
fn rewrite_defs_refs(value: &mut Value) {
    match value {
        Value::Object(map) => {
            if let Some(Value::String(reference)) = map.get_mut("$ref")
                && let Some(name) = reference.strip_prefix("#/$defs/")
            {
                *reference = format!("#/components/schemas/{name}");
            }
            for child in map.values_mut() {
                rewrite_defs_refs(child);
            }
        }
        Value::Array(items) => {
            for child in items {
                rewrite_defs_refs(child);
            }
        }
        _ => {}
    }
}

/// Describe a data collection endpoint as an OpenAPI document.
///
/// The document covers `GET /.../data` and includes the collection's query
/// parameters plus an example payload containing the current metadata entries.
pub fn data_collection_docs(collection_path: &str, items: &[Metadata], server_url: &str) -> Value {
    build_openapi_doc(
        &format!("Data collection {collection_path}"),
        collection_path,
        json!({
            "get": {
                "summary": "List the data resources of the entity",
                "parameters": list_query_parameters(),
                "responses": {
                    "200": {
                        "description": "The data resources currently exposed by the entity.",
                        "content": {
                            "application/json": {
                                "schema": data_list_response_schema(),
                                "example": {
                                    "items": items,
                                },
                            },
                        },
                    },
                    "400": { "description": "The request query was invalid." },
                    "404": { "description": "The entity was not found." },
                    "500": { "description": "An internal data provider error occurred." },
                },
            }
        }),
        server_url,
    )
}

/// Document the `GET /.../data` query parameters using ISO-style repeated keys.
fn list_query_parameters() -> Value {
    json!([
        {
            "name": "groups",
            "in": "query",
            "required": false,
            "description": "Filter by data group. Repeat the parameter to select multiple groups.",
            "style": "form",
            "explode": true,
            "schema": {
                "type": "array",
                "items": { "type": "string" }
            }
        },
        {
            "name": "categories",
            "in": "query",
            "required": false,
            "description": "Filter by data category. Repeat the parameter to select multiple categories.",
            "style": "form",
            "explode": true,
            "schema": {
                "type": "array",
                "items": { "type": "string" }
            }
        },
        {
            "name": "tags",
            "in": "query",
            "required": false,
            "description": "Filter by tag. Repeat the parameter to select multiple tags.",
            "style": "form",
            "explode": true,
            "schema": {
                "type": "array",
                "items": { "type": "string" }
            }
        },
        {
            "name": "include-schema",
            "in": "query",
            "required": false,
            "description": "Include the JSON schema of the data list response in the response body.",
            "schema": { "type": "boolean", "default": false }
        }
    ])
}

/// Schema for the `/data` response envelope.
fn data_list_response_schema() -> Value {
    let mut schema = DataList::schema();
    if let Some(properties) = schema.get_mut("properties").and_then(Value::as_object_mut) {
        properties.insert(
            "schema".to_owned(),
            json!({
                "type": "object",
                "description": "Optional JSON Schema for the data list response when include-schema=true."
            }),
        );
    }
    schema
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_collection_documents_get_with_filters_and_example_items() {
        let doc = data_collection_docs(
            "/components/Engine/data",
            &[Metadata {
                id: "rpm".into(),
                name: "rpm".into(),
                category: opensovd_models::data::DataCategory::CurrentData,
                translation_id: None,
                groups: Some(vec!["powertrain".into()]),
                tags: Some(vec!["OBD".into()]),
            }],
            "http://localhost/sovd/v1",
        );
        let path = &doc["paths"]["/components/Engine/data"];

        assert!(path["get"].is_object(), "GET must be documented");
        assert!(path["put"].is_null(), "PUT must not be documented");
        assert_eq!(path["get"]["parameters"][0]["name"], "groups");
        assert_eq!(path["get"]["parameters"][0]["explode"], true);
        assert_eq!(
            path["get"]["responses"]["200"]["content"]["application/json"]["example"]["items"][0]["id"],
            "rpm"
        );
        assert!(path["get"]["responses"]["400"].is_object());
        assert!(path["get"]["responses"]["500"].is_object());
    }

    /// Collect every `$ref` string in a JSON value.
    fn collect_refs(value: &Value, out: &mut Vec<String>) {
        match value {
            Value::Object(map) => {
                if let Some(Value::String(reference)) = map.get("$ref") {
                    out.push(reference.clone());
                }
                for child in map.values() {
                    collect_refs(child, out);
                }
            }
            Value::Array(items) => items.iter().for_each(|v| collect_refs(v, out)),
            _ => {}
        }
    }

    #[test]
    fn embedded_schema_defs_are_hoisted_to_components_and_refs_resolve() {
        let doc = data_collection_docs(
            "/components/Engine/data",
            &[Metadata {
                id: "rpm".into(),
                name: "rpm".into(),
                category: opensovd_models::data::DataCategory::CurrentData,
                translation_id: None,
                groups: None,
                tags: None,
            }],
            "http://localhost/sovd/v1",
        );

        let mut refs = Vec::new();
        collect_refs(&doc, &mut refs);

        // No JSON-Schema-local `$defs` references must leak into the document.
        assert!(
            refs.iter().all(|r| !r.starts_with("#/$defs/")),
            "found unrewritten $defs refs: {refs:?}"
        );

        // Every `#/components/schemas/...` reference must resolve.
        let schemas = &doc["components"]["schemas"];
        for reference in &refs {
            if let Some(name) = reference.strip_prefix("#/components/schemas/") {
                assert!(
                    schemas.get(name).is_some(),
                    "dangling reference {reference} (schemas: {schemas:?})"
                );
            }
        }
    }
}
