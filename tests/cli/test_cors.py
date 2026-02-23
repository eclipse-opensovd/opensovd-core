# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

"""Tests for the --cors-* CLI options (CORS validation)."""

import pytest


@pytest.fixture(
    scope="module",
    params=[
        pytest.param(
            ["--cors-origin", "*", "--cors-credentials"],
            id="wildcard-origin-with-credentials",
        ),
        pytest.param(
            ["--cors-origin", "http://localhost", "--cors-header", "*", "--cors-credentials"],
            id="wildcard-header-with-credentials",
        ),
        pytest.param(
            ["--cors-origin", "*", "--cors-header", "*", "--cors-credentials"],
            id="wildcard-both-with-credentials",
        ),
    ],
)
def gateway_args(request):
    """Invalid CORS config (exits with error)."""
    return request.param


@pytest.fixture(scope="module")
def gateway_banner():
    """Skip waiting for server ready pattern since the gateway exits with error."""
    return None


def test_cors_validation_rejects_wildcard_with_credentials(gateway):
    """Test that wildcard origins/headers with credentials are rejected.

    tower-http panics at runtime when credentials are combined with wildcard
    origins or headers. The CLI validates this upfront and exits with an error.
    """
    gateway.process.wait(timeout=5.0)
    assert gateway.process.returncode == 2

    output = gateway.stdout
    assert "error:" in output
    assert "wildcard '*'" in output
    assert "credentials" in output
