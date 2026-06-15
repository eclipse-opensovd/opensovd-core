# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
#
# See the NOTICE file(s) distributed with this work for additional
# information regarding copyright ownership.
#
# This program and the accompanying materials are made available under the
# terms of the Apache License Version 2.0 which is available at
# https://www.apache.org/licenses/LICENSE-2.0
#
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
