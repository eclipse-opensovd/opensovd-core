# SPDX-FileCopyrightText: 2026 Copyright (c) Contributors to the Eclipse Foundation
#
# See the NOTICE file(s) distributed with this work for additional
# information regarding copyright ownership.
#
# This program and the accompanying materials are made available under the
# terms of the Apache License Version 2.0 which is available at
# https://www.apache.org/licenses/LICENSE-2.0
#
# SPDX-License-Identifier: Apache-2.0

"""Generic API traversal tests with schema validation."""

import jsonschema
import pytest
from fixtures import default_binary_args


@pytest.fixture(scope="module")
def binary_args(request):
    """Enable mock entities for all tests in this module."""
    return default_binary_args(request.config, "--mock")


def validate_schema(data):
    """Validate response data against the schema it carries."""
    assert "schema" in data, "expected schema with include-schema=true"
    schema = data["schema"]
    assert "$schema" in schema, "Schema must have $schema property"
    jsonschema.validate(instance=data, schema=schema)


def get_json(client, url, params, include_schema):
    """GET a path or an advertised href and return its body."""
    response = client.get(url, params=params)
    assert response.status_code == 200, url
    body = response.json()
    if include_schema:
        validate_schema(body)
    return body


def get_references(client, capabilities, path, relation, params, include_schema):
    """Return the hrefs a reference collection lists.

    An advertised collection is read through its link and is not empty. A
    collection that is not advertised is still served at `path`, empty.
    """
    href = capabilities.get(relation)
    items = get_json(client, href or path, params, include_schema)["items"]
    if href:
        assert href.endswith(path), href
        assert items, f"{href} is advertised but empty"
    else:
        assert items == [], path
    return {item["href"] for item in items}


