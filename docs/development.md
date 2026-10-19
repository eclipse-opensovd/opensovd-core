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

The container installs the tools pinned in [`mise.toml`](../mise.toml), the Rust
toolchain from `rust-toolchain.toml` and the git hooks, pre-configures VS Code extensions
(rust-analyzer, ruff, etc.) and forwards port 7690 for the gateway.
The tools live in the `opensovd-core-mise` volume, so rebuilds reuse them, and the
Python environment in `/home/vscode/.venv`, apart from the checkout's `.venv`. It has
no Docker, so run the published-image workflow in [Testing](testing.md) from the host.

## Option 2: Local

Install [mise](https://mise.jdx.dev/getting-started.html) with its shell activation. It
installs the Rust toolchain from `rust-toolchain.toml` and every other tool from
[`mise.toml`](../mise.toml):

```bash
mise trust
mise install
uv sync
prek install
```

## Option 3: Bazel bootstrap

The repository also includes a Bazel entrypoint backed by `rules_rust` and `crate_universe`. This keeps the Rust workspace native in Bazel instead of shelling out to Cargo for package builds.

Use Bazel 8.6.0 directly or via Bazelisk so the workspace follows the pinned version in `.bazelversion`.

```bash
# Build the gateway through Bazel
bazel build //:opensovd-gateway
```

The Bazel integration builds the gateway and its transitive internal dependencies. Cargo remains the authoritative test workflow.

When dependency versions change, refresh the generated crate-universe lock metadata with:

```bash
CARGO_BAZEL_REPIN=1 bazel sync --only=crate_index
```
