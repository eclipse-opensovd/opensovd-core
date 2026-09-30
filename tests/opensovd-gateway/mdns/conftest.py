# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

"""Fixtures for mDNS tests inside a private network namespace."""

import json
import re
import sys
import time
from collections.abc import Callable
from pathlib import Path

import pytest
from fixtures import LISTENING_PATTERN
from opensovd_e2e import NetworkNamespace, ProcessUnderTest, spawn_process

CLIENT = Path(__file__).with_name("mdns_client.py")
READY = re.compile(r'^\{"event": "(browsing|registered)"')
EVENT = re.compile(r"^\{")


class MdnsClient(ProcessUnderTest):
    """A running mdns_client.py command inside the network namespace."""

    def event(self, where: Callable[[dict], bool], timeout: float = 10.0) -> dict:
        """Wait for the next event that `where` accepts."""
        deadline = time.monotonic() + timeout
        while True:
            line = self.wait_for(EVENT, max(deadline - time.monotonic(), 0.0)).string
            if where(event := json.loads(line)):
                return event


class Mdns:
    """Runs mdns_client.py commands inside the network namespace."""

    def __init__(self, netns: NetworkNamespace):
        self._netns = netns
        self._clients: list[MdnsClient] = []

    def browse(self) -> MdnsClient:
        return self._start("browse")

    def register(self, instance: str, host: str, address: str) -> MdnsClient:
        return self._start("register", instance, host, address)

    def fetch(self, instance: str, path: str) -> dict:
        out = self._netns.run(sys.executable, str(CLIENT), "fetch", instance, path).stdout
        return json.loads(out.splitlines()[-1])

    def close(self) -> None:
        for client in self._clients:
            client.close()

    def _start(self, *args: str) -> MdnsClient:
        cmd = [*self._netns.wrap, sys.executable, str(CLIENT), *args]
        client = MdnsClient.spawn(cmd, ready_banner=READY)
        self._clients.append(client)
        return client


@pytest.fixture(autouse=True)
def sovd0(netns):
    """One multicast interface, sovd0 at 192.0.2.1."""
    netns.add_interface("sovd0", "192.0.2.1/24")


@pytest.fixture
def start_gateway(request, crate_binary, netns):
    """Start the gateway inside the network namespace; it is stopped after the test."""
    started: list[ProcessUnderTest] = []

    def start(*args: str) -> ProcessUnderTest:
        proc = spawn_process(
            request.config, list(args), LISTENING_PATTERN, crate=crate_binary, wrap=netns.wrap
        )
        started.append(proc)
        return proc

    yield start
    for proc in started:
        proc.close()


@pytest.fixture
def mdns(netns):
    """An independent mDNS client (python-zeroconf) inside the network namespace."""
    client = Mdns(netns)
    yield client
    client.close()
