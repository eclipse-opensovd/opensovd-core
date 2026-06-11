# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

"""Integration tests for scoped data docs endpoints."""

import pytest
from fixtures import default_binary_args
from openapi_spec_validator import validate


@pytest.fixture(scope="module")
def binary_args(request):
    """Enable mock entities for this module."""
    return default_binary_args(request.config, "--mock")


def _first_entity_id(client, path):
    response = client.get(path)
    assert response.status_code == 200
    payload = response.json()
    assert payload["items"], f"Expected at least one entity at {path}"
    return payload["items"][0]["id"]


def test_component_data_docs_returns_valid_openapi(client):
    component_id = _first_entity_id(client, "/v1/components")

    response = client.get(f"/v1/components/{component_id}/data/docs")
    assert response.status_code == 200

    payload = response.json()
    assert payload["openapi"] == "3.1.0"
    assert "paths" in payload
    assert f"/components/{component_id}/data" in payload["paths"]
    assert f"/components/{component_id}/data/{{data_id}}" in payload["paths"]

    validate(payload)


def test_app_data_docs_returns_valid_openapi(client):
    app_id = _first_entity_id(client, "/v1/apps")

    response = client.get(f"/v1/apps/{app_id}/data/docs")
    assert response.status_code == 200

    payload = response.json()
    assert payload["openapi"] == "3.1.0"
    assert "paths" in payload
    assert f"/apps/{app_id}/data" in payload["paths"]
    assert f"/apps/{app_id}/data/{{data_id}}" in payload["paths"]

    validate(payload)


def test_component_data_docs_missing_entity_returns_404(client):
    response = client.get("/v1/components/missing-component/data/docs")
    assert response.status_code == 404

    payload = response.json()
    assert payload["vendor_code"] == "entity-not-found"
