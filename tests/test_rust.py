# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

"""Run Rust unit tests via pytest.

Tests are skipped when --opensovd-binary or --opensovd-docker is specified.
"""

import re
import subprocess

import pytest


def parse_cargo_test_output(output: str) -> list[tuple[str, str]]:
    """Parse cargo test --list output, returning (binary_path, test_name) tuples.

    This is extracted as a separate function to enable unit testing of the parsing logic.
    """
    tests = []
    current_binary = None

    for line in output.splitlines():
        # Match: "     Running unittests src/lib.rs (target/debug/deps/foo-xxx)"
        # or:    "     Running tests/builder.rs (target/debug/deps/builder-xxx)"
        if "Running" in line and "(" in line:
            match = re.search(r"\(([^)]+)\)", line)
            if match:
                current_binary = match.group(1)
        # Match: "   Doc-tests opensovd_core"
        elif line.strip().startswith("Doc-tests"):
            # Convert underscore to hyphen (cargo uses hyphens for package names)
            package = line.split()[-1].replace("_", "-")
            current_binary = "doctest:" + package
        # Match: "test_name: test"
        elif line.endswith(": test"):
            test_name = line.replace(": test", "").strip()
            if current_binary:
                tests.append((current_binary, test_name))

    return tests


def list_rust_tests() -> list[tuple[str, str]]:
    """List all Rust tests in the workspace, returning (binary_path, test_name) tuples."""
    result = subprocess.run(
        ["cargo", "test", "--workspace", "--", "--list"],
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
    )
    if result.returncode != 0:
        raise RuntimeError(f"cargo test --list failed:\n{result.stdout.decode()}")
    return parse_cargo_test_output(result.stdout.decode())


def pytest_generate_tests(metafunc):
    if "rust_test" in metafunc.fixturenames:
        # Skip Rust tests when running against a pre-built binary or docker image
        binary = metafunc.config.getoption("--opensovd-binary", default=None)
        docker = metafunc.config.getoption("--opensovd-docker", default=None)
        if binary or docker:
            # Don't parametrize - test will be skipped via marker below
            return
        tests = list_rust_tests()
        metafunc.parametrize("rust_test", tests, ids=[t[1] for t in tests])


@pytest.fixture
def rust_test(request):
    """Fixture that skips when no parametrization (external binary/docker mode)."""
    if not hasattr(request, "param"):
        pytest.skip("Rust tests skipped when using --opensovd-binary or --opensovd-docker")
    return request.param


def test_rust(rust_test):
    """Execute a single Rust unit or doc test."""
    binary_path, test_name = rust_test

    if binary_path.startswith("doctest:"):
        # Run doctest via cargo
        package = binary_path.replace("doctest:", "")
        result = subprocess.run(
            ["cargo", "test", "-p", package, "--doc", "--", test_name, "--exact"],
            capture_output=True,
        )
    else:
        # Run binary test directly
        result = subprocess.run(
            [binary_path, test_name, "--exact"],
            capture_output=True,
        )

    assert result.returncode == 0, (
        f"Test {test_name} failed:\n{result.stdout.decode()}\n{result.stderr.decode()}"
    )
