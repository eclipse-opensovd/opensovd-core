# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

"""Self-tests for the network namespace."""

import os
import signal
import subprocess
import sys
import textwrap
import time
from pathlib import Path

import opensovd_e2e
import pytest
from opensovd_e2e.netns import NetworkNamespace, own_pid_namespace, skip_unless_available

pytestmark = pytest.mark.skipif(sys.platform != "linux", reason="network namespaces need Linux")


@pytest.fixture
def netns():
    skip_unless_available()
    with NetworkNamespace() as netns:
        yield netns


def flags(netns: NetworkNamespace, name: str) -> set[str]:
    link = netns.run("ip", "link", "show", name).stdout
    return set(link.split("<", 1)[1].split(">", 1)[0].split(","))


def children() -> set[int]:
    pid = os.getpid()
    return set(map(int, Path(f"/proc/{pid}/task/{pid}/children").read_text().split()))


def pids_in(netns: str) -> list[int]:
    pids = []
    for entry in Path("/proc").iterdir():
        try:
            if entry.name.isdigit() and os.readlink(entry / "ns" / "net") == netns:
                pids.append(int(entry.name))
        except OSError:
            continue
    return pids


def test_has_only_loopback(netns):
    assert netns.id != os.readlink("/proc/self/ns/net")
    links = netns.run("ip", "-o", "link", "show").stdout.splitlines()
    assert [line.split(": ")[1] for line in links] == ["lo"]
    assert "UP" in flags(netns, "lo")


def test_adds_interfaces(netns):
    netns.add_interface("test0", "192.0.2.1/24")
    assert {"MULTICAST", "UP"} <= flags(netns, "test0")
    assert "192.0.2.1/24" in netns.run("ip", "addr", "show", "test0").stdout


def test_close_kills_everything_inside(netns):
    inner = subprocess.Popen([*netns.wrap, "sleep", "infinity"])
    deadline = time.monotonic() + 5
    while inner.pid not in netns.pids():
        assert time.monotonic() < deadline, "process did not join the namespace"
        time.sleep(0.01)

    netns.close()
    assert inner.wait(timeout=5) == -signal.SIGKILL
    assert pids_in(netns.id) == []


def test_close_kills_processes_forked_during_close(netns):
    shell = subprocess.Popen([*netns.wrap, "sh", "-c", "while :; do sleep 60 & sleep 0.005; done"])
    deadline = time.monotonic() + 5
    while len(netns.pids()) < 3 + own_pid_namespace():
        assert time.monotonic() < deadline, "shell did not start forking"
        time.sleep(0.01)

    netns.close()
    assert shell.wait(timeout=5) == -signal.SIGKILL
    assert pids_in(netns.id) == []


def test_failed_setup_leaves_nothing(monkeypatch):
    skip_unless_available()
    before = children()

    def fail(self, *args):
        raise subprocess.CalledProcessError(1, ["ip", *args])

    monkeypatch.setattr(NetworkNamespace, "ip", fail)
    with pytest.raises(subprocess.CalledProcessError):
        NetworkNamespace()
    assert children() == before


@pytest.mark.parametrize(
    ("cmd", "forks"),
    [("exec sleep infinity", False), ("sleep infinity & wait", True)],
    ids=["direct", "forked"],
)
def test_dies_with_the_test_process(cmd, forks):
    skip_unless_available()
    if forks and not own_pid_namespace():
        pytest.skip("forked processes need util-linux 2.42 to die with the test process")
    count = 2 + forks + own_pid_namespace()
    script = """
        import subprocess, sys, time
        from opensovd_e2e.netns import NetworkNamespace

        netns = NetworkNamespace()
        subprocess.Popen([*netns.wrap, "sh", "-c", sys.argv[1]])
        while len(netns.pids()) < int(sys.argv[2]):
            time.sleep(0.01)
        print(netns.id, flush=True)
        time.sleep(60)
    """
    env = {**os.environ, "PYTHONPATH": str(Path(opensovd_e2e.__file__).parents[1])}
    parent = subprocess.Popen(
        [sys.executable, "-c", textwrap.dedent(script), cmd, str(count)],
        stdout=subprocess.PIPE,
        text=True,
        env=env,
    )
    assert parent.stdout is not None
    netns = parent.stdout.readline().strip()
    assert len(pids_in(netns)) == count

    parent.kill()
    parent.wait()
    deadline = time.monotonic() + 5
    while pids_in(netns):
        assert time.monotonic() < deadline, f"left behind: {pids_in(netns)}"
        time.sleep(0.01)
