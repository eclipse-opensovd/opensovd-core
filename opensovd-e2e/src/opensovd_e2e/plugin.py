# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

"""Generic pytest plugin for end-to-end testing of OpenSOVD binaries."""

import re
import shlex
from collections.abc import Iterable, Iterator

import pytest

from opensovd_e2e.netns import NetworkNamespace, skip_unless_available
from opensovd_e2e.process import ProcessUnderTest, spawn_process

# Stored in pytest_configure for later access in report hooks.
_config = None


def pytest_addoption(parser):
    parser.addoption(
        "--opensovd-run",
        default=None,
        help="Command prefix to run instead of building from source; test args are appended",
    )
    parser.addoption(
        "--opensovd-args",
        default="",
        help="Additional arguments to pass to the binary",
    )
    parser.addoption(
        "--opensovd-profile",
        default=None,
        help="Cargo profile to build (default: dev; e.g. release, release-small)",
    )
    parser.addoption(
        "--opensovd-target",
        default=None,
        help="Cargo --target triple; needed when artifacts live under target/<triple>/...",
    )
    parser.addoption(
        "--opensovd-features", default="", help="Cargo features to enable (comma-separated)"
    )


def pytest_configure(config):
    """Store config for report hooks and validate options."""
    global _config
    _config = config
    if config.getoption("--opensovd-run"):
        for flag in ("--opensovd-profile", "--opensovd-target", "--opensovd-features"):
            if config.getoption(flag):
                raise pytest.UsageError(f"{flag} has no effect when --opensovd-run is set")


@pytest.fixture(scope="module")
def crate_binary() -> str | None:
    """Cargo crate binary to build/run when ``--opensovd-run`` is unset.

    No generic default. Consumers override this with their crate name (e.g.
    ``opensovd-gateway``). Unused when ``--opensovd-run`` is
    given, which is the path consumers without an in-tree crate take.
    """
    return None


@pytest.fixture(scope="module")
def binary_args(request) -> list[str]:
    """Arguments passed to the binary.

    Generic default is just the ``--opensovd-args`` passthrough. Consumers
    override to inject binary-specific defaults (e.g. a server ``--url``).
    """
    return shlex.split(request.config.getoption("--opensovd-args"))


@pytest.fixture(scope="module")
def ready_banner() -> re.Pattern | None:
    """Pattern to wait for in stdout before treating the process as ready.

    Default is None (no wait). Suitable for CLI commands like ``--version``
    that print and exit. Consumers override for long-running servers.
    """
    return None


@pytest.fixture(scope="module")
def process(request, crate_binary, binary_args, ready_banner) -> ProcessUnderTest:
    """Generic process-under-test fixture.

    Spawns the binary (or the ``--opensovd-run`` command) with ``binary_args``
    and waits for ``ready_banner`` (None = no wait). Consumers may build richer
    fixtures (e.g. an HTTP client) on top of this.
    """
    proc = spawn_process(
        request.config,
        binary_args,
        ready_banner,
        crate=crate_binary,
    )
    yield proc
    proc.close()


@pytest.fixture
def netns() -> Iterator[NetworkNamespace]:
    """A private network namespace, removed with everything inside after the test.

    Skipped where unprivileged user namespaces are unavailable.
    """
    skip_unless_available()
    with NetworkNamespace() as netns:
        yield netns


@pytest.hookimpl(optionalhook=True)
def pytest_html_results_summary(prefix, summary, postfix):
    """Render metadata keys ending in _URL as clickable links at the top."""
    if _config is None:
        return
    try:
        from pytest_metadata.plugin import metadata_key
    except ImportError:
        return

    metadata = _config.stash.get(metadata_key, {})
    for key, url in list(metadata.items()):
        if key.endswith("_URL") and url:
            label = key.replace("_URL", "").replace("_", " ")
            prefix.append(f'<p><strong>{label}:</strong> <a href="{url}">{url}</a></p>')
            del metadata[key]  # Remove from Environment table to avoid duplication


def _find_process(funcargs: Iterable) -> ProcessUnderTest | None:
    """Find the ProcessUnderTest among a test's fixture values, if any.

    Matches a process fixture directly, or one held as an attribute on a
    client/wrapper fixture (e.g. a SovdClient holding its server process).
    """
    values = list(funcargs)
    for val in values:
        if isinstance(val, ProcessUnderTest):
            return val
    for val in values:
        for inner in vars(val).values() if hasattr(val, "__dict__") else ():
            if isinstance(inner, ProcessUnderTest):
                return inner
    return None


@pytest.hookimpl(tryfirst=True, hookwrapper=True)
def pytest_runtest_makereport(item, call):
    """Capture process output on failure."""
    outcome = yield
    report = outcome.get_result()

    if report.failed and hasattr(item, "funcargs"):
        proc = _find_process(item.funcargs.values())
        if proc and proc.has_output and not proc._output_printed:
            proc._output_printed = True
            report.sections.append(("Process Output", proc.stdout))
