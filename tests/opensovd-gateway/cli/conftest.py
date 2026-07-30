# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

"""Shared fixtures for CLI integration tests."""

import shutil
import subprocess
from pathlib import Path

import pytest

_MKCERTS = Path(__file__).parents[3] / "scripts" / "mkcerts.sh"


@pytest.fixture(scope="session")
def tls_certs(tmp_path_factory):
    """Generate CA, server, and client certs once per session via scripts/mkcerts.sh."""
    tmp = tmp_path_factory.mktemp("tls_certs")
    bash = shutil.which("bash")
    if bash is None:
        pytest.fail("bash not found on PATH -- required to run scripts/mkcerts.sh")
    cmd = [bash, _MKCERTS.as_posix(), tmp.as_posix()]
    result = subprocess.run(
        cmd,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
    )
    if result.returncode != 0:
        pytest.fail(
            f"mkcerts.sh failed (exit {result.returncode}): {' '.join(cmd)}\n"
            f"{result.stdout}\n{result.stderr}"
        )
    return {
        "ca_crt": tmp / "ca.crt",
        "server_crt": tmp / "server.crt",
        "server_key": tmp / "server.key",
        "client_crt": tmp / "client.crt",
        "client_key": tmp / "client.key",
    }
