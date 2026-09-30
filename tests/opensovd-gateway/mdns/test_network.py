# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

"""The mDNS announcement as another mDNS stack sees it, in a private network namespace."""

import signal
import sys

import pytest

pytestmark = pytest.mark.skipif(sys.platform != "linux", reason="network namespaces need Linux")

ID = "OPENSOVDTEST0001"
INSTANCE = ID.lower()
SERVICE = "_sovd._tcp.local."
FULLNAME = f"{INSTANCE}.{SERVICE}"
URL = ("--url", "http://0.0.0.0:0/sovd")


def port_of(gateway) -> int:
    return int(gateway.match.group(1).rsplit(":", 1)[1])


def line_with(gateway, text: str) -> str:
    return next(line for line in gateway.stdout.splitlines() if text in line)


def announced(interface: str, addrs: str) -> str:
    return f'Announced instance="{INSTANCE}" host={ID}.local interface={interface} addrs={addrs} '


def added(name: str):
    return lambda event: event["event"] == "added" and event["name"] == name


def test_is_discovered(start_gateway, mdns):
    browser = mdns.browse()
    gateway = start_gateway(*URL, "--mdns", ID)
    port = port_of(gateway)

    service = browser.event(added(FULLNAME))
    assert service["server"] == f"{ID}.local."
    assert service["port"] == port
    assert service["addresses"] == ["192.0.2.1"]
    assert service["txt"] == {
        "identification": ID,
        "accessurl": f"http://{ID}.local:{port}/sovd",
    }


def test_is_reachable_through_the_accessurl(start_gateway, mdns):
    gateway = start_gateway(*URL, "--mdns", ID)
    gateway.wait_for(announced("sovd0", "192.0.2.1"), 10.0)

    fetched = mdns.fetch(INSTANCE, "/version-info")
    assert fetched["address"] == "192.0.2.1"
    assert fetched["status"] == 200


def test_logs_the_lifecycle(start_gateway):
    gateway = start_gateway(*URL, "--mdns", ID)
    port = port_of(gateway)

    registering = line_with(gateway, "Registering")
    for field in (
        f'instance="{INSTANCE}"',
        f"service={SERVICE}",
        f"host={ID}.local ",
        f"port={port}",
        "interfaces=all",
        f'identification="{ID}"',
    ):
        assert field in registering
    line = gateway.wait_for(announced("sovd0", "192.0.2.1"), 10.0).string
    assert f"accessurl=http://{ID}.local:{port}/sovd" in line

    gateway.process.send_signal(signal.SIGTERM)
    gateway.wait_for(f'Withdrawn instance="{INSTANCE}" host={ID}.local', 5.0)
    assert gateway.process.wait(timeout=5.0) == 0


def test_withdraws_on_shutdown(start_gateway, mdns):
    browser = mdns.browse()
    gateway = start_gateway(*URL, "--mdns", ID)
    browser.event(added(FULLNAME))

    gateway.process.send_signal(signal.SIGTERM)
    browser.event(lambda e: e["event"] == "removed" and e["name"] == FULLNAME, 5.0)
    assert gateway.process.wait(timeout=5.0) == 0


def test_renames_a_taken_instance(start_gateway, mdns):
    browser = mdns.browse()
    mdns.register(INSTANCE, "other", "192.0.2.9")
    gateway = start_gateway(*URL, "--mdns", ID)

    renamed = f"{INSTANCE} (2)"
    gateway.wait_for(
        f'Instance renamed after conflict from="{INSTANCE}" to="{renamed}" interface=sovd0', 10.0
    )
    service = browser.event(added(f"{renamed}.{SERVICE}"))
    assert service["txt"]["identification"] == ID

    gateway.process.send_signal(signal.SIGTERM)
    gateway.wait_for(f'Withdrawn instance="{renamed}" host={ID}.local', 5.0)


