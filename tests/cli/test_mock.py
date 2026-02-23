# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

"""Tests for the --mock CLI option that populates the topology for testing."""

import jsonschema
import pytest
from fixtures import default_gateway_args


@pytest.fixture(scope="module")
def gateway_args(request):
    """Enable mock entities for all tests in this module."""
    return default_gateway_args(request.config, "--mock")


def find_component(items, component_id):
    """Find a component by ID in an items list."""
    component = next((item for item in items if item["id"] == component_id), None)
    assert component is not None, f"Component '{component_id}' not found"
    return component


def test_root_capabilities(gateway):
    """Test that /v1 returns entity capabilities with components link."""
    response = gateway.get("/v1")
    assert response.status_code == 200
    data = response.json()
    assert "id" in data
    assert "name" in data
    assert "components" in data
    assert data["components"].endswith("/v1/components")


def test_root_capabilities_with_schema(gateway):
    """Test that /v1 returns valid schema when requested."""
    response = gateway.get("/v1", params={"include-schema": "true"})
    assert response.status_code == 200
    data = response.json()
    assert "schema" in data
    assert "$schema" in data["schema"]
    jsonschema.validate(instance=data, schema=data["schema"])


def test_list_components(gateway):
    """Test that /v1/components returns a list of component items."""
    response = gateway.get("/v1/components")
    assert response.status_code == 200
    data = response.json()
    assert "items" in data
    assert isinstance(data["items"], list)


def test_list_components_with_schema(gateway):
    """Test that /v1/components returns valid schema when requested."""
    response = gateway.get("/v1/components", params={"include-schema": "true"})
    assert response.status_code == 200
    data = response.json()
    assert "items" in data
    assert "schema" in data
    assert "$schema" in data["schema"]
    jsonschema.validate(instance=data, schema=data["schema"])


def test_list_components_have_tags(gateway):
    """Test that components include their configured tags."""
    response = gateway.get("/v1/components")
    assert response.status_code == 200
    data = response.json()
    items = data["items"]
    assert len(items) >= 2

    ecu = find_component(items, "ecu")
    assert "tags" in ecu
    assert "powertrain" in ecu["tags"]
    assert "critical" in ecu["tags"]

    gateway_item = find_component(items, "gateway")
    assert "tags" in gateway_item
    assert "network" in gateway_item["tags"]


def test_list_components_filter_by_single_tag(gateway):
    """Test filtering components by a single tag."""
    # Filter by powertrain tag - should return only ECU
    response = gateway.get("/v1/components", params={"tags": "powertrain"})
    assert response.status_code == 200
    data = response.json()
    items = data["items"]
    assert len(items) == 1
    assert items[0]["id"] == "ecu"

    # Filter by network tag - should return only Gateway
    response = gateway.get("/v1/components", params={"tags": "network"})
    assert response.status_code == 200
    data = response.json()
    items = data["items"]
    assert len(items) == 1
    assert items[0]["id"] == "gateway"


def test_list_components_filter_by_multiple_tags(gateway):
    """Test filtering components by multiple tags uses OR logic."""
    # Filter by multiple tags (OR logic) - should return both
    response = gateway.get("/v1/components", params=[("tags", "powertrain"), ("tags", "network")])
    assert response.status_code == 200
    data = response.json()
    items = data["items"]
    assert len(items) == 2
    ids = [item["id"] for item in items]
    assert "ecu" in ids
    assert "gateway" in ids


def test_list_components_filter_by_nonexistent_tag(gateway):
    """Test that filtering by nonexistent tag returns empty list."""
    response = gateway.get("/v1/components", params={"tags": "nonexistent"})
    assert response.status_code == 200
    data = response.json()
    items = data["items"]
    assert len(items) == 0


def test_component_capabilities_has_variant(gateway):
    """Test that component capabilities include variant information."""
    response = gateway.get("/v1/components/ecu")
    assert response.status_code == 200
    data = response.json()
    assert data["id"] == "ecu"
    assert data["name"] == "Engine Control Unit"
    assert data["variant"] == {"variant": "v2", "manufacturer": "ACME"}


def test_component_unknown(gateway):
    """Test that unknown component returns 404 with error details."""
    response = gateway.get("/v1/components/unknown")
    assert response.status_code == 404
    data = response.json()
    assert data["vendor_code"] == "entity-not-found"
    assert "unknown" in data["message"]


def test_list_components_has_translation_id(gateway):
    """Test that components with translation_id return it in list response."""
    response = gateway.get("/v1/components")
    assert response.status_code == 200
    data = response.json()
    items = data["items"]

    # ECU has translation_id set
    ecu = find_component(items, "ecu")
    assert ecu.get("translation_id") == "ecu.name"

    # Gateway does not have translation_id (should be absent from response)
    gateway_item = find_component(items, "gateway")
    assert "translation_id" not in gateway_item


def test_component_capabilities_has_translation_id(gateway):
    """Test that component capabilities endpoint returns translation_id."""
    response = gateway.get("/v1/components/ecu")
    assert response.status_code == 200
    data = response.json()
    assert data["translation_id"] == "ecu.name"


def test_component_capabilities_without_translation_id(gateway):
    """Test that components without translation_id omit the field."""
    response = gateway.get("/v1/components/gateway")
    assert response.status_code == 200
    data = response.json()
    assert "translation_id" not in data
