# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

"""OpenSOVD-core test helpers: server startup-banner pattern, URL mapping, --url args."""

import re
import shlex

import pytest

# The OpenSOVD server announces its bound address on stdout in this form:
#   Listening addr=127.0.0.1:7690 type=tcp base=/sovd
LISTENING_PATTERN = re.compile(r"Listening addr=(\S+) type=(tcp|unix|abstract|tls) base=(\S+)")


def listening_url(match: re.Match) -> str:
    """Build a base URL string from a LISTENING_PATTERN match.

    The match is expected to capture (addr, transport, base) in groups 1-3.
    Useful for consumers (e.g. Bruno) that only need the URL and not a full
    SovdClient with an httpx connection pool.
    """
    addr, transport, base = match.group(1), match.group(2), match.group(3)
    if transport == "tcp":
        return f"http://{addr}{base}"
    if transport == "tls":
        return f"https://{addr}{base}"
    return f"http://localhost{base}"


def default_binary_args(config: pytest.Config, *extra: str) -> list[str]:
    """Build binary args: ephemeral-port URL plus extras and any --opensovd-args.

    Skips the auto-injected --url if the caller (or --opensovd-args) already
    supplied one. Detects both `--url X` and `--url=X` forms.
    """
    extra_args = shlex.split(config.getoption("--opensovd-args"))
    has_url = any(a == "--url" or a.startswith("--url=") for a in (*extra, *extra_args))
    if has_url:
        return [*extra, *extra_args]
    return ["--url", "http://127.0.0.1:0/sovd", *extra, *extra_args]
