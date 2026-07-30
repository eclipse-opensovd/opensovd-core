# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

"""opensovd-mcp pytest fixtures.

opensovd-mcp speaks JSON-RPC over stdio and does not bind to a port, so the
ready-banner plumbing of the generic harness is disabled.
"""

import pytest
from opensovd_e2e import spawn_process


@pytest.fixture(scope="module")
def crate_binary() -> str:
    return "opensovd-mcp"


@pytest.fixture(scope="module")
def mcp(request, crate_binary, binary_args):
    proc = spawn_process(
        request.config,
        binary_args,
        crate=crate_binary,
    )
    yield proc
    proc.close()
