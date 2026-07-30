# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

"""Generic pytest plugin for end-to-end testing of OpenSOVD binaries."""

import os
import re
import shlex
import subprocess
from collections.abc import Iterable
from pathlib import Path

import pytest

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
    parser.addoption(
        "--opensovd-coverage",
        action="store_true",
        default=False,
        help="Instrument the workspace with cargo-llvm-cov; writes coverage.json + HTML + "
        "Cobertura at session end",
    )


def pytest_configure(config):
    """Store config for report hooks, validate options, register the req marker."""
    global _config
    _config = config
    config.addinivalue_line(
        "markers", "req(id): requirement ID for traceability (e.g. req('SOVD-7.1.1'))"
    )
    if config.getoption("--opensovd-run"):
        for flag in ("--opensovd-profile", "--opensovd-target", "--opensovd-features"):
            if config.getoption(flag):
                raise pytest.UsageError(f"{flag} has no effect when --opensovd-run is set")
    if config.getoption("--opensovd-coverage"):
        _setup_coverage(config)


def _setup_coverage(config):
    """Clean prior coverage data and inject cargo-llvm-cov's instrumentation env.

    Mirrors `source <(cargo llvm-cov show-env --export-prefix)`: the cargo builds
    and the spawned binary both inherit os.environ, so RUSTFLAGS instruments the
    build and LLVM_PROFILE_FILE makes the running binary emit profile data. Runs
    from pytest_configure, before collection builds the first test binary.
    """
    if config.getoption("--opensovd-run"):
        raise pytest.UsageError(
            "--opensovd-coverage requires building from source; it cannot be combined "
            "with --opensovd-run"
        )
    project_root = Path(config.rootpath)
    show_env = subprocess.run(
        ["cargo", "llvm-cov", "show-env"],
        cwd=project_root,
        check=True,
        stdout=subprocess.PIPE,
        text=True,
    )
    for line in show_env.stdout.splitlines():
        key, sep, value = line.partition("=")
        if sep:
            # show-env quotes values; shlex unwraps single/double/unquoted alike.
            os.environ[key] = shlex.split(value)[0] if value else ""
    subprocess.run(["cargo", "llvm-cov", "clean", "--workspace"], cwd=project_root, check=True)

    # Render the report at teardown. Config cleanups run after every
    # sessionfinish hook (incl. the trylast hook in tests/bruno/conftest.py that
    # closes the shared gateway), so all instrumented processes have exited and
    # flushed their profile data. Registered only on success, so the UsageError
    # path above never triggers a report.
    config.add_cleanup(lambda: _write_coverage_report(project_root))


def _write_coverage_report(project_root: Path):
    """Render the merged coverage data to coverage.json, HTML, and Cobertura.

    Reads the profile data via the env injected by _setup_coverage; no rebuild.
    """
    html_dir = project_root / "target" / "llvm-cov" / "html"
    subprocess.run(
        ["cargo", "llvm-cov", "report", "--json", "--output-path", "coverage.json"],
        cwd=project_root,
        check=True,
    )
    subprocess.run(["cargo", "llvm-cov", "report", "--html"], cwd=project_root, check=True)
    subprocess.run(
        [
            "cargo",
            "llvm-cov",
            "report",
            "--cobertura",
            "--output-path",
            str(html_dir / "cobertura.xml"),
        ],
        cwd=project_root,
        check=True,
    )


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


@pytest.hookimpl(optionalhook=True)
def pytest_html_results_table_header(cells):
    """Add Requirements column to HTML report table."""
    cells.insert(2, "<th>Requirements</th>")


@pytest.hookimpl(optionalhook=True)
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
        output = Path(session.config.rootpath) / "requirements-coverage.txt"
        with output.open("w") as f:
            f.write("# Requirements Traceability Matrix\n")
            f.write(f"# Total requirements covered: {len(req_map)}\n\n")
            for req_id, tests in sorted(req_map.items()):
                f.write(f"{req_id}:\n")
                for test in sorted(tests):
                    f.write(f"  - {test}\n")


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
    """Capture req markers for the report, and process output on failure."""
    outcome = yield
    report = outcome.get_result()

    # Capture requirement markers for HTML report
    markers = list(item.iter_markers(name="req"))
    report.req = [arg for m in markers for arg in m.args]

    # Capture process output on failure
    if report.failed and hasattr(item, "funcargs"):
        proc = _find_process(item.funcargs.values())
        if proc and proc.has_output and not proc._output_printed:
            proc._output_printed = True
            report.sections.append(("Process Output", proc.stdout))
