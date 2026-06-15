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

"""Tests for the --unix-socket CLI option with abstract socket addresses."""

import sys
import uuid

import pytest

pytestmark = pytest.mark.skipif(
    sys.platform != "linux", reason="Abstract sockets only supported on Linux"
)


@pytest.fixture(scope="module")
def binary_args():
    """Use abstract socket transport."""
    # Abstract sockets work with Docker via --network=host
    name = f"opensovd-test-{uuid.uuid4().hex[:8]}"
    return ["--unix-socket", f"@{name}"]


def test_abstract_socket_transport(client):
    """Verify the gateway listens on an abstract socket when given --unix-socket @name."""
    assert client.transport == "abstract"
    response = client.get("/version-info")
    assert response.status_code == 200
    data = response.json()
    assert "sovd_info" in data
