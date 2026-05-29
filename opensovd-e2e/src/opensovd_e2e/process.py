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

    # The cargo workspace root is the pytest rootdir (the project's
    # pyproject.toml directory), not this file's location: the plugin may be
    # installed elsewhere (site-packages, a git checkout) than the crate under
    # test. Consumers that ship no crate use --opensovd-run and never reach here.
    project_root = Path(config.rootpath)
    target_dir, binary_name = _resolve_binary(project_root, crate)

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
    binary_path = base / artifact_dir / f"{binary_name}{exe_suffix}"

    # Surface what was built in the HTML report, reusing the cached metadata.
    version = next(
        (
            p.get("version", "")
            for p in _cargo_metadata(project_root).get("packages", [])
            if p.get("name") == crate
        ),
        "",
    )
    _record_metadata(
        config,
        **{
            f"Crate ({crate})": f"{crate} {version}".strip(),
            f"Binary ({crate})": str(binary_path),
            "Cargo Profile": profile,
            "Cargo Target": target or "",
        },
    )
    return binary_path


@functools.cache
def _cargo_metadata(project_root: Path) -> dict:
    """Parsed `cargo metadata --no-deps` for the workspace at `project_root`.

    Cached so the single shell-out is shared by binary resolution and report
    metadata; both read crate info from the same result.
    """
    cmd = ["cargo", "metadata", "--format-version=1", "--no-deps"]
    result = subprocess.run(cmd, cwd=project_root, capture_output=True, check=True)
    return json.loads(result.stdout)


def _resolve_binary(project_root: Path, crate: str) -> tuple[Path, str]:
    """Return (target_directory, binary_name) for `crate` via `cargo metadata`."""
    metadata = _cargo_metadata(project_root)
    target_dir = Path(metadata["target_directory"])
    for pkg in metadata.get("packages", []):
        if pkg.get("name") != crate:
            continue
        # "bin" is cargo's target-kind string for an executable target.
        binaries = [t for t in pkg.get("targets", []) if "bin" in (t.get("kind") or [])]
        if not binaries:
            raise RuntimeError(f"crate {crate!r} has no binary targets")
        for t in binaries:
            if t.get("name") == crate:
                return target_dir, t["name"]
        if len(binaries) == 1:
            return target_dir, binaries[0]["name"]
        names = ", ".join(sorted(t.get("name", "") for t in binaries))
        raise RuntimeError(
            f"crate {crate!r} has multiple binaries ({names}); none matched the crate name"
        )
    raise RuntimeError(f"crate {crate!r} not found in workspace metadata")


def _record_metadata(config: pytest.Config, **entries: str) -> None:
    """Add entries to the pytest-metadata report dict, if that plugin is present.

    No-op when pytest-metadata is unavailable (it lives in the optional ``html``
    extra), mirroring the report hooks in plugin.py. Empty values are skipped.
    """
    try:
        from pytest_metadata.plugin import metadata_key
    except ImportError:
        return
    md = config.stash.get(metadata_key, None)
    if md is None:
        return
    for key, value in entries.items():
        if value:
            md[key] = value


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
            proc.close()  # close the pipe's write end so the reader hits EOF
            output = proc.stdout  # returncode now set -> stdout drains the reader fully
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
        if self._reader_thread:
            self._reader_thread.join(timeout=PROCESS_TERMINATE_TIMEOUT)
        if self.process and self.process.stdout:
            self.process.stdout.close()

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


def spawn_process(
    config: pytest.Config,
    args: list[str],
    ready_banner: re.Pattern | None = None,
    crate: str | None = None,
) -> ProcessUnderTest:
    """Spawn the process under test with the given arguments.

    A helper for tests that need custom configurations; most tests use the
    generic module-scoped ``process`` fixture instead.

    Args:
        config: pytest configuration object
        args: Command-line arguments to pass after the run command / binary
        ready_banner: Pattern to wait for before considering ready (None to
            skip). The match is stored on ProcessUnderTest.match for consumers.
        crate: cargo workspace package name to build when --opensovd-run is
            unset; required on that path.

    Returns:
        A running ProcessUnderTest instance (caller must call close())
    """
    run_cmd = config.getoption("--opensovd-run")
    if run_cmd:
        cmd = [*shlex.split(run_cmd), *args]
        _record_metadata(config, **{"Binary (prebuilt)": run_cmd})
    else:
        if crate is None:
            raise pytest.UsageError(
                "no crate to build: pass --opensovd-run or override crate_binary"
            )
        binary_path = _build_crate_binary(config, crate)
        cmd = [str(binary_path), *args]
    return ProcessUnderTest.spawn(cmd, ready_banner=ready_banner)
