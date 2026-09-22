# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

"""Integration tests for optional client authentication.

The gateway verifies a client certificate when one is presented but also
accepts anonymous clients.  See test_mtls.py for the required mode.
"""

import ssl

import httpx
import pytest
from fixtures import default_binary_args


@pytest.fixture(scope="module")
def binary_args(request, tls_certs):
    return default_binary_args(
        request.config,
        "--tls-cert",
        str(tls_certs["server_crt"]),
        "--tls-key",
        str(tls_certs["server_key"]),
        "--tls-client-ca",
        str(tls_certs["ca_crt"]),
        "--tls-client-auth",
        "optional",
    )


@pytest.fixture(scope="module")
def gateway_ssl_context(tls_certs):
    ctx = ssl.create_default_context(cafile=str(tls_certs["ca_crt"]))
    ctx.load_cert_chain(
        certfile=str(tls_certs["client_crt"]),
        keyfile=str(tls_certs["client_key"]),
    )
    return ctx


def test_optional_accepts_client_cert(client):
    response = client.get("/version-info")
    assert response.status_code == 200


def test_optional_accepts_missing_client_cert(client, tls_certs):
    ssl_ctx = ssl.create_default_context(cafile=str(tls_certs["ca_crt"]))
    with httpx.Client(base_url=client.base_url, verify=ssl_ctx) as anonymous:
        response = anonymous.get("/version-info")
    assert response.status_code == 200


def test_optional_rejects_untrusted_client_cert(client, tls_certs, untrusted_tls_certs):
    ssl_ctx = ssl.create_default_context(cafile=str(tls_certs["ca_crt"]))
    ssl_ctx.load_cert_chain(
        certfile=str(untrusted_tls_certs["client_crt"]),
        keyfile=str(untrusted_tls_certs["client_key"]),
    )
    with (
        httpx.Client(base_url=client.base_url, verify=ssl_ctx) as untrusted,
        pytest.raises((httpx.ConnectError, httpx.ReadError, httpx.RemoteProtocolError)),
    ):
        untrusted.get("/version-info")
