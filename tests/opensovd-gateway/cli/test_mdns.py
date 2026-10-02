# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

"""Tests for the --mdns options."""

import sys

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
    assert "Registering" not in out


@pytest.mark.parametrize("host", ["VIN_1", "-abc", "a.local", "a" * 59])
def test_rejects_invalid_host(request, crate_binary, host):
    code, out = run(request, crate_binary, "--mdns", ID, f"--mdns-host={host}")
    assert code == 2
    assert "invalid mDNS host" in out


@pytest.mark.parametrize("name", ["", " ", "eth0,"])
def test_rejects_empty_interface(request, crate_binary, name):
    code, _ = run(request, crate_binary, "--mdns", ID, "--mdns-interface", name)
    assert code == 2


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
        assert "Registering" not in proc.stdout
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


@pytest.mark.parametrize(
    ("name", "value"),
    [("SOVD_MDNS_HOST", "abc"), ("SOVD_MDNS_HOST", "VIN_1"), ("SOVD_MDNS_INTERFACE", ",")],
)
def test_env_options_without_mdns_are_ignored(request, crate_binary, monkeypatch, name, value):
    monkeypatch.setenv(name, value)
    args = ["--url", "http://127.0.0.1:0/sovd"]
    spawn_process(request.config, args, LISTENING_PATTERN, crate=crate_binary).close()


def test_rejects_invalid_env_host_with_mdns(request, crate_binary, monkeypatch):
    monkeypatch.setenv("SOVD_MDNS_HOST", "VIN_1")
    code, out = run(request, crate_binary, "--mdns", ID)
    assert code == 2
    assert "invalid value 'VIN_1' for '--mdns-host <HOST>': invalid mDNS host" in out
