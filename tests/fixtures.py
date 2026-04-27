# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

"""Shared test fixtures and utilities for gateway tests."""

from __future__ import annotations

import re
import shlex
import subprocess
import threading
import time
from pathlib import Path
from typing import Self

import httpx
import pytest

# Timeout constants (seconds)
GATEWAY_SPAWN_TIMEOUT = 30.0
GATEWAY_WAIT_TIMEOUT = 1.0
GATEWAY_TERMINATE_TIMEOUT = 5.0

LISTENING_PATTERN = re.compile(
    r"Listening addr=([^\s]+) type=(tcp|unix|abstract|tls|mtls) base=([^\s]+)"
)


def _build_gateway(config: pytest.Config, extra_features: list[str] | None = None) -> Path:
    """Build or locate the gateway binary.

    Args:
        config: pytest configuration object
        extra_features: Additional cargo features to enable on top of --opensovd-features

    Returns:
        Path to the gateway binary
    """
    binary = config.getoption("--opensovd-binary")
    if binary and not extra_features:
        return Path(binary)

    release_mode = config.getoption("--opensovd-release")
    configured = config.getoption("--opensovd-features") or ""
    features: set[str] = {f for f in configured.split(",") if f}
    if extra_features:
        features.update(extra_features)

    project_root = Path(__file__).parent.parent
    cargo_cmd = ["cargo", "build", "-p", "opensovd-gateway"]
    if release_mode:
        cargo_cmd.append("--release")
    if features:
        cargo_cmd.extend(["--features", ",".join(sorted(features))])

    result = subprocess.run(
        cargo_cmd,
        cwd=project_root,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
    )
    if result.returncode != 0:
        raise subprocess.CalledProcessError(result.returncode, cargo_cmd, result.stdout)

    profile = "release" if release_mode else "debug"
    return project_root / "target" / profile / "opensovd-gateway"


def get_gateway_binary(config: pytest.Config) -> Path:
    """Build or locate the gateway binary based on pytest options."""
    return _build_gateway(config)


def get_tls_gateway_binary(config: pytest.Config) -> Path:
    """Build or locate the gateway binary with the tls feature enabled."""
    return _build_gateway(config, extra_features=["tls"])


class Gateway:
    def __init__(
        self,
        process: subprocess.Popen | None = None,
        *,
        base_url: str | None = None,
        addr: str | None = None,
        transport: str | None = None,
    ):
        self.process = process
        self._output: list[str] = []
        self._line_event = threading.Event()
        self._lock = threading.Lock()
        self._read_pos = 0
        self._closed = False
        self._reader_thread: threading.Thread | None = None
        if process and process.stdout:
            self._reader_thread = threading.Thread(target=self._read_output, daemon=True)
            self._reader_thread.start()
        self.base_url = base_url
        self.addr = addr
        self.transport = transport
        self.client = httpx.Client(base_url=base_url) if base_url else None
        self._output_printed = False

    @classmethod
    def spawn(
        cls,
        cmd: list[str],
        timeout_seconds: float = GATEWAY_SPAWN_TIMEOUT,
        env: dict | None = None,
        banner: str | re.Pattern | None = None,
        docker_container: str | None = None,
        ssl_context=None,
    ) -> Self:
        """Spawn process, wait for listening, return ready Gateway.

        Args:
            cmd: Command to execute
            timeout_seconds: Maximum seconds to wait for banner
            env: Environment variables for the process
            banner: Pattern to wait for before considering ready
            docker_container: If provided, use `docker port` to resolve the host address
        """
        process = subprocess.Popen(
            cmd,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            env=env,
        )

        gw = cls(process)
        if banner is None:
            return gw

        try:
            gw.wait_for(banner, timeout_seconds)

            # Docker mode: resolve mapped port via `docker port`
            if docker_container:
                port_result = subprocess.run(
                    ["docker", "port", docker_container, "7690"],
                    stdout=subprocess.PIPE,
                )
                host_port = port_result.stdout.decode().strip().split(":")[-1]

                gw.addr = f"127.0.0.1:{host_port}"
                gw.transport = "tcp"
                gw.base_url = f"http://127.0.0.1:{host_port}/sovd"
                gw.client = httpx.Client(base_url=gw.base_url)
            else:
                # Standard mode: parse address from process output
                match = LISTENING_PATTERN.search(gw.stdout)
                if match:
                    gw.addr, gw.transport, base = match.group(1), match.group(2), match.group(3)
                    if gw.transport == "tcp":
                        gw.base_url = f"http://{gw.addr}{base}"
                        gw.client = httpx.Client(base_url=gw.base_url)
                    elif gw.transport in ("tls", "mtls"):
                        gw.base_url = f"https://{gw.addr}{base}"
                        gw.client = httpx.Client(
                            base_url=gw.base_url,
                            verify=ssl_context if ssl_context is not None else True,
                        )
                    else:
                        gw.base_url = f"http://localhost{base}"
                        uds_addr = "\0" + gw.addr if gw.transport == "abstract" else gw.addr
                        gw.client = httpx.Client(
                            base_url=gw.base_url,
                            transport=httpx.HTTPTransport(uds=uds_addr),
                        )

            return gw
        except Exception as e:
            output = gw.stdout
            gw.close()
            if output:
                raise RuntimeError(f"{e}\n\nGateway Output:\n{output}") from e
            raise

    @property
    def has_output(self) -> bool:
        with self._lock:
            return len(self._output) > 0

    @property
    def stdout(self) -> str:
        # If process exited, wait for reader thread to finish draining the pipe
        if self.process and self.process.returncode is not None and self._reader_thread:
            self._reader_thread.join(timeout=1.0)
        with self._lock:
            return "".join(self._output)

    def get(self, path: str, **kwargs) -> httpx.Response:
        assert self.client is not None
        return self.client.get(path, **kwargs)

    def post(self, path: str, **kwargs) -> httpx.Response:
        assert self.client is not None
        return self.client.post(path, **kwargs)

    def put(self, path: str, **kwargs) -> httpx.Response:
        assert self.client is not None
        return self.client.put(path, **kwargs)

    def delete(self, path: str, **kwargs) -> httpx.Response:
        assert self.client is not None
        return self.client.delete(path, **kwargs)

    def wait_for(
        self,
        pattern: str | re.Pattern,
        timeout_seconds: float = GATEWAY_WAIT_TIMEOUT,
    ) -> re.Match[str]:
        """Wait for a line matching pattern in stdout.

        Args:
            pattern: String or compiled regex to match against lines
            timeout_seconds: Maximum seconds to wait

        Returns:
            The match object (use .string for full line, .group() for matched text)

        Raises:
            RuntimeError: If process exits before pattern matched
            TimeoutError: If no matching line found within timeout
        """
        if isinstance(pattern, str):
            pattern = re.compile(re.escape(pattern))

        deadline = time.monotonic() + timeout_seconds
        while True:
            # Check existing lines
            with self._lock:
                while self._read_pos < len(self._output):
                    line = self._output[self._read_pos]
                    self._read_pos += 1
                    if match := pattern.search(line):
                        return match
            if self._closed:
                raise RuntimeError("Process exited before pattern matched")
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise TimeoutError(
                    f"Pattern {pattern.pattern!r} not found within {timeout_seconds}s"
                )
            self._line_event.clear()
            self._line_event.wait(timeout=remaining)

    def close(self):
        if self.client:
            self.client.close()
        if self.process and self.process.returncode is None:
            self.process.terminate()
            try:
                self.process.wait(timeout=GATEWAY_TERMINATE_TIMEOUT)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait(timeout=GATEWAY_TERMINATE_TIMEOUT)

    def __enter__(self):
        return self

    def __exit__(self, exc_type, exc_val, exc_tb):
        self.close()

    def _read_output(self):
        """Read stdout in background thread."""
        try:
            assert self.process is not None
            assert self.process.stdout is not None
            for line in self.process.stdout:
                with self._lock:
                    self._output.append(line.decode())
                self._line_event.set()
        finally:
            self._closed = True
            self._line_event.set()


