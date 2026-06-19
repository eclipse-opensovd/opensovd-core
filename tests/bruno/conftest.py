# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

"""Pytest integration for Bruno test files."""

import shutil
import subprocess

import pytest
from fixtures import LISTENING_PATTERN, default_binary_args, listening_url, spawn_process


def pytest_collect_file(parent, file_path):
    """Collect .bru files as test items."""
    if (
        file_path.suffix == ".bru"
        and file_path.name != "folder.bru"
        and "environments" not in file_path.parts
    ):
        return BrunoFile.from_parent(parent, path=file_path)


@pytest.hookimpl(tryfirst=True)
def pytest_runtest_setup(item):
    """Spawn gateway for Bruno tests."""
    if not isinstance(item, BrunoItem):
        return

    if shutil.which("bru") is None:
        pytest.skip("bru CLI not installed")

    if not hasattr(item.config, "_bruno_process"):
        proc = spawn_process(item.config, default_binary_args(item.config), LISTENING_PATTERN)
        item.config._bruno_process = proc
        if proc.match is not None:
            item.config._gateway_base_url = listening_url(proc.match)


@pytest.hookimpl(trylast=True)
def pytest_sessionfinish(session, exitstatus):
    """Clean up gateway."""
    if hasattr(session.config, "_bruno_process"):
        session.config._bruno_process.close()


class BrunoFile(pytest.File):
    def collect(self):
        yield BrunoItem.from_parent(self, name=self.path.stem)


class BrunoItem(pytest.Item):
    def __init__(self, name, parent):
        super().__init__(name, parent)
        self.add_marker(pytest.mark.bruno)

    def runtest(self):
        bru = shutil.which("bru")
        if bru is None:
            pytest.skip("bru CLI not installed")

        # Find bruno.json root
        collection_root = self.path.parent
        while collection_root.parent != collection_root:
            if (collection_root / "bruno.json").exists():
                break
            collection_root = collection_root.parent

        base_url = getattr(self.config, "_gateway_base_url", None)
        if base_url is None:
            raise RuntimeError("Gateway base URL not set -- gateway may have failed to start")
        result = subprocess.run(
            [
                bru,
                "run",
                str(self.path),
                "--env",
                "local",
                "--env-var",
                f"base_url={base_url}",
            ],
            cwd=collection_root,
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
        )
        if result.returncode != 0:
            raise BrunoTestException(result.stdout, result.stderr)

    def repr_failure(self, excinfo, style=None):
        if isinstance(excinfo.value, BrunoTestException):
            return f"Bruno test failed:\n{excinfo.value.stdout}\n{excinfo.value.stderr}"
        return super().repr_failure(excinfo, style)

    def reportinfo(self):
        return self.path, None, f"bruno: {self.name}"


class BrunoTestException(Exception):
    def __init__(self, stdout, stderr):
        self.stdout = stdout
        self.stderr = stderr
        super().__init__(f"{stdout}\n{stderr}")
