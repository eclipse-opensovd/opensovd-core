# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

"""Generic API traversal tests with schema validation."""

import jsonschema
import pytest
from fixtures import default_gateway_args


@pytest.fixture(scope="module")
def gateway_args(request):
    """Enable mock entities for all tests in this module."""
    return default_gateway_args(request.config, "--mock")


def validate_schema(data, is_data_item=False):
    """Validate response data against its schema if present.

    Args:
        data: Response JSON data
        is_data_item: True if this is a data item response (schema validates 'data' field)
    """
    if "schema" not in data:
        return

    schema = data["schema"]

    if is_data_item:
        # Data item responses: schema describes the 'data' field
        jsonschema.validate(instance=data["data"], schema=schema)
    else:
        # Standard responses: schema describes the entire response
        assert "$schema" in schema, "Schema must have $schema property"
        jsonschema.validate(instance=data, schema=schema)


@pytest.mark.parametrize("include_schema", [False, True], ids=["without-schema", "with-schema"])
def test_traverse_api(gateway, include_schema):
    """Traverse all API endpoints, optionally validating schemas."""
    params = {"include-schema": "true"} if include_schema else None

    # GET /version-info
    response = gateway.get("/version-info", params=params)
    assert response.status_code == 200
    version_info = response.json()
    assert "sovd_info" in version_info
    if include_schema:
        validate_schema(version_info)

    # 1. GET /v1 root
    response = gateway.get("/v1", params=params)
    assert response.status_code == 200
    root = response.json()
    assert "components" in root
    if include_schema:
        validate_schema(root)

    # 2. GET /v1/components list
    response = gateway.get("/v1/components", params=params)
    assert response.status_code == 200
    components_list = response.json()
    assert "items" in components_list
    assert isinstance(components_list["items"], list)
    if include_schema:
        validate_schema(components_list)

    # 3. Traverse each component
    for component_item in components_list["items"]:
        component_id = component_item["id"]

        # GET component capabilities
        response = gateway.get(f"/v1/components/{component_id}", params=params)
        assert response.status_code == 200
        component = response.json()
        assert component["id"] == component_id
        if include_schema:
            validate_schema(component)

        # GET data-categories
        response = gateway.get(f"/v1/components/{component_id}/data-categories", params=params)
        assert response.status_code == 200
        categories = response.json()
        assert "items" in categories
        if include_schema:
            validate_schema(categories)

        # GET data-groups
        response = gateway.get(f"/v1/components/{component_id}/data-groups", params=params)
        assert response.status_code == 200
        groups = response.json()
        assert "items" in groups
        if include_schema:
            validate_schema(groups)

        # GET data list
        response = gateway.get(f"/v1/components/{component_id}/data", params=params)
        assert response.status_code == 200
        data_list = response.json()
        assert "items" in data_list
        if include_schema:
            validate_schema(data_list)

        # GET each individual data item
        for data_item in data_list["items"]:
            data_id = data_item["id"]
            response = gateway.get(f"/v1/components/{component_id}/data/{data_id}", params=params)
            assert response.status_code == 200
            data_value = response.json()
            assert "data" in data_value
            assert "id" in data_value
            if include_schema:
                validate_schema(data_value, is_data_item=True)

    # 4. GET /v1/areas list
    response = gateway.get("/v1/areas", params=params)
    assert response.status_code == 200
    areas_list = response.json()
    assert "items" in areas_list
    assert isinstance(areas_list["items"], list)
    if include_schema:
        validate_schema(areas_list)

    # 5. Traverse each area
    area_contains: dict[str, list[str]] = {}
    for area_item in areas_list["items"]:
        area_id = area_item["id"]

        # GET area capabilities
        response = gateway.get(f"/v1/areas/{area_id}", params=params)
        assert response.status_code == 200
        area = response.json()
        assert area["id"] == area_id
        assert "contains" in area  # All areas have contains link
        if include_schema:
            validate_schema(area)

        # GET area contains
        response = gateway.get(f"/v1/areas/{area_id}/contains", params=params)
        assert response.status_code == 200
        contains = response.json()
        assert "items" in contains
        if include_schema:
            validate_schema(contains)

        # Store for later validation
        area_contains[area_id] = [item["id"] for item in contains["items"]]

    # 6. GET /v1/apps list
    response = gateway.get("/v1/apps", params=params)
    assert response.status_code == 200
    apps_list = response.json()
    assert "items" in apps_list
    assert isinstance(apps_list["items"], list)
    if include_schema:
        validate_schema(apps_list)

    # 7. Traverse each app
    app_hosts: dict[str, str] = {}
    app_areas: dict[str, str] = {}
    for app_item in apps_list["items"]:
        app_id = app_item["id"]

        # GET app capabilities
        response = gateway.get(f"/v1/apps/{app_id}", params=params)
        assert response.status_code == 200
        app = response.json()
        assert app["id"] == app_id
        assert "is-located-on" in app  # Mandatory
        if include_schema:
            validate_schema(app)

        # GET app is-located-on
        response = gateway.get(f"/v1/apps/{app_id}/is-located-on", params=params)
        assert response.status_code == 200
        located_on = response.json()
        assert "items" in located_on
        assert len(located_on["items"]) == 1  # Exactly one host
        host_component_id = located_on["items"][0]["id"]
        if include_schema:
            validate_schema(located_on)

        # GET app belongs-to (optional)
        response = gateway.get(f"/v1/apps/{app_id}/belongs-to", params=params)
        assert response.status_code == 200
        belongs_to = response.json()
        assert "items" in belongs_to
        assert len(belongs_to["items"]) <= 1  # 0 or 1
        if include_schema:
            validate_schema(belongs_to)

        # Verify data link in app capabilities (if app has data provider)
        if "data" in app:
            # GET app data-categories
            response = gateway.get(f"/v1/apps/{app_id}/data-categories", params=params)
            assert response.status_code == 200
            app_categories = response.json()
            assert "items" in app_categories
            if include_schema:
                validate_schema(app_categories)

            # GET app data-groups
            response = gateway.get(f"/v1/apps/{app_id}/data-groups", params=params)
            assert response.status_code == 200
            app_groups = response.json()
            assert "items" in app_groups
            if include_schema:
                validate_schema(app_groups)

            # GET app data list
            response = gateway.get(f"/v1/apps/{app_id}/data", params=params)
            assert response.status_code == 200
            app_data_list = response.json()
            assert "items" in app_data_list
            if include_schema:
                validate_schema(app_data_list)

            # GET each individual app data item
            for data_item in app_data_list["items"]:
                data_id = data_item["id"]
                response = gateway.get(f"/v1/apps/{app_id}/data/{data_id}", params=params)
                assert response.status_code == 200
                data_value = response.json()
                assert "data" in data_value
                assert "id" in data_value
                if include_schema:
                    validate_schema(data_value, is_data_item=True)

        # Store relationships
        app_hosts[app_id] = host_component_id
        if len(belongs_to["items"]) > 0:
            app_areas[app_id] = belongs_to["items"][0]["id"]

    # 8. Test component relationship endpoints
    component_hosts: dict[str, list[str]] = {}
    component_areas: dict[str, str] = {}
    for component_item in components_list["items"]:
        component_id = component_item["id"]

        # GET component hosts
        response = gateway.get(f"/v1/components/{component_id}/hosts", params=params)
        assert response.status_code == 200
        hosts = response.json()
        assert "items" in hosts
        if include_schema:
            validate_schema(hosts)

        component_hosts[component_id] = [item["id"] for item in hosts["items"]]

        # GET component belongs-to
        response = gateway.get(f"/v1/components/{component_id}/belongs-to", params=params)
        assert response.status_code == 200
        belongs_to = response.json()
        assert "items" in belongs_to
        assert len(belongs_to["items"]) <= 1  # 0 or 1
        if include_schema:
            validate_schema(belongs_to)

        if len(belongs_to["items"]) > 0:
            component_areas[component_id] = belongs_to["items"][0]["id"]

    # 9. Validate relationship consistency
    # Verify app.is-located-on <-> component.hosts
    for app_id, host_component_id in app_hosts.items():
        assert app_id in component_hosts.get(host_component_id, []), (
            f"App {app_id} hosted on {host_component_id}, but not in hosts list"
        )

    # Reverse direction
    for component_id, hosted_apps in component_hosts.items():
        for app_id in hosted_apps:
            assert app_hosts.get(app_id) == component_id, (
                f"Component {component_id} hosts {app_id}, but mismatch in is-located-on"
            )

    # Verify area.contains <-> component.belongs-to
    for area_id, contained_ids in area_contains.items():
        for entity_id in contained_ids:
            if entity_id in component_areas:
                assert component_areas[entity_id] == area_id

    # Verify area.contains <-> app.belongs-to
    for area_id, contained_ids in area_contains.items():
        for entity_id in contained_ids:
            if entity_id in app_areas:
                assert app_areas[entity_id] == area_id


def test_root_capabilities_areas_link(gateway):
    """Verify root capabilities include areas link."""
    response = gateway.get("/v1")
    assert response.status_code == 200
    root = response.json()
    assert "areas" in root
