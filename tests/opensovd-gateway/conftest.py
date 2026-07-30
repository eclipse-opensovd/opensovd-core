# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

"""Gateway-specific pytest fixtures (HTTP client + TLS context)."""

from typing import Self

import httpx
import pytest
from fixtures import LISTENING_PATTERN, listening_url
from opensovd_e2e import ProcessUnderTest, spawn_process


class SovdClient:
    """HTTP client for the SOVD gateway under test.

    Interprets `(addr, transport, base)` groups from the listening match into
    an httpx.Client with the right scheme. Holds a backreference to the
    ProcessUnderTest so tests can reach process-level concerns via .gateway.
    """

    def __init__(
        self,
        gateway: ProcessUnderTest,
        *,
        addr: str,
        transport: str,
        base_url: str,
        http: httpx.Client,
    ):
        self.gateway = gateway
        self.addr = addr
        self.transport = transport
        self.base_url = base_url
        self.http = http

    @classmethod
    def from_process(cls, gateway: ProcessUnderTest, *, ssl_context=None) -> Self:
        """Build a SovdClient by interpreting `gateway.match` groups.

        Args:
            gateway: ProcessUnderTest with `.match` populated by ready_banner.
            ssl_context: ssl.SSLContext for the tls transport. None falls
                through to httpx's default `verify=True` (system CAs); a
                context overrides verification (and supplies client certs
                for mTLS).

        Raises:
            RuntimeError: if `gateway.match` is None.
        """
        if gateway.match is None:
            raise RuntimeError("ready_banner did not match; cannot build SovdClient")
        addr = gateway.match.group(1)
        transport = gateway.match.group(2)
        if ssl_context is not None and transport != "tls":
            raise ValueError(
                f"ssl_context provided but transport is {transport!r}; "
                "ssl_context only applies to tls"
            )
        base_url = listening_url(gateway.match)
        match transport:
            case "tcp":
                http = httpx.Client(base_url=base_url)
            case "tls":
                http = httpx.Client(
                    base_url=base_url,
                    verify=ssl_context if ssl_context is not None else True,
                )
            case "abstract":
                http = httpx.Client(
                    base_url=base_url,
                    transport=httpx.HTTPTransport(uds="\0" + addr),
                )
            case "unix":
                http = httpx.Client(
                    base_url=base_url,
                    transport=httpx.HTTPTransport(uds=addr),
                )
            case _:
                raise ValueError(f"unknown transport: {transport!r}")
        return cls(gateway, addr=addr, transport=transport, base_url=base_url, http=http)

    def get(self, path: str, **kwargs) -> httpx.Response:
        return self.http.get(path, **kwargs)

    def post(self, path: str, **kwargs) -> httpx.Response:
        return self.http.post(path, **kwargs)

    def put(self, path: str, **kwargs) -> httpx.Response:
        return self.http.put(path, **kwargs)

    def delete(self, path: str, **kwargs) -> httpx.Response:
        return self.http.delete(path, **kwargs)

    def close(self):
        self.http.close()

    def __enter__(self):
        return self

    def __exit__(self, exc_type, exc_val, exc_tb):
        self.close()


@pytest.fixture(scope="module")
def ready_banner():
    return LISTENING_PATTERN


@pytest.fixture(scope="module")
def gateway(
    request,
    crate_binary,
    binary_args,
    ready_banner,
):
    proc = spawn_process(
        request.config,
        binary_args,
        ready_banner,
        crate=crate_binary,
    )
    yield proc
    proc.close()


@pytest.fixture(scope="module")
def gateway_ssl_context():
    """SSL context for the gateway's httpx client.

    Override in TLS/mTLS test modules to return an ssl.SSLContext configured
    with the appropriate CA and (for mTLS) client certificate.
    """
    return None


@pytest.fixture(scope="module")
def client(gateway, gateway_ssl_context):
    if gateway.match is None:
        pytest.skip("ready_banner did not match; no SovdClient available")
    c = SovdClient.from_process(gateway, ssl_context=gateway_ssl_context)
    yield c
    c.close()
