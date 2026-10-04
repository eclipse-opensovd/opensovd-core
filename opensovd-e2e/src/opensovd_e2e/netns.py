# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

"""Private network namespaces for tests that must not touch the host network."""

import contextlib
import functools
import os
import shutil
import signal
import subprocess
import sys
import time
from pathlib import Path
from typing import Self

import pytest

_TOOLS = ("unshare", "nsenter", "setpriv", "ip")
_UNSHARE = ("unshare", "--user", "--map-root-user", "--net", "--")
_PDEATHSIG = ("setpriv", "--pdeathsig", "KILL", "--")
_PIDNS = ("unshare", "--pid", "--kill-child", "--forward-signals", "--")
_TIMEOUT = 5.0


@functools.cache
def unavailable_reason() -> str | None:
    """Why no network namespace can be created on this host, or None if one can."""
    if sys.platform != "linux":
        return "network namespaces need Linux"
    if missing := [tool for tool in _TOOLS if shutil.which(tool) is None]:
        return f"missing {', '.join(missing)}"
    probe = subprocess.run([*_UNSHARE, "true"], capture_output=True, text=True)
    if probe.returncode != 0:
        return f"unprivileged user namespaces unavailable: {probe.stderr.strip()}"
    return None


@functools.cache
def own_pid_namespace() -> bool:
    """Whether wrapped commands get their own PID namespace, which needs util-linux 2.42."""
    return subprocess.run([*_UNSHARE, *_PIDNS, "true"], capture_output=True).returncode == 0


def skip_unless_available() -> None:
    """Skip the calling test where no network namespace can be created."""
    if reason := unavailable_reason():
        pytest.skip(reason)


class NetworkNamespace:
    """A private user and network namespace with only a loopback interface.

    Commands prefixed with `wrap` run inside it and get SIGKILL when the test
    process dies. With `own_pid_namespace`, each runs as init of its own PID
    namespace, so the kernel also kills everything it forked. `close` kills
    everything left inside, which removes the namespace together with its
    interfaces.
    """

    def __init__(self):
        self._holder = subprocess.Popen([*_UNSHARE, *_PDEATHSIG, "sleep", "infinity"])
        try:
            deadline = time.monotonic() + _TIMEOUT
            while _comm(self._holder.pid) != "sleep":
                if self._holder.poll() is not None or time.monotonic() > deadline:
                    raise RuntimeError("network namespace did not start")
                time.sleep(0.01)
            self.id = os.readlink(f"/proc/{self._holder.pid}/ns/net")
            self.wrap = [
                "nsenter",
                f"--target={self._holder.pid}",
                "--user",
                "--net",
                "--preserve-credentials",
                "--",
                *_PDEATHSIG,
                *(_PIDNS if own_pid_namespace() else ()),
            ]
            self.ip("link", "set", "lo", "up")
        except BaseException:
            self._holder.kill()
            self._holder.wait()
            raise

    def run(self, *cmd: str) -> subprocess.CompletedProcess[str]:
        """Run a command inside the namespace and return its output."""
        return subprocess.run([*self.wrap, *cmd], capture_output=True, text=True, check=True)

    def ip(self, *args: str) -> None:
        self.run("ip", *args)

    def add_interface(self, name: str, *addrs: str) -> None:
        """Add a multicast capable dummy interface with the given CIDR addresses."""
        self.ip("link", "add", name, "type", "dummy")
        self.ip("link", "set", name, "multicast", "on", "up")
        for addr in addrs:
            self.ip("addr", "add", addr, "dev", name)

    def pids(self) -> list[int]:
        """Processes inside the namespace, including the one holding it open."""
        pids = []
        for entry in Path("/proc").iterdir():
            if not entry.name.isdigit():
                continue
            with contextlib.suppress(OSError):
                if os.readlink(entry / "ns" / "net") == self.id:
                    pids.append(int(entry.name))
        return pids

    def close(self) -> None:
        deadline = time.monotonic() + _TIMEOUT
        while pids := self.pids():
            if time.monotonic() > deadline:
                raise RuntimeError(f"processes left in the network namespace: {pids}")
            for pid in pids:
                with contextlib.suppress(ProcessLookupError):
                    os.kill(pid, signal.SIGKILL)
            time.sleep(0.01)
        self._holder.wait()

    def __enter__(self) -> Self:
        return self

    def __exit__(self, exc_type, exc_val, exc_tb):
        self.close()


def _comm(pid: int) -> str:
    with contextlib.suppress(OSError):
        return Path(f"/proc/{pid}/comm").read_text().strip()
    return ""
