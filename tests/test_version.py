# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

"""Tests for the /version-info endpoint."""

import jsonschema


def test_version_info(gateway):
    """Test that /version-info returns SOVD info without schema."""
    response = gateway.get("/version-info")
    assert response.status_code == 200
    data = response.json()
    assert "sovd_info" in data
    assert "schema" not in data
    assert len(data["sovd_info"]) > 0
    info = data["sovd_info"][0]
    assert "version" in info
    assert "base_uri" in info


def test_version_info_with_schema(gateway):
    """Test that /version-info returns valid schema when requested."""
    response = gateway.get("/version-info", params={"include-schema": "true"})
    assert response.status_code == 200
    data = response.json()
    assert "sovd_info" in data
    assert "schema" in data
    assert "$schema" in data["schema"]
    # Validate that the response data conforms to the returned schema
    jsonschema.validate(instance=data, schema=data["schema"])
