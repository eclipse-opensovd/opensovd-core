# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

"""Shared fixtures for tests that run the gateway with mock entities."""

import pytest
from fixtures import default_binary_args


@pytest.fixture(scope="module")
def binary_args(request):
    """Enable mock entities for all tests in this directory."""
    return default_binary_args(request.config, "--mock")
