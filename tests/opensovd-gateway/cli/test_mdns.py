# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

"""Tests for the --mdns options."""

import signal
import sys

import httpx
import pytest
from fixtures import LISTENING_PATTERN
from opensovd_e2e import spawn_process

ID = "OPENSOVDTEST0001"


def run(request, crate_binary, *args):
    proc = spawn_process(request.config, list(args), None, crate=crate_binary)
    assert proc.process is not None
    try:
        proc.process.wait(timeout=10.0)
        return proc.process.returncode, proc.stdout
    finally:
        proc.close()


def test_rejects_loopback(request, crate_binary):
    args = ["--url", "http://127.0.0.1:0/sovd", "--mdns", ID]
    code, out = run(request, crate_binary, *args)
    assert code == 1
    assert "loopback" in out


def test_rejects_https_url_without_tls(request, crate_binary):
    args = ["--url", "https://0.0.0.0:0/sovd", "--mdns", ID]
    code, out = run(request, crate_binary, *args)
    assert code == 1
    assert "--tls-cert" in out


def test_failed_startup_does_not_announce(request, crate_binary):
    args = ["--url", "http://0.0.0.0:0/sovd", "--mdns", ID]
    code, out = run(request, crate_binary, *args, "--serve-dir", "nocolon")
    assert code == 1
    assert "mDNS enabled" not in out


@pytest.mark.parametrize("host", ["VIN_1", "-abc", "a.local", "a" * 61])
def test_rejects_invalid_host(request, crate_binary, host):
    code, out = run(request, crate_binary, "--mdns", ID, f"--mdns-host={host}")
    assert code == 2
    assert "invalid mDNS host" in out


@pytest.mark.parametrize("name", ["", " ", "eth0,"])
def test_rejects_empty_interface(request, crate_binary, name):
    code, _ = run(request, crate_binary, "--mdns", ID, "--mdns-interface", name)
    assert code == 2


def test_warns_about_unknown_interface(request, crate_binary):
    args = ["--url", "http://0.0.0.0:0/sovd", "--mdns", ID]
    args += ["--mdns-interface", "eth0, no-such-intf"]
    proc = spawn_process(request.config, args, LISTENING_PATTERN, crate=crate_binary)
    try:
        assert "mDNS interface has no usable address yet interface=no-such-intf" in proc.stdout
    finally:
        proc.close()


def test_host_is_derived_from_identification(request, crate_binary):
    args = ["--url", "http://0.0.0.0:0/sovd", "--mdns", "VIN_0001"]
    proc = spawn_process(request.config, args, LISTENING_PATTERN, crate=crate_binary)
    try:
        assert "host=VIN-0001.local." in proc.stdout
    finally:
        proc.close()


def test_requires_identification(request, crate_binary):
    code, _ = run(request, crate_binary, "--url", "http://0.0.0.0:0/sovd", "--mdns")
    assert code == 2


@pytest.mark.parametrize("value", ["true", "1", "YES", "on"])
def test_rejects_switch_values(request, crate_binary, value):
    code, out = run(request, crate_binary, "--url", "http://0.0.0.0:0/sovd", "--mdns", value)
    assert code == 2
    assert "vehicle identification" in out


@pytest.mark.parametrize("value", ["", "false", "0", "No", "off"])
def test_off_values_disable_mdns(request, crate_binary, monkeypatch, value):
    monkeypatch.setenv("SOVD_MDNS", value)
    args = ["--url", "http://0.0.0.0:0/sovd"]
    proc = spawn_process(request.config, args, LISTENING_PATTERN, crate=crate_binary)
    try:
        assert "mDNS enabled" not in proc.stdout
    finally:
        proc.close()


def test_env_identification_enables_mdns(request, crate_binary, monkeypatch):
    monkeypatch.setenv("SOVD_MDNS", ID)
    args = ["--url", "http://0.0.0.0:0/sovd"]
    proc = spawn_process(request.config, args, LISTENING_PATTERN, crate=crate_binary)
    try:
        assert f"identification={ID}" in proc.stdout
    finally:
        proc.close()


def test_options_require_mdns(request, crate_binary):
    code, _ = run(request, crate_binary, "--mdns-host", "abc")
    assert code == 2


@pytest.mark.skipif(sys.platform == "win32", reason="Unix sockets only")
def test_conflicts_with_unix_socket(request, crate_binary, tmp_path):
    code, _ = run(request, crate_binary, "--mdns", ID, "--unix-socket", str(tmp_path / "gw.sock"))
    assert code == 2


@pytest.mark.skipif(sys.platform == "win32", reason="Unix sockets only")
def test_disabled_env_allows_unix_socket(request, crate_binary, tmp_path, monkeypatch):
    monkeypatch.setenv("SOVD_MDNS", "false")
    args = ["--unix-socket", str(tmp_path / "gw.sock")]
    spawn_process(request.config, args, LISTENING_PATTERN, crate=crate_binary).close()


def test_env_options_without_mdns_are_ignored(request, crate_binary, monkeypatch):
    monkeypatch.setenv("SOVD_MDNS_HOST", "abc")
    args = ["--url", "http://127.0.0.1:0/sovd"]
    spawn_process(request.config, args, LISTENING_PATTERN, crate=crate_binary).close()


@pytest.mark.skipif(sys.platform == "win32", reason="Unix signals only")
def test_logs_enabled_and_disabled(request, crate_binary):
    proc = spawn_process(
        request.config,
        ["--url", "http://0.0.0.0:0/sovd", "--mdns", ID],
        LISTENING_PATTERN,
        crate=crate_binary,
    )
    assert proc.process is not None
    try:
        listening = LISTENING_PATTERN.search(proc.stdout)
        assert listening is not None
        port = listening.group(1).rsplit(":", 1)[1]
        enabled = next(line for line in proc.stdout.splitlines() if "mDNS enabled" in line)
        for field in (
            f"instance={ID.lower()}",
            "service=_sovd._tcp.local.",
            f"host={ID}.local.",
            "addr=0.0.0.0",
            "interfaces=all",
            f"identification={ID}",
            f"accessurl=http://{ID}.local:{port}/sovd",
        ):
            assert field in enabled

        httpx.get(f"http://127.0.0.1:{port}/sovd/version-info", timeout=5.0).raise_for_status()
        proc.process.send_signal(signal.SIGTERM)
        proc.wait_for(f"mDNS disabled instance={ID.lower()} host={ID}.local.")
        proc.process.wait(timeout=5.0)
        assert proc.process.returncode == 0
    finally:
        proc.close()
