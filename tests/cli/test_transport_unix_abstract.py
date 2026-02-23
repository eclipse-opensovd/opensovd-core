# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

"""Tests for the --unix-socket CLI option with abstract socket addresses."""

import sys
import uuid

import pytest

pytestmark = pytest.mark.skipif(
    sys.platform != "linux", reason="Abstract sockets only supported on Linux"
)


@pytest.fixture(scope="module")
def gateway_args():
    """Use abstract socket transport."""
    # Abstract sockets work with Docker via --network=host
    name = f"opensovd-test-{uuid.uuid4().hex[:8]}"
    return ["--unix-socket", f"@{name}"]


def test_abstract_socket_transport(gateway):
    """Verify the gateway listens on an abstract socket when given --unix-socket @name."""
    assert gateway.transport == "abstract"
    response = gateway.get("/version-info")
    assert response.status_code == 200
    data = response.json()
    assert "sovd_info" in data
