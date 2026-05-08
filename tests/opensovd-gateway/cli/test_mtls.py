# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

"""Integration tests for mutual TLS (mTLS) transport.

The gateway requires a client certificate signed by the configured CA.
See test_tls.py for plain (server-only) TLS tests.
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
    )


@pytest.fixture(scope="module")
def gateway_ssl_context(tls_certs):
    ctx = ssl.create_default_context(cafile=str(tls_certs["ca_crt"]))
    ctx.load_cert_chain(
        certfile=str(tls_certs["client_crt"]),
        keyfile=str(tls_certs["client_key"]),
    )
    return ctx


def test_mtls_transport(client):
    """mTLS: gateway reports tls transport type."""
    assert client.transport == "tls"


def test_mtls_valid_client_cert(client):
    """mTLS: client presents a valid cert — request should succeed."""
    response = client.get("/version-info")
    assert response.status_code == 200
    data = response.json()
    assert "sovd_info" in data


def test_mtls_rejects_missing_client_cert(client, tls_certs):
    """mTLS: client sends no cert — TLS handshake must be rejected by the server."""
    ca_path = tls_certs["ca_crt"]
    ssl_ctx = ssl.create_default_context(cafile=str(ca_path))
    no_cert = httpx.Client(base_url=client.base_url, verify=ssl_ctx)
    with pytest.raises((httpx.ConnectError, httpx.ReadError)):
        no_cert.get("/version-info")
    no_cert.close()
