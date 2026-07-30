<!--
SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
SPDX-License-Identifier: Apache-2.0
-->

# opensovd-e2e

Reusable pytest plugin for end-to-end testing of OpenSOVD binaries. It provides
the generic harness - spawning the binary, waiting for a startup banner,
capturing output, and requirement traceability - so any OpenSOVD repo can test
its own binary without duplicating the setup. Project-specific bits (default
crate, server URL/banner, metadata) are layered on as fixture overrides in the
consuming repo.

## Install

Depend on it via a Git source (no PyPI publish):

```toml
# pyproject.toml
[tool.uv.sources]
opensovd-e2e = { git = "https://github.com/eclipse-opensovd/opensovd-core", subdirectory = "opensovd-e2e" }
```

The plugin auto-activates once installed (via its `pytest11` entry point).

For the Requirements column and metadata links in the HTML report, install the
`html` extra (pulls in `pytest-html`/`pytest-metadata`):

```toml
[tool.uv.sources]
opensovd-e2e = { git = "https://github.com/eclipse-opensovd/opensovd-core", subdirectory = "opensovd-e2e", extras = ["html"] }
```

## Usage

Provide the binary one of two ways:

- `--opensovd-run=<cmd>` - run a prebuilt binary (test args are appended), or
- override the `crate_binary` fixture - build that crate from the cargo
  workspace (`--opensovd-profile`/`--opensovd-target`/`--opensovd-features`).

A minimal CLI test using the generic `process` fixture:

```python
import re
import pytest

VERSION = re.compile(r"my-binary (\d+\.\d+\.\d+)")

@pytest.fixture(scope="module")
def crate_binary() -> str:        # build target when --opensovd-run is unset
    return "my-binary"

@pytest.fixture(scope="module")
def binary_args() -> list[str]:   # ready_banner defaults to None (no wait)
    return ["--version"]

def test_version(process):
    match = process.wait_for(VERSION, timeout_seconds=5.0)
    process.process.wait(timeout=5.0)
    assert process.process.returncode == 0
```

```bash
uv run pytest --opensovd-run=./target/release/my-binary
```

## What it provides

- **Options:** `--opensovd-run`, `--opensovd-args`, `--opensovd-profile`,
  `--opensovd-target`, `--opensovd-features`, `--opensovd-coverage`.
- **Fixtures:** `crate_binary`, `binary_args`, `ready_banner`, `process`
  (override any of them for binary-specific behaviour, e.g. an HTTP client).
- **Requirement traceability:** the `@pytest.mark.req("...")` marker, a
  Requirements column in the HTML report, and a `requirements-coverage.txt`
  matrix.
