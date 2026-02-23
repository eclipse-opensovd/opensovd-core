<!--
SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
SPDX-License-Identifier: Apache-2.0
-->

# Development

Clone the repository:

```bash
git clone https://github.com/eclipse-opensovd/opensovd-core.git
cd opensovd-core
```

There are two options to set up a build environment:

## Option 1: Dev Container (VS Code)

The repository includes a [Dev Container](.devcontainer/devcontainer.json) configuration that provides a ready-to-use environment with all tools pre-configured.

1. Install the [Dev Containers](https://marketplace.visualstudio.com/items?itemName=ms-vscode-remote.remote-containers) extension in VS Code.
2. Open the project and select **Dev Containers: Reopen in Container**.

The container includes:

- Python 3.14, Rust toolchain, and uv (via devenv)
- Pre-configured VS Code extensions (rust-analyzer, ruff, gitlens, errorlens, etc.)
- Docker-in-Docker and GitHub CLI
- Port 7690 forwarded for the gateway

## Option 2: devenv (local)

Use [devenv](https://devenv.sh/) for a reproducible local environment.

```bash
devenv shell
```

Or with direnv (auto-activates when entering the directory):

```bash
direnv allow
```

> [!NOTE]
> If neither option applies, install [Rust](https://rustup.rs/) (auto-configured via `rust-toolchain.toml`) and optionally [uv](https://docs.astral.sh/uv/) for running integration tests.
