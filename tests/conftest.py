# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

"""Pytest configuration and fixtures for end2end tests."""

import shlex
from pathlib import Path

import pytest
from fixtures import Gateway, default_gateway_args, spawn_gateway

# Re-export for other modules
__all__ = ["Gateway", "default_gateway_args", "spawn_gateway"]


# --- Session metadata (shown in HTML report header) ---

# Module-level variable to store config for later access
_config = None


def pytest_configure(config):
    """Store config for later access in other hooks."""
    global _config
    _config = config


@pytest.hookimpl(optionalhook=True)
def pytest_metadata(metadata):
    """Add project metadata to the test report (pytest-metadata hook)."""
    metadata["SOVD Version"] = "1.1.0"


@pytest.hookimpl(optionalhook=True)
def pytest_html_results_summary(prefix, summary, postfix):
    """Render metadata keys ending in _URL as clickable links at the top."""
    if _config is None:
        return
    try:
        from pytest_metadata.plugin import metadata_key

        metadata = _config.stash.get(metadata_key, {})
    except Exception:
        return

    for key, url in list(metadata.items()):
        if key.endswith("_URL") and url:
            label = key.replace("_URL", "").replace("_", " ")
            prefix.append(f'<p><strong>{label}:</strong> <a href="{url}">{url}</a></p>')
            del metadata[key]  # Remove from Environment table to avoid duplication


# --- Requirement tracking for HTML report ---


def pytest_html_results_table_header(cells):
    """Add Requirements column to HTML report table."""
    cells.insert(2, "<th>Requirements</th>")


def pytest_html_results_table_row(report, cells):
    """Populate Requirements column for each test."""
    reqs = ", ".join(report.req) if hasattr(report, "req") and report.req else ""
    cells.insert(2, f"<td>{reqs}</td>")


def pytest_sessionfinish(session, exitstatus):
    """Generate requirements traceability matrix."""
    req_map: dict[str, list[str]] = {}
    for item in session.items:
        for marker in item.iter_markers(name="req"):
            for req_id in marker.args:
                req_map.setdefault(req_id, []).append(item.nodeid)

    if req_map:
        output = Path("requirements-coverage.txt")
        with output.open("w") as f:
            f.write("# Requirements Traceability Matrix\n")
            f.write(f"# Total requirements covered: {len(req_map)}\n\n")
            for req_id, tests in sorted(req_map.items()):
                f.write(f"{req_id}:\n")
                for test in sorted(tests):
                    f.write(f"  - {test}\n")


def pytest_addoption(parser):
    parser.addoption(
        "--opensovd-binary", default=None, help="Path to pre-built opensovd-gateway binary"
    )
    parser.addoption(
        "--opensovd-docker", default=None, help="Docker image:tag to run instead of local binary"
    )
    parser.addoption(
        "--opensovd-args",
        default="",
        help="Additional arguments to pass to the gateway",
    )
    parser.addoption(
        "--opensovd-release", action="store_true", default=False, help="Build in release mode"
    )
    parser.addoption(
        "--opensovd-features", default="", help="Cargo features to enable (comma-separated)"
    )


@pytest.fixture(scope="module")
def gateway_args(request) -> list[str]:
    args = shlex.split(request.config.getoption("--opensovd-args"))
    return args or default_gateway_args(request.config)


@pytest.fixture(scope="module")
def gateway_banner() -> str:
    """Pattern to wait for before considering the gateway ready.

    Override this fixture to wait for different output patterns,
    e.g., for CLI tests that don't start a server.
    """
    return "Listening addr="


@pytest.fixture(scope="module")
def gateway_extra_features() -> list[str]:
    """Extra cargo features to compile into the gateway binary.

    Override in test modules that require optional features (e.g. ["tls"]).
    """
    return []


@pytest.fixture(scope="module")
def gateway_ssl_context():
    """SSL context for the gateway's httpx client.

    Override in TLS/mTLS test modules to return an ssl.SSLContext configured
    with the appropriate CA and (for mTLS) client certificate.
    """
    return None


@pytest.fixture(scope="module")
def gateway(request, gateway_args, gateway_banner, gateway_extra_features, gateway_ssl_context):
    gw = spawn_gateway(
        request.config,
        gateway_args,
        gateway_banner,
        extra_features=gateway_extra_features or None,
        ssl_context=gateway_ssl_context,
    )

    # Store URL for Bruno tests to access
    if gw.base_url:
        request.config._gateway_base_url = gw.base_url

    yield gw

    gw.close()


@pytest.hookimpl(tryfirst=True, hookwrapper=True)
def pytest_runtest_makereport(item, call):
    outcome = yield
    report = outcome.get_result()

    # Capture requirement markers for HTML report
    markers = list(item.iter_markers(name="req"))
    report.req = [arg for m in markers for arg in m.args]

    # Capture gateway output on failure
    if report.failed and hasattr(item, "funcargs"):
        gateway = item.funcargs.get("gateway")
        if gateway and gateway.has_output and not gateway._output_printed:
            gateway._output_printed = True
            report.sections.append(("Gateway Output", gateway.stdout))
