# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

"""Pytest config: load the opensovd_e2e plugin, add OpenSOVD-core overrides."""

import os

import pytest
from fixtures import default_binary_args

pytest_plugins = ["opensovd_e2e.plugin"]

# Report metadata taken from GitHub Actions when the suite runs there.
_GITHUB_METADATA = {
    "Repository": "GITHUB_REPOSITORY",
    "Branch": "GITHUB_REF_NAME",
    "Commit": "GITHUB_SHA",
    "Run ID": "GITHUB_RUN_ID",
}
_metadata: dict | None = None


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
    """Add project and CI metadata to the test report (pytest-metadata hook)."""
    global _metadata
    _metadata = metadata
    metadata["SOVD Version"] = "1.1.0"
    for key, var in _GITHUB_METADATA.items():
        if value := os.environ.get(var):
            metadata[key] = value


@pytest.hookimpl(optionalhook=True)
def pytest_html_report_title(report):
    """Title the HTML report after the binary and target it covers (pytest-html hook)."""
    metadata = _metadata or {}
    if binary := metadata.get("Binary"):
        target = metadata.get("Target")
        report.title = f"{binary} e2e on {target}" if target else f"{binary} e2e"
