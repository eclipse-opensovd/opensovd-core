# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

"""Shared test fixtures and utilities for the process under test."""

import functools
import json
import re
import shlex
import subprocess
import sysconfig
import threading
import time
from pathlib import Path
from typing import Self

import pytest

# Timeout constants (seconds)
PROCESS_SPAWN_TIMEOUT = 30.0
PROCESS_WAIT_TIMEOUT = 1.0
PROCESS_TERMINATE_TIMEOUT = 5.0

LISTENING_PATTERN = re.compile(r"Listening addr=(\S+) type=(tcp|unix|abstract|tls) base=(\S+)")


def _build_crate_binary(config: pytest.Config, crate: str) -> Path:
    """Build the opensovd binary for the given crate via cargo.

    Args:
        config: pytest configuration object
        crate: cargo workspace package name to build

    Returns:
        Path to the opensovd binary, resolved via `cargo metadata`
    """
    profile = config.getoption("--opensovd-profile") or "dev"
    target = config.getoption("--opensovd-target")

    project_root = Path(__file__).parent.parent
    target_dir, bin_name = _resolve_bin(project_root, crate)

    cargo_cmd = ["cargo", "build", "--locked", "-p", crate, "--profile", profile]
    if target:
        cargo_cmd.extend(["--target", target])
    if features := config.getoption("--opensovd-features"):
        cargo_cmd.extend(["--features", features])

    subprocess.run(
        cargo_cmd,
        cwd=project_root,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=True,
    )

    # cargo writes the dev profile to target/[<triple>/]debug/, every other
    # profile to a directory matching its name.
    artifact_dir = "debug" if profile == "dev" else profile
    base = target_dir / target if target else target_dir
    exe_suffix = sysconfig.get_config_var("EXE") or ""
    return base / artifact_dir / f"{bin_name}{exe_suffix}"


@functools.cache
def _resolve_bin(project_root: Path, crate: str) -> tuple[Path, str]:
    """Return (target_directory, bin_name) for `crate` via `cargo metadata`."""
    cmd = ["cargo", "metadata", "--format-version=1", "--no-deps"]
    result = subprocess.run(cmd, cwd=project_root, capture_output=True, check=True)

    metadata = json.loads(result.stdout)
    target_dir = Path(metadata["target_directory"])
    for pkg in metadata.get("packages", []):
        if pkg.get("name") != crate:
            continue
        bins = [t for t in pkg.get("targets", []) if "bin" in (t.get("kind") or [])]
        if not bins:
            raise RuntimeError(f"crate {crate!r} has no bin targets")
        for t in bins:
            if t.get("name") == crate:
                return target_dir, t["name"]
        if len(bins) == 1:
            return target_dir, bins[0]["name"]
        names = ", ".join(sorted(t.get("name", "") for t in bins))
        raise RuntimeError(
            f"crate {crate!r} has multiple bins ({names}); none matched the crate name"
        )
    raise RuntimeError(f"crate {crate!r} not found in workspace metadata")


class ProcessUnderTest:
    def __init__(self, process: subprocess.Popen | None = None):
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
        self.match: re.Match | None = None
        self._output_printed = False

    @classmethod
    def spawn(
        cls,
        cmd: list[str],
        timeout_seconds: float = PROCESS_SPAWN_TIMEOUT,
        env: dict | None = None,
        ready_banner: re.Pattern | None = None,
    ) -> Self:
        """Spawn process, wait for banner, return ready ProcessUnderTest.

        Args:
            cmd: Command to execute
            timeout_seconds: Maximum seconds to wait for banner
            env: Environment variables for the process
            ready_banner: Pattern to wait for before considering ready (None
                to skip). The `re.Match` is stored on `.match` for consumers.
        """
        process = subprocess.Popen(
            cmd,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            env=env,
            text=True,
            encoding="utf-8",
            errors="replace",
        )

        proc = cls(process)
        if ready_banner is None:
            return proc

        try:
            proc.match = proc.wait_for(ready_banner, timeout_seconds)
            return proc
        except (TimeoutError, RuntimeError) as e:
            output = proc.stdout
            proc.close()
            if output:
                e.add_note(f"Process Output:\n{output}")
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

    def wait_for(
        self,
        pattern: str | re.Pattern,
        timeout_seconds: float = PROCESS_WAIT_TIMEOUT,
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
            # Clear before checking so a set() that races with the drain
            # below still wakes the next wait().
            self._line_event.clear()
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
            self._line_event.wait(timeout=remaining)

    def close(self):
        if self.process and self.process.returncode is None:
            self.process.terminate()
            try:
                self.process.wait(timeout=PROCESS_TERMINATE_TIMEOUT)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait(timeout=PROCESS_TERMINATE_TIMEOUT)

    def __enter__(self):
        return self

    def __exit__(self, exc_type, exc_val, exc_tb):
        self.close()

    def _read_output(self):
        """Read stdout in background thread."""
        stdout = self.process.stdout if self.process else None
        if stdout is None:
            self._closed = True
            self._line_event.set()
            return
        try:
            for line in stdout:
                with self._lock:
                    self._output.append(line)
                self._line_event.set()
        finally:
            self._closed = True
            self._line_event.set()


def default_binary_args(config: pytest.Config, *extra: str) -> list[str]:
    """Build binary args: ephemeral-port URL plus extras and any --opensovd-args.

    Skips the auto-injected --url if the caller (or --opensovd-args) already
    supplied one. Detects both `--url X` and `--url=X` forms.
    """
    extra_args = shlex.split(config.getoption("--opensovd-args"))
    has_url = any(a == "--url" or a.startswith("--url=") for a in (*extra, *extra_args))
    if has_url:
        return [*extra, *extra_args]
    return ["--url", "http://127.0.0.1:0/sovd", *extra, *extra_args]


def spawn_process(
    config: pytest.Config,
    args: list[str],
    ready_banner: re.Pattern | None = None,
    crate: str = "opensovd-gateway",
) -> ProcessUnderTest:
    """Spawn the process under test with the given arguments.

    This is a helper for tests that need custom configurations.
    For standard tests, use the module-scoped `gateway` fixture instead.

    Args:
        config: pytest configuration object
        args: Command-line arguments to pass after the run command / binary
        ready_banner: Pattern to wait for before considering ready (None to
            skip). The match (groups: addr, transport, base) is stored on
            ProcessUnderTest.match for SovdClient to interpret.
        crate: cargo workspace package name to build when --opensovd-run is unset

    Returns:
        A running ProcessUnderTest instance (caller must call close())
    """
    run_cmd = config.getoption("--opensovd-run")
    if run_cmd:
        cmd = [*shlex.split(run_cmd), *args]
    else:
        bin_path = _build_crate_binary(config, crate)
        cmd = [str(bin_path), *args]
    return ProcessUnderTest.spawn(cmd, ready_banner=ready_banner)


def listening_url(match: re.Match) -> str:
    """Build a base URL string from a LISTENING_PATTERN match.

    The match is expected to capture (addr, transport, base) in groups 1-3.
    Useful for consumers (e.g. Bruno) that only need the URL and not a full
    SovdClient with an httpx connection pool.
    """
    addr, transport, base = match.group(1), match.group(2), match.group(3)
    if transport == "tcp":
        return f"http://{addr}{base}"
    if transport == "tls":
        return f"https://{addr}{base}"
    return f"http://localhost{base}"
