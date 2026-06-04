# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

"""Tests for the data filter on the data list endpoint."""

import pytest

DATA = "/v1/components/ecu/data"
ALL = {
    "voltage",
    "temperature",
    "sw.version",
    "sw.build_date",
    "sw.sha1",
    "hw.version",
    "hw.revision",
    "hw.sn",
}
IDENT = {"sw.version", "sw.build_date", "sw.sha1", "hw.version", "hw.revision", "hw.sn"}
CURRENT = {"voltage", "temperature"}


# The mock provider applies filters as AND across dimensions, OR within each.
@pytest.mark.parametrize(
    ("params", "expected"),
    [
        (None, ALL),
        ({"categories": "currentData"}, CURRENT),
        ({"categories": "identData"}, IDENT),
        ({"groups": "power"}, {"voltage"}),
        ({"groups": ["power", "thermal"]}, CURRENT),
        ({"tags": "sensor"}, CURRENT),
        ({"categories": "currentData", "tags": "sensor"}, CURRENT),
        ({"categories": "identData", "tags": "sensor"}, set()),
        ({"groups": "nonexistent"}, set()),
    ],
)
def test_data_list_filter(client, params, expected):
    """Test that groups/categories/tags narrow the data list."""
    response = client.get(DATA, params=params)
    assert response.status_code == 200
    ids = {item["id"] for item in response.json()["items"]}
    assert ids == expected
