# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

"""Tests for the --url CLI option."""

import pytest
from fixtures import default_binary_args


@pytest.fixture(
    scope="module",
    params=[
        pytest.param(["--url", "/sovd"], id="without-authority"),
        pytest.param(["--url", "http://[::1/sovd"], id="malformed"),
    ],
)
def binary_args(request):
    """Invalid --url (exits with error)."""
    return default_binary_args(request.config, *request.param)


@pytest.fixture(scope="module")
def ready_banner():
    """Skip waiting for server ready pattern since the gateway exits with error."""
    return None


def test_invalid_url_is_a_usage_error(gateway):
    gateway.process.wait(timeout=5.0)
    assert gateway.process.returncode == 2
    assert "invalid value" in gateway.stdout
    assert "--url" in gateway.stdout
