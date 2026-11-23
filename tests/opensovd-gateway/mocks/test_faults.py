# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

"""Gateway integration tests for the SOVD faults resource."""

import jsonschema
import pytest

FAULTS = "/v1/components/ecu/faults"


@pytest.mark.parametrize(
    ("params", "expected"),
    [
        (None, {"fuelPressureLow", "coolantSensorSignalLost"}),
        ({"scope": "powertrain"}, {"fuelPressureLow", "coolantSensorSignalLost"}),
        ({"severity": "3"}, {"fuelPressureLow"}),
        ({"tags": "powertrain"}, {"fuelPressureLow"}),
        ({"tags": "emissions"}, {"coolantSensorSignalLost"}),
        ({"tags": ["powertrain", "emissions"]}, {"fuelPressureLow", "coolantSensorSignalLost"}),
        ({"status[aggregatedStatus]": "active"}, {"fuelPressureLow"}),
        (
            [
                ("status[aggregatedStatus]", "active"),
                ("status[aggregatedStatus]", "passive"),
            ],
            {"fuelPressureLow", "coolantSensorSignalLost"},
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
    response = client.get(f"{FAULTS}/fuelPressureLow", params={"include-schema": "true"})
    assert response.status_code == 200
    body = response.json()
    assert body["item"]["code"] == "fuelPressureLow"
    assert body["environment_data"]["engineTemperature"] == 93
    jsonschema.validate(
        instance=body,
        schema=body["schema"],
        cls=jsonschema.Draft202012Validator,
    )


def test_unknown_fault_returns_resource_not_found(client):
    """An unknown fault code is reported as a missing resource."""
    response = client.get(f"{FAULTS}/unknownFault")
    assert response.status_code == 404
    assert response.json()["vendor_code"] == "resource-not-found"