@pytest.mark.parametrize("include_schema", [False, True], ids=["without-schema", "with-schema"])
def test_traverse_api(client, include_schema):
    """Traverse all API endpoints, optionally validating schemas."""
    params = {"include-schema": "true"} if include_schema else None

    # GET /version-info
    response = client.get("/version-info", params=params)
    assert response.status_code == 200
    version_info = response.json()
    assert "sovd_info" in version_info
    if include_schema:
        validate_schema(version_info)

    # 1. GET /v1 root
    response = client.get("/v1", params=params)
    assert response.status_code == 200
    root = response.json()
    assert "components" in root
    if include_schema:
        validate_schema(root)

    # 2. GET /v1/components list
    response = client.get("/v1/components", params=params)
    assert response.status_code == 200
    components_list = response.json()
    assert "items" in components_list
    assert isinstance(components_list["items"], list)
    if include_schema:
        validate_schema(components_list)

    # Capabilities of every listed entity, keyed by the href it is listed under
    entities: dict[str, dict] = {}

    # 3. Traverse each component
    for component_item in components_list["items"]:
        component_id = component_item["id"]

        # GET component capabilities
        component = get_json(client, component_item["href"], params, include_schema)
        assert component["id"] == component_id
        entities[component_item["href"]] = component

        # GET data-categories
        response = client.get(f"/v1/components/{component_id}/data-categories")
        assert response.status_code == 200
        categories = response.json()
        assert "items" in categories

        # GET data-groups
        response = client.get(f"/v1/components/{component_id}/data-groups")
        assert response.status_code == 200
        groups = response.json()
        assert "items" in groups

        # GET data list
        response = client.get(f"/v1/components/{component_id}/data", params=params)
        assert response.status_code == 200
        data_list = response.json()
        assert "items" in data_list
        if include_schema:
            validate_schema(data_list)

        # GET each individual data item
        for data_item in data_list["items"]:
            data_id = data_item["id"]
            response = client.get(f"/v1/components/{component_id}/data/{data_id}", params=params)
            assert response.status_code == 200
            data_value = response.json()
            assert "data" in data_value
            assert "id" in data_value
            if include_schema:
                validate_schema(data_value)

    # 4. GET /v1/areas list
    response = client.get("/v1/areas", params=params)
    assert response.status_code == 200
    areas_list = response.json()
    assert "items" in areas_list
    assert isinstance(areas_list["items"], list)
    if include_schema:
        validate_schema(areas_list)

    # 5. Traverse each area
    contains: dict[str, set[str]] = {}
    for area_item in areas_list["items"]:
        area_id = area_item["id"]

        # GET area capabilities
        area = get_json(client, area_item["href"], params, include_schema)
        assert area["id"] == area_id
        entities[area_item["href"]] = area

        # GET area contains
        contains[area_item["href"]] = get_references(
            client, area, f"/v1/areas/{area_id}/contains", "contains", params, include_schema
        )

    # 6. GET /v1/apps list
    response = client.get("/v1/apps", params=params)
    assert response.status_code == 200
    apps_list = response.json()
    assert "items" in apps_list
    assert isinstance(apps_list["items"], list)
    if include_schema:
        validate_schema(apps_list)

    # 7. Traverse each app
    for app_item in apps_list["items"]:
        app_id = app_item["id"]

        # GET app capabilities
        app = get_json(client, app_item["href"], params, include_schema)
        assert app["id"] == app_id
        assert "is-located-on" in app  # Every mock app is hosted
        entities[app_item["href"]] = app

        # Verify data link in app capabilities (if app has data provider)
        if "data" in app:
            # GET app data-categories
            response = client.get(f"/v1/apps/{app_id}/data-categories")
            assert response.status_code == 200
            app_categories = response.json()
            assert "items" in app_categories

            # GET app data-groups
            response = client.get(f"/v1/apps/{app_id}/data-groups")
            assert response.status_code == 200
            app_groups = response.json()
            assert "items" in app_groups

            # GET app data list
            response = client.get(f"/v1/apps/{app_id}/data", params=params)
            assert response.status_code == 200
            app_data_list = response.json()
            assert "items" in app_data_list
            if include_schema:
                validate_schema(app_data_list)

            # GET each individual app data item
            for data_item in app_data_list["items"]:
                data_id = data_item["id"]
                response = client.get(f"/v1/apps/{app_id}/data/{data_id}", params=params)
                assert response.status_code == 200
                data_value = response.json()
                assert "data" in data_value
                assert "id" in data_value
                if include_schema:
                    validate_schema(data_value)

    # 8. GET component hosts
    hosts: dict[str, set[str]] = {}
    for component_item in components_list["items"]:
        component_id = component_item["id"]
        hosts[component_item["href"]] = get_references(
            client,
            entities[component_item["href"]],
            f"/v1/components/{component_id}/hosts",
            "hosts",
            params,
            include_schema,
        )

    # 9. belongs-to and is-located-on reference the listed area and component
    # directly; follow each link.
    located_on: set[tuple[str, str]] = set()
    belongs_to: set[tuple[str, str]] = set()
    for href, entity in entities.items():
        for link, pairs in (("is-located-on", located_on), ("belongs-to", belongs_to)):
            if link not in entity:
                continue
            target = entity[link]
            assert target in entities, f"{href} {link} {target} is not a listed entity"
            assert get_json(client, target, params, include_schema)["id"] == entities[target]["id"]
            pairs.add((href, target))

    # 10. Each relation matches its reverse relation in both directions
    assert located_on == {(app, comp) for comp, apps in hosts.items() for app in apps}
    assert belongs_to == {
        (entity, area) for area, members in contains.items() for entity in members
    }


@pytest.mark.parametrize(
    "path",
    [
        "/v1/components/unknown/hosts",
        "/v1/areas/unknown/contains",
    ],
)
def test_relation_of_unknown_entity(client, path):
    """Relations of an unknown entity report the entity, not the relation, as missing."""
    response = client.get(path)
    assert response.status_code == 404
    assert response.json()["vendor_code"] == "entity-not-found"


@pytest.mark.parametrize(
    "path",
    [
        "/v1/unknown",
        "/v1/components/ecu/unknown",
        # Removed: both links reference the area and component directly.
        "/v1/apps/engine_control/is-located-on",
        "/v1/apps/engine_control/belongs-to",
        "/v1/components/ecu/belongs-to",
    ],
)
def test_unknown_path(client, path):
    """Paths no route serves answer 404 with a GenericError body."""
    response = client.get(path)
    assert response.status_code == 404
    assert response.headers["content-type"] == "application/json"
    body = response.json()
    assert body["error_code"] == "vendor-specific"
    assert body["vendor_code"] == "resource-not-found"


def test_root_capabilities_areas_link(client):
    """Verify root capabilities include areas link."""
    response = client.get("/v1")
    assert response.status_code == 200
    root = response.json()
    assert "areas" in root
