# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

"""Tests for scripts/dev-token.sh: a token for a stack without an identity provider."""

import subprocess
from pathlib import Path

import pytest
from fixtures import default_binary_args

SCRIPT = Path(__file__).parents[3] / "scripts" / "dev-token.sh"
ISSUER = "dev-stack"


def dev_token(directory, subject="tester"):
    run = subprocess.run(
        [SCRIPT, directory, ISSUER, subject], capture_output=True, text=True, check=True
    )
    return run.stdout.strip()


@pytest.fixture(scope="module")
def keys(tmp_path_factory):
    directory = tmp_path_factory.mktemp("keys")
    dev_token(directory)
    return directory


@pytest.fixture(scope="module")
def binary_args(request, keys):
    """Run the gateway with the key set of the script."""
    return default_binary_args(
        request.config,
        "--mock",
        "--auth-jwks",
        str(keys / "dev-jwks.json"),
        "--auth-issuer",
        ISSUER,
    )


def test_the_token_passes_the_gateway(client, keys):
    headers = {"Authorization": f"Bearer {dev_token(keys)}"}
    assert client.get("/v1/components", headers=headers).status_code == 200


def test_a_request_without_a_token_is_refused(client):
    assert client.get("/v1/components").status_code == 401
