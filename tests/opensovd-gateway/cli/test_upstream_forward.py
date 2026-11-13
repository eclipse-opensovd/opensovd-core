# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

"""Tests for --upstream-url: what the gateway does not serve goes to a mock CDA."""

import json
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from typing import ClassVar

import pytest
from fixtures import default_binary_args

CDA_BINARY_BODY = b"\x00\x01not json"


# What the CDA answers for discovery: links to itself, spelled like the CDA does.
_CDA_JSON = {
    "/vehicle/v15/components": {
        "items": [
            {
                "id": "flxc1000",
                "name": "flxc1000",
                "href": "http://localhost:20002/Vehicle/v15/components/flxc1000",
            }
        ]
    },
    # What the CDA serves at the top level; it has no root yet, see
    # eclipse-opensovd/classic-diagnostic-adapter#684.
    "/vehicle/v15": {
        "id": "",
        "name": "",
        "components": "http://localhost:20002/vehicle/v15/components",
        "apps": "http://localhost:20002/vehicle/v15/apps",
        "functions": "http://localhost:20002/vehicle/v15/functions",
        "locks": "http://localhost:20002/vehicle/v15/locks",
        "data": "http://localhost:20002/vehicle/v15/data",
    },
    "/vehicle/v15/locks": {"id": "cda-lock", "owned": True},
    "/vehicle/v15/components/flxc1000": {
        "id": "flxc1000",
        "name": "flxc1000",
        "faults": "http://localhost:20002/vehicle/v15/components/flxc1000/faults",
    },
}


class _MockCda(BaseHTTPRequestHandler):
    """Records each request and answers 202 with a binary body and a Location."""

    seen: ClassVar[list[tuple[str, str, bytes]]] = []

    def _answer(self):
        length = int(self.headers.get("content-length") or 0)
        body = self.rfile.read(length) if length else b""
        type(self).seen.append((self.command, self.path, body))
        if self.path in _CDA_JSON:
            payload = json.dumps(_CDA_JSON[self.path]).encode()
            self.send_response(200)
            self.send_header("content-type", "application/json")
            self.send_header("content-length", str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)
            return
        self.send_response(202)
        self.send_header("content-type", "application/octet-stream")
        self.send_header("location", "http://cda:20002/vehicle/v15/x/1")
        self.send_header("content-length", str(len(CDA_BINARY_BODY)))
        self.end_headers()
        self.wfile.write(CDA_BINARY_BODY)

    do_GET = do_POST = do_PUT = do_DELETE = _answer

    def log_message(self, format, *args):
        pass


@pytest.fixture(scope="module")
def mock_cda():
    """A mock CDA on an ephemeral port, serving its API below /vehicle/v15."""
    server = ThreadingHTTPServer(("127.0.0.1", 0), _MockCda)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    yield server
    server.shutdown()
    server.server_close()


@pytest.fixture(scope="module")
def binary_args(request, mock_cda):
    """Run the gateway with the mock topology, forwarding to the mock CDA."""
    host, port = mock_cda.server_address[:2]
    url = f"http://{host}:{port}/vehicle/v15"
    return default_binary_args(
        request.config,
        "--mock",
        "--upstream-url",
        url,
        "--upstream-authorize-url",
        url,
    )


@pytest.fixture(autouse=True)
def _clear_seen():
    _MockCda.seen.clear()


def test_held_component_is_served_by_the_gateway(client):
    response = client.get("/v1/components/ecu")
    assert response.status_code == 200
    assert response.json()["id"] == "ecu"
    assert _MockCda.seen == []


@pytest.mark.parametrize(
    ("method", "path", "cda_path"),
    [
        (
            "GET",
            "/v1/components/flxc1000/faults?status=x",
            "/vehicle/v15/components/flxc1000/faults?status=x",
        ),
        (
            "PUT",
            "/v1/components/flxc1000/genericservice",
            "/vehicle/v15/components/flxc1000/genericservice",
        ),
        ("POST", "/v1/authorize", "/vehicle/v15/authorize"),
        ("GET", "/v1/functions/functionalgroups/fg", "/vehicle/v15/functions/functionalgroups/fg"),
        ("GET", "/v1/data/version", "/vehicle/v15/data/version"),
        (
            "GET",
            "/v1/components/flxc1000/x-single-ecu-jobs",
            "/vehicle/v15/components/flxc1000/x-single-ecu-jobs",
        ),
    ],
)
def test_unserved_requests_reach_the_cda_unchanged(client, method, path, cda_path):
    response = client.http.request(method, client.base_url + path, content=b"\x22\x10")
    assert response.status_code == 202
    assert response.headers["location"] == f"{client.base_url}/v1/x/1"
    assert response.headers["content-type"] == "application/octet-stream"
    assert response.content == CDA_BINARY_BODY
    assert _MockCda.seen == [(method, cda_path, b"\x22\x10")]


def test_paths_below_a_held_entity_stay_with_the_gateway(client):
    response = client.http.get(client.base_url + "/v1/components/ecu/x-single-ecu-jobs")
    assert response.status_code == 404
    assert _MockCda.seen == []


def test_a_vehicle_lock_goes_through_the_gateway(client):
    response = client.http.post(f"{client.base_url}/v1/locks", json={"lock_expiration": 60})
    assert response.status_code == 200
    assert response.json()["id"] != "cda-lock"
    assert _MockCda.seen == [("POST", "/vehicle/v15/locks", b'{"lock_expiration":60}')]


def test_the_api_root_combines_the_gateway_and_the_cda(client):
    response = client.get("/v1")
    assert response.status_code == 200
    capabilities = response.json()
    assert capabilities["id"] == ""
    for name in ("components", "apps", "functions", "locks", "data"):
        assert capabilities[name] == f"{client.base_url}/v1/{name}"
    assert _MockCda.seen == [("GET", "/vehicle/v15", b"")]


def test_discovery_walks_from_the_list_to_the_cda_through_the_gateway(client):
    listed = client.get("/v1/components").json()["items"]
    assert [item["id"] for item in listed] == ["ecu", "gateway", "flxc1000"]

    href = listed[-1]["href"]
    assert href == f"{client.base_url}/v1/components/flxc1000"
    capabilities = client.http.get(href).json()

    assert capabilities["faults"] == f"{client.base_url}/v1/components/flxc1000/faults"
