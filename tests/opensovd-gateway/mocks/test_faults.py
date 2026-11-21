# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

"""Gateway integration tests for the SOVD faults resource."""

import pytest

FAULTS = "/v1/components/ecu/faults"


@pytest.mark.parametrize(
    ("params", "expected"),
    [
        (None, {"P0001", "P0002"}),
        ({"scope": "powertrain"}, {"P0001", "P0002"}),
        ({"severity": "3"}, {"P0001"}),
        ({"tags": "powertrain"}, {"P0001"}),
        ({"tags": "emissions"}, {"P0002"}),
        ({"tags": ["powertrain", "emissions"]}, {"P0001", "P0002"}),
        ({"status[confirmedDTC]": "1"}, {"P0001"}),
        (
            [
                ("status[aggregatedStatus]", "active"),
                ("status[aggregatedStatus]", "cleared"),
            ],
            {"P0001"},
        ),
        ({"tags": "unmatched"}, set()),
        ({"tags": "powertrain", "severity": "1"}, set()),
    ],
)
def test_fault_list_filters(client, params, expected):
    """List filters compose with AND; repeated status and tags match with OR."""
    response = client.get(FAULTS, params=params)
    assert response.status_code == 200
    codes = {fault["code"] for fault in response.json()["items"]}
    assert codes == expected


def test_fault_detail_includes_environment_data(client):
    """The detail response includes environment data beside the fault item."""
    response = client.get(f"{FAULTS}/P0001")
    assert response.status_code == 200
    body = response.json()
    assert body["item"]["code"] == "P0001"
    assert body["environment_data"]["engineTemperature"] == 93


def test_unknown_fault_returns_resource_not_found(client):
    """An unknown fault code is reported as a missing resource."""
    response = client.get(f"{FAULTS}/P9999")
    assert response.status_code == 404
    assert response.json()["vendor_code"] == "resource-not-found"
