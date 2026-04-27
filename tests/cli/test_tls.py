# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

"""Integration tests for plain TLS transport.

The gateway presents a server certificate; the client verifies it but does not
need to present one.  See test_mtls.py for mutual-TLS tests.
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
    _ca, srv_cert, srv_key, _cli_cert, _cli_key = (
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
    )


@pytest.fixture(scope="module")
def gateway_ssl_context(tls_certs):
    ca_path, _, _, _, _ = (
        tls_certs["ca_crt"],
        tls_certs["server_crt"],
        tls_certs["server_key"],
        tls_certs["client_crt"],
        tls_certs["client_key"],
    )
    return ssl.create_default_context(cafile=str(ca_path))


def test_tls_transport(gateway):
    """Plain TLS: client authenticates the server cert, no client cert needed."""
    assert gateway.transport == "tls"
    assert gateway.addr.startswith("127.0.0.1:")

    response = gateway.get("/version-info")
    assert response.status_code == 200
    data = response.json()
    assert "sovd_info" in data


def test_tls_rejects_untrusted_ca(gateway):
    """Client using system CA store cannot verify the self-signed server cert."""
    # verify=True (system CAs) — cannot verify self-signed cert
    with httpx.Client(base_url=gateway.base_url) as client, pytest.raises(httpx.ConnectError):
        client.get("/version-info")
