# SPDX-FileCopyrightText: 2026 Copyright (c) Contributors to the Eclipse Foundation
#
# See the NOTICE file(s) distributed with this work for additional
# information regarding copyright ownership.
#
# This program and the accompanying materials are made available under the
# terms of the Apache License Version 2.0 which is available at
# https://www.apache.org/licenses/LICENSE-2.0
#
# SPDX-License-Identifier: Apache-2.0

"""Pytest config: load the opensovd_e2e plugin, add OpenSOVD-core overrides."""

import pytest
from fixtures import default_binary_args

pytest_plugins = ["opensovd_e2e.plugin"]


@pytest.fixture(scope="module")
def crate_binary() -> str:
    """Default crate for the in-repo suite (override per crate dir, e.g. mcp)."""
    return "opensovd-gateway"


@pytest.fixture(scope="module")
def binary_args(request) -> list[str]:
    """Inject an ephemeral server ``--url`` by default (SOVD HTTP server)."""
    return default_binary_args(request.config)


@pytest.hookimpl(optionalhook=True)
def pytest_metadata(metadata):
    """Add project metadata to the test report (pytest-metadata hook)."""
    metadata["SOVD Version"] = "1.1.0"