@pytest.mark.parametrize("host", ["cabin", "cabin".ljust(58, "0")])
def test_renames_a_taken_host(start_gateway, mdns, netns, host):
    netns.ip("addr", "add", "192.0.2.2/24", "dev", "sovd0")
    browser = mdns.browse()
    owner = start_gateway(*URL, "--mdns", "OWNER", "--mdns-host", host)
    owner.wait_for(f'Announced instance="{host}" host={host}.local ', 10.0)

    second = start_gateway(
        "--url", "http://192.0.2.2:0/sovd", "--mdns", "SECOND", "--mdns-host", host
    )
    second.wait_for(
        f"Host renamed after conflict from={host}.local to={host}-2.local interface=sovd0", 10.0
    )
    accessurl = f"http://{host}-2.local:{port_of(second)}/sovd"
    service = browser.event(
        lambda e: e["event"] != "removed" and e["txt"].get("accessurl") == accessurl
    )
    assert service["server"] == f"{host}-2.local."
    assert service["addresses"] == ["192.0.2.2"]
    assert service["txt"]["identification"] == "SECOND"


def test_follows_interfaces(start_gateway, mdns, netns):
    browser = mdns.browse()
    gateway = start_gateway(*URL, "--mdns", ID)
    gateway.wait_for(announced("sovd0", "192.0.2.1"), 10.0)

    netns.add_interface("sovd1", "198.51.100.1/24")
    gateway.wait_for("Address added interface=sovd1 addr=198.51.100.1", 10.0)
    gateway.wait_for(announced("sovd1", "198.51.100.1"), 10.0)
    browser.event(lambda e: e["event"] == "updated" and "198.51.100.1" in e["addresses"])

    netns.ip("link", "del", "sovd1")
    gateway.wait_for("Address removed interface=sovd1 addr=198.51.100.1", 10.0)


def test_warns_without_a_usable_interface(start_gateway, netns):
    netns.ip("link", "del", "sovd0")
    gateway = start_gateway(*URL, "--mdns", ID)
    gateway.wait_for("Not announced yet after=5s", 10.0)


def test_announces_only_the_bound_address(start_gateway, mdns, netns):
    netns.ip("addr", "add", "192.0.2.2/24", "dev", "sovd0")
    browser = mdns.browse()
    gateway = start_gateway("--url", "http://192.0.2.2:0/sovd", "--mdns", ID)

    assert "interfaces=sovd0 " in line_with(gateway, "Registering")
    gateway.wait_for(announced("sovd0", "192.0.2.2"), 10.0)
    assert browser.event(added(FULLNAME))["addresses"] == ["192.0.2.2"]


def test_announces_only_on_listed_interfaces(start_gateway, netns):
    netns.add_interface("sovd1", "198.51.100.1/24")
    gateway = start_gateway(*URL, "--mdns", ID, "--mdns-interface", "sovd1")

    assert "interfaces=sovd1 " in line_with(gateway, "Registering")
    gateway.wait_for(announced("sovd1", "198.51.100.1"), 10.0)
    with pytest.raises(TimeoutError):
        gateway.wait_for("interface=sovd0", 2.0)


def test_warns_about_unknown_interface(start_gateway):
    gateway = start_gateway(*URL, "--mdns", ID, "--mdns-interface", "sovd0, no-such-intf")
    assert "Interface has no usable address yet interface=no-such-intf" in gateway.stdout


def test_host_is_derived_from_identification(start_gateway, mdns):
    browser = mdns.browse()
    gateway = start_gateway(*URL, "--mdns", "VIN_0001")

    assert "host=VIN-0001.local " in line_with(gateway, "Registering")
    assert browser.event(added(f"vin-0001.{SERVICE}"))["server"] == "VIN-0001.local."


def test_env_identification_enables_mdns(start_gateway, monkeypatch):
    monkeypatch.setenv("SOVD_MDNS", ID)
    gateway = start_gateway(*URL)
    assert f'identification="{ID}"' in line_with(gateway, "Registering")
