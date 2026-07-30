# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

"""Tests for signal handling (graceful shutdown)."""

import signal
import sys

import pytest
from fixtures import LISTENING_PATTERN, default_binary_args
from opensovd_e2e import spawn_process

pytestmark = pytest.mark.skipif(
    sys.platform == "win32", reason="Unix signals not supported on Windows"
)

# Signals to test for graceful shutdown - SIGINT/SIGTERM are Unix-only
SHUTDOWN_SIGNALS = [(signal.SIGINT, "SIGINT"), (signal.SIGTERM, "SIGTERM")]


@pytest.fixture
def gateway(request, crate_binary):
    """Function-scoped gateway for signal testing (will be terminated)."""
    proc = spawn_process(
        request.config, default_binary_args(request.config), LISTENING_PATTERN, crate=crate_binary
    )
    yield proc
    proc.close()


@pytest.mark.parametrize(("sig", "name"), SHUTDOWN_SIGNALS)
def test_graceful_shutdown(gateway, sig, name):
    """Test that the gateway shuts down gracefully on signals."""
    # Send signal to the gateway process
    gateway.process.send_signal(sig)

    # Wait for shutdown log with signal name
    gateway.wait_for(f"Shutdown signal signal={name}", timeout_seconds=5.0)

    # Wait for process to exit
    gateway.process.wait(timeout=5.0)

    # Verify clean exit
    assert gateway.process.returncode == 0
