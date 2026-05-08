# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

"""Tests for the --url CLI option (TCP transport)."""

import pytest


@pytest.fixture(scope="module")
def binary_args():
    """Use TCP transport."""
    return ["--url", "http://127.0.0.1:0/sovd"]


def test_tcp_transport(client):
    """Verify the gateway is reachable over TCP when given --url.

    Ensures the gateway binds to the specified TCP address and responds
    to HTTP requests on that transport.
    """
    assert client.transport == "tcp"

    response = client.get("/version-info")
    assert response.status_code == 200
    data = response.json()
    assert "sovd_info" in data
