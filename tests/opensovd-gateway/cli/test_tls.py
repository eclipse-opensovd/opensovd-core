# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

"""Integration tests for plain TLS transport.

The gateway presents a server certificate; the client verifies it but does not
need to present one.  See test_mtls.py for mutual-TLS tests.
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
    )


@pytest.fixture(scope="module")
def gateway_ssl_context(tls_certs):
    return ssl.create_default_context(cafile=str(tls_certs["ca_crt"]))


def test_tls_transport(client):
    """Plain TLS: client authenticates the server cert, no client cert needed."""
    assert client.transport == "tls"

    response = client.get("/version-info")
    assert response.status_code == 200
    data = response.json()
    assert "sovd_info" in data


def test_tls_rejects_untrusted_ca(client):
    """Client using system CA store cannot verify the self-signed server cert."""
    # verify=True (system CAs) -- cannot verify self-signed cert
    with httpx.Client(base_url=client.base_url) as untrusted, pytest.raises(httpx.ConnectError):
        untrusted.get("/version-info")
