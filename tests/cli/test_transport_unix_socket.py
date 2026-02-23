# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

"""Tests for the --unix-socket CLI option with filesystem socket paths."""

import contextlib
import os
import sys
import uuid

import pytest

pytestmark = pytest.mark.skipif(
    sys.platform == "win32", reason="Unix sockets not supported on Windows"
)


@pytest.fixture(scope="module")
def socket_path():
    """Create a unique socket path and clean up after tests."""
    # Use short path to stay under macOS 104-byte SUN_LEN limit
    # Path must be in /tmp for Docker volume mount compatibility
    path = f"/tmp/sovd-{uuid.uuid4().hex[:8]}.sock"
    yield path
    with contextlib.suppress(OSError):
        os.unlink(path)


@pytest.fixture(scope="module")
def gateway_args(socket_path):
    """Use Unix socket transport."""
    return ["--unix-socket", socket_path]


def test_unix_socket_transport(gateway):
    """Verify the gateway listens on a filesystem socket when given --unix-socket /path."""
    assert gateway.transport == "unix"
    response = gateway.get("/version-info")
    assert response.status_code == 200
    data = response.json()
    assert "sovd_info" in data
