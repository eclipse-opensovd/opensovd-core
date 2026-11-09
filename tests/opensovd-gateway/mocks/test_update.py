# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

"""Gateway integration tests for the update capabilities.

Uses the ``--mock`` topology which provides updates on the server and on the
``ota_manager`` app.
"""

UPDATE_HOST = "ota_manager"


def test_server_capabilities_include_updates(client):
    """Server capabilities response includes an updates link."""
    resp = client.get("/v1")
    assert resp.status_code == 200
    body = resp.json()
    assert "updates" in body, f"missing updates in capabilities: {list(body.keys())}"


def test_app_capabilities_include_updates(client):
    """App capabilities response includes an updates link for the OTA manager."""
    resp = client.get(f"/v1/apps/{UPDATE_HOST}")
    assert resp.status_code == 200
    body = resp.json()
    assert "updates" in body, f"missing updates in capabilities: {list(body.keys())}"
