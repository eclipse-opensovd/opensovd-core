# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

"""Self-tests for the opensovd_e2e plugin, driven via pytest's pytester."""

# The plugin is loaded into each inner run explicitly (-p), since it is not
# pip-installed in this dev checkout (no pytest11 auto-discovery here).
PLUGIN = ("-p", "opensovd_e2e.plugin")


def test_registers_options(pytester):
    """The plugin contributes its --opensovd-* options."""
    result = pytester.runpytest(*PLUGIN, "--help")
    result.stdout.fnmatch_lines(["*--opensovd-run*"])


def test_registers_req_marker(pytester):
    """The req traceability marker is self-registered (no unknown-mark warning)."""
    result = pytester.runpytest(*PLUGIN, "--markers")
    result.stdout.fnmatch_lines(["*@pytest.mark.req*"])


def test_process_fixture_runs_command(pytester):
    """The generic `process` fixture spawns --opensovd-run and captures output."""
    pytester.makepyfile(
        """
        import pytest

        @pytest.fixture(scope="module")
        def binary_args():
            return ["hello"]

        def test_echo(process):
            process.wait_for("hello", timeout_seconds=5.0)
            process.process.wait(timeout=5.0)
            assert process.process.returncode == 0
        """
    )
    result = pytester.runpytest(*PLUGIN, "--opensovd-run=echo")
    result.assert_outcomes(passed=1)
