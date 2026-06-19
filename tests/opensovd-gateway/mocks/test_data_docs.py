# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

"""Tests for the online capability description endpoint `.../data/docs`.

These tests validate the returned payload against a real OpenAPI 3.1.0 schema
using `openapi-spec-validator`, in addition to checking the document is scoped
to the requested entity and the `data` endpoint only.
"""

import pytest
from openapi_spec_validator import validate
from openapi_spec_validator.validation.exceptions import OpenAPIValidationError

COMPONENT_DOCS = "/v1/components/ecu/data/docs"
APP_DOCS = "/v1/apps/engine_control/data/docs"


@pytest.mark.parametrize(
    ("path", "expected_collection"),
    [
        (COMPONENT_DOCS, "/components/ecu/data"),
        (APP_DOCS, "/apps/engine_control/data"),
    ],
    ids=["component", "app"],
)
def test_data_docs_is_valid_openapi_31(client, path, expected_collection):
    """The docs payload is a valid OpenAPI 3.1.0 document scoped to `data`."""
    response = client.get(path)
    assert response.status_code == 200

    doc = response.json()

    # Real OpenAPI 3.1.0 schema validation (raises on any violation).
    validate(doc)

    assert doc["openapi"] == "3.1.0"

    # Scoped to the requested entity and to the data endpoint only.
    assert list(doc["paths"].keys()) == [expected_collection]

    operation = doc["paths"][expected_collection]
    assert "get" in operation
    assert "put" not in operation, "only the read (GET) contract is documented"

    # HTTP response codes are part of the schema.
    responses = operation["get"]["responses"]
    assert "200" in responses
    assert "404" in responses


def test_data_docs_validator_rejects_malformed_document():
    """Guard: the validator actually fails on a non-OpenAPI payload."""
    with pytest.raises(OpenAPIValidationError):
        validate({"openapi": "3.1.0"})  # missing required `info`/`paths`


def test_data_docs_unknown_component_is_not_found(client):
    """Docs for a missing component return 404."""
    response = client.get("/v1/components/does-not-exist/data/docs")
    assert response.status_code == 404


def test_data_docs_unknown_app_is_not_found(client):
    """Docs for a missing app return 404."""
    response = client.get("/v1/apps/does-not-exist/data/docs")
    assert response.status_code == 404
