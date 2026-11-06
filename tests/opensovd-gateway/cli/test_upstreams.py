# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

"""Tests for a repeated --upstream-url: each upstream serves its own entities."""

import json
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from typing import ClassVar

import pytest
from fixtures import default_binary_args


def _mock_upstream(name: str, ecu: str):
    """A mock upstream with one component, below /api."""

    class Handler(BaseHTTPRequestHandler):
        seen: ClassVar[list[tuple[str, str]]] = []

        def _answer(self):
            length = int(self.headers.get("content-length") or 0)
            self.rfile.read(length)
            type(self).seen.append((self.command, self.path))
            host = self.headers["host"]
            base = f"http://{host}/api/components/{ecu}"
            if self.path == "/api/components":
                status, body = 200, {"items": [{"id": ecu, "name": name, "href": base}]}
            elif self.command == "POST" and self.path == "/api/locks":
                status, body = 201, {"id": f"{name}-lock", "owned": True}
            elif self.path == "/api/locks":
                status, body = 200, {"items": []}
            elif self.path == f"/api/locks/{name}-lock":
                expiration = {"first": "2030-01-01T00:00:10Z", "second": "2030-01-01T00:00:05Z"}
                status, body = 200, {"owned": True, "lock_expiration": expiration[name]}
            elif self.path.startswith(f"/api/components/{ecu}"):
                status, body = 200, {"backend": name, "self": base}
            else:
                status, body = 404, {"message": f"Not found: {self.path}"}
            payload = json.dumps(body).encode()
            self.send_response(status)
            self.send_header("content-type", "application/json")
            self.send_header("content-length", str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)

        do_GET = do_POST = do_PUT = do_DELETE = _answer

        def log_message(self, format, *args):
            pass

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    return server


@pytest.fixture(scope="module")
def upstreams():
    servers = _mock_upstream("first", "flxc1000"), _mock_upstream("second", "hovr4000")
    yield servers
    for server in servers:
        server.shutdown()
        server.server_close()


@pytest.fixture(scope="module")
def binary_args(request, upstreams):
    """Run the gateway with the mock topology and two upstreams."""
    urls = [f"http://127.0.0.1:{server.server_address[1]}/api" for server in upstreams]
    return default_binary_args(
        request.config,
        "--mock",
        "--upstream-url",
        urls[0],
        "--upstream-url",
        urls[1],
    )


@pytest.fixture(autouse=True)
def _clear_seen(upstreams):
    for server in upstreams:
        server.RequestHandlerClass.seen.clear()


def test_the_list_holds_the_entities_of_both(client):
    items = client.get("/v1/components").json()["items"]
    assert [item["id"] for item in items] == ["ecu", "gateway", "flxc1000", "hovr4000"]


def test_a_write_reaches_the_owner_of_the_entity_only(client, upstreams):
    first, second = (server.RequestHandlerClass.seen for server in upstreams)
    url = f"{client.base_url}/v1/components/hovr4000/modes/session"

    response = client.http.put(url, content=b"{}")

    assert response.status_code == 200
    assert response.json()["backend"] == "second"
    assert response.json()["self"] == f"{client.base_url}/v1/components/hovr4000"
    assert not any(method == "PUT" for method, _ in first)
    assert ("PUT", "/api/components/hovr4000/modes/session") in second


def test_an_unknown_entity_is_the_gateways_404(client):
    response = client.get("/v1/components/nowhere")
    assert response.status_code == 404
    assert response.json()["vendor_code"] == "entity-not-found"


def test_a_vehicle_lock_holds_on_all_upstreams(client, upstreams):
    for upstream in upstreams:
        upstream.RequestHandlerClass.seen.clear()
    locks = f"{client.base_url}/v1/locks"

    created = client.http.post(locks, json={"lock_expiration": 60})

    assert created.status_code == 201
    lock = created.json()["id"]
    assert lock not in ("first-lock", "second-lock")
    assert upstreams[0].RequestHandlerClass.seen == [("POST", "/api/locks")]
    assert upstreams[1].RequestHandlerClass.seen == [("POST", "/api/locks")]

    assert [item["id"] for item in client.http.get(locks).json()["items"]] == [lock]
    details = client.http.get(f"{locks}/{lock}").json()
    assert details["lock_expiration"] == "2030-01-01T00:00:05Z"
    assert details["owned"] is True

    assert client.http.delete(f"{locks}/{lock}").status_code == 200
    assert client.http.get(locks).json()["items"] == []
    assert ("DELETE", "/api/locks/first-lock") in upstreams[0].RequestHandlerClass.seen
    assert ("DELETE", "/api/locks/second-lock") in upstreams[1].RequestHandlerClass.seen


def test_authorize_stays_with_the_gateway_by_default(client, upstreams):
    response = client.http.post(f"{client.base_url}/v1/authorize", content=b"{}")
    assert response.status_code == 404
    assert upstreams[0].RequestHandlerClass.seen == []
    assert upstreams[1].RequestHandlerClass.seen == []
