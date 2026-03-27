# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

"""Shared fixtures for CLI integration tests."""

import subprocess
from pathlib import Path

import pytest

_MKCERTS = Path(__file__).parent.parent.parent / "scripts" / "mkcerts.sh"


@pytest.fixture(scope="session")
def tls_certs(tmp_path_factory):
    """Generate CA, server, and client certs once per session via scripts/mkcerts.sh."""
    tmp = tmp_path_factory.mktemp("tls_certs")
    subprocess.run(["bash", str(_MKCERTS), str(tmp)], check=True, capture_output=True)
    return {
        "ca_crt": tmp / "ca.crt",
        "server_crt": tmp / "server.crt",
        "server_key": tmp / "server.key",
        "client_crt": tmp / "client.crt",
        "client_key": tmp / "client.key",
    }
