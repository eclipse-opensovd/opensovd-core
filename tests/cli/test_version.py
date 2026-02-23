# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

"""Tests for the --version CLI option."""

import re

import pytest

# opensovd-gateway 0.1.1 (8ffa3f4-dirty 2025-12-16)
VERSION_PATTERN = re.compile(
    r"opensovd-gateway (\d+\.\d+\.\d+) \(([a-f0-9]+(?:-dirty)?) (\d{4}-\d{2}-\d{2})\)"
)


@pytest.fixture(scope="module")
def gateway_args():
    """Run --version command."""
    return ["--version"]


@pytest.fixture(scope="module")
def gateway_banner():
    """Skip waiting for server ready pattern since --version exits immediately."""
    return None


def test_cli_version(gateway):
    """Test that --version outputs version info and exits.

    Ensures version, git hash, and date are correctly collected during build.
    """
    match = gateway.wait_for(VERSION_PATTERN, timeout_seconds=5.0)
    gateway.process.wait(timeout=5.0)
    assert gateway.process.returncode == 0
    assert match.group(1)  # version
    assert match.group(2)  # git hash
    assert match.group(3)  # date