def default_gateway_args(config: pytest.Config, *extra: str) -> list[str]:
    """Build gateway args with Docker-vs-local URL detection and extra CLI options."""
    extra_args = shlex.split(config.getoption("--opensovd-args"))
    if config.getoption("--opensovd-docker"):
        url = "http://0.0.0.0:7690/sovd"
    else:
        url = "http://127.0.0.1:0/sovd"
    return ["--url", url, *extra, *extra_args]


def spawn_gateway(
    config: pytest.Config,
    args: list[str],
    banner: str | re.Pattern | None = "Listening addr=",
    extra_features: list[str] | None = None,
    ssl_context=None,
) -> Gateway:
    """Spawn a gateway process with the given arguments.

    This is a helper for tests that need custom gateway configurations.
    For standard tests, use the session-scoped `gateway` fixture instead.

    Args:
        config: pytest configuration object
        args: Command-line arguments for the gateway
        banner: Pattern to wait for before considering ready (None to skip)
        extra_features: Additional cargo features to enable (e.g. ["tls"])
        ssl_context: ssl.SSLContext for HTTPS gateways; passed to httpx.Client

    Returns:
        A running Gateway instance (caller must call close())
    """
    docker_image = config.getoption("--opensovd-docker")

    if docker_image:
        import uuid

        container_name = f"sovd-test-{uuid.uuid4().hex[:8]}"

        socket_type = "tcp"
        socket_path = None
        for i, arg in enumerate(args):
            if arg == "--unix-socket" and i + 1 < len(args):
                socket_path = args[i + 1]
                socket_type = "abstract" if socket_path.startswith("@") else "unix"
                break

        cmd = ["docker", "run", "--rm", "-i", "--name", container_name]

        if socket_type == "unix":
            assert socket_path is not None
            socket_dir = str(Path(socket_path).parent)
            cmd += ["-v", f"{socket_dir}:{socket_dir}"]
        elif socket_type == "abstract":
            cmd += ["--network=host"]
        else:
            cmd += ["-P"]

        cmd += [docker_image, *args]

        docker_container = container_name if socket_type == "tcp" else None
        return Gateway.spawn(cmd, banner=banner, docker_container=docker_container)
    else:
        bin_path = _build_gateway(config, extra_features=extra_features)
        cmd = [str(bin_path), *args]
        return Gateway.spawn(cmd, banner=banner, ssl_context=ssl_context)
