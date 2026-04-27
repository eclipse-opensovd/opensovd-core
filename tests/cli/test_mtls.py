# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

"""Integration tests for mutual TLS (mTLS) transport.

The gateway requires a client certificate signed by the configured CA.
See test_tls.py for plain (server-only) TLS tests.
"""

import ssl

import httpx
import pytest
from fixtures import default_gateway_args


@pytest.fixture(scope="module")
def gateway_extra_features():
    return ["tls"]


@pytest.fixture(scope="module")
def gateway_args(request, tls_certs):
    ca_path, srv_cert, srv_key, _cli_cert, _cli_key = (
        tls_certs["ca_crt"],
        tls_certs["server_crt"],
        tls_certs["server_key"],
        tls_certs["client_crt"],
        tls_certs["client_key"],
    )
    return default_gateway_args(
        request.config,
        "--tls-cert",
        str(srv_cert),
        "--tls-key",
        str(srv_key),
        "--tls-client-ca",
        str(ca_path),
    )


@pytest.fixture(scope="module")
def gateway_ssl_context(tls_certs):
    ca_path, _srv_cert, _srv_key, cli_cert, cli_key = (
        tls_certs["ca_crt"],
        tls_certs["server_crt"],
        tls_certs["server_key"],
        tls_certs["client_crt"],
        tls_certs["client_key"],
    )
    ctx = ssl.create_default_context(cafile=str(ca_path))
    ctx.load_cert_chain(certfile=str(cli_cert), keyfile=str(cli_key))
    return ctx


def test_mtls_transport(gateway):
    """mTLS: gateway reports mtls transport type."""
    assert gateway.transport == "mtls"
    assert gateway.addr.startswith("127.0.0.1:")


def test_mtls_valid_client_cert(gateway):
    """mTLS: client presents a valid cert — request should succeed."""
    response = gateway.get("/version-info")
    assert response.status_code == 200
    data = response.json()
    assert "sovd_info" in data


def test_mtls_rejects_missing_client_cert(gateway, tls_certs):
    """mTLS: client sends no cert — TLS handshake must be rejected by the server."""
    ca_path = tls_certs["ca_crt"]
    ssl_ctx = ssl.create_default_context(cafile=str(ca_path))
    client = httpx.Client(base_url=gateway.base_url, verify=ssl_ctx)
    with pytest.raises((httpx.ConnectError, httpx.ReadError)):
        client.get("/version-info")
    client.close()
