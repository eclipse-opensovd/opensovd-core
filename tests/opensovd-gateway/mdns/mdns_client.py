# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

"""mDNS client for the network namespace tests, printing one JSON event per line.

browse                         report services as they are added, updated and removed
register INSTANCE HOST ADDR    hold an instance name and host name until SIGTERM
fetch INSTANCE PATH            resolve INSTANCE and GET PATH below its accessurl
resolve INSTANCE               report the IPv4 addresses INSTANCE is advertised with
"""

import argparse
import json
import signal
import socket
import threading
import time

import httpx
from zeroconf import (
    DNSAddress,
    DNSOutgoing,
    DNSQuestion,
    IPVersion,
    ServiceBrowser,
    ServiceInfo,
    ServiceStateChange,
    Zeroconf,
)
from zeroconf.const import _CLASS_IN, _FLAGS_QR_QUERY, _TYPE_A

SERVICE_TYPE = "_sovd._tcp.local."
RESOLVE_TIMEOUT_MS = 3000
REQUERY_DELAY_S = 1.1
ANSWER_WAIT_S = 0.5


def emit(event: str, **fields) -> None:
    print(json.dumps({"event": event, **fields}), flush=True)


def describe(info: ServiceInfo) -> dict:
    return {
        "name": info.name,
        "server": info.server,
        "port": info.port,
        "addresses": sorted(info.parsed_addresses()),
        "txt": {
            key.decode(): value.decode() if value is not None else None
            for key, value in info.properties.items()
        },
    }


def wait_for_sigterm() -> None:
    stop = threading.Event()
    signal.signal(signal.SIGTERM, lambda *_: stop.set())
    stop.wait()


def browse(zc: Zeroconf) -> None:
    def on_change(zeroconf, service_type, name, state_change):
        if state_change is ServiceStateChange.Removed:
            emit("removed", name=name)
            return
        info = zeroconf.get_service_info(service_type, name, timeout=RESOLVE_TIMEOUT_MS)
        if info is not None:
            emit(state_change.name.lower(), **describe(info))

    ServiceBrowser(zc, SERVICE_TYPE, handlers=[on_change])
    emit("browsing")
    wait_for_sigterm()


def register(zc: Zeroconf, instance: str, host: str, address: str) -> None:
    info = ServiceInfo(
        SERVICE_TYPE,
        f"{instance}.{SERVICE_TYPE}",
        port=9,
        addresses=[socket.inet_aton(address)],
        server=f"{host}.local.",
    )
    zc.register_service(info)
    emit("registered", **describe(info))
    wait_for_sigterm()
    zc.unregister_service(info)


def fetch(zc: Zeroconf, instance: str, path: str) -> None:
    info = zc.get_service_info(SERVICE_TYPE, f"{instance}.{SERVICE_TYPE}", RESOLVE_TIMEOUT_MS)
    accessurl = info.properties.get(b"accessurl") if info is not None else None
    if info is None or accessurl is None:
        emit("unresolved", name=instance)
        return
    url = httpx.URL(accessurl.decode() + path)
    address = info.parsed_addresses()[0]
    response = httpx.get(
        url.copy_with(host=address), headers={"Host": url.netloc.decode()}, trust_env=False
    )
    emit("fetched", url=str(url), address=address, status=response.status_code)


def resolve(zc: Zeroconf, instance: str) -> None:
    info = ServiceInfo(SERVICE_TYPE, f"{instance}.{SERVICE_TYPE}")
    if not info.request(zc, RESOLVE_TIMEOUT_MS) or info.server is None:
        emit("unresolved", name=instance)
        return
    # Interfaces answer with their own address at most once a second.
    time.sleep(REQUERY_DELAY_S)
    query = DNSOutgoing(_FLAGS_QR_QUERY)
    query.add_question(DNSQuestion(info.server, _TYPE_A, _CLASS_IN))
    zc.send(query)
    time.sleep(ANSWER_WAIT_S)
    records = zc.cache.entries_with_name(info.server.lower())
    addresses = {
        socket.inet_ntoa(r.address)
        for r in records
        if isinstance(r, DNSAddress) and len(r.address) == 4
    }
    emit("resolved", name=instance, addresses=sorted(addresses))


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("browse")
    reg = commands.add_parser("register")
    reg.add_argument("instance")
    reg.add_argument("host")
    reg.add_argument("address")
    get = commands.add_parser("fetch")
    get.add_argument("instance")
    get.add_argument("path")
    commands.add_parser("resolve").add_argument("instance")
    args = parser.parse_args()

    zc = Zeroconf(ip_version=IPVersion.V4Only)
    try:
        match args.command:
            case "browse":
                browse(zc)
            case "register":
                register(zc, args.instance, args.host, args.address)
            case "fetch":
                fetch(zc, args.instance, args.path)
            case "resolve":
                resolve(zc, args.instance)
    finally:
        zc.close()


if __name__ == "__main__":
    main()
