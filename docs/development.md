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

- Python, Rust toolchain, and uv (versions per [devcontainer.json](../.devcontainer/devcontainer.json))
- Pre-configured VS Code extensions (rust-analyzer, ruff, gitlens, errorlens, etc.)
- GitHub CLI
- Port 7690 forwarded for the gateway

## Option 2: Nix flake (local)

Use the [Nix flake](../flake.nix) for a reproducible local environment. It requires
[Nix](https://nixos.org/download/) with the `nix-command` and `flakes`
[experimental features](https://nixos.org/manual/nix/stable/command-ref/conf-file#conf-experimental-features) enabled.

```bash
nix develop
```

The Rust toolchain is pinned via `rust-toolchain.toml` (the single source of truth shared
with `cargo`/`rustup`); changing it also requires updating the toolchain `sha256` in
[`flake.nix`](../flake.nix); the comment there explains how. Python, uv, and the
supporting CLI tools are provided by the flake. Run `uv sync` after entering the shell
(and after `uv.lock` changes) to set up the Python integration-test environment.

Or with [direnv](https://direnv.net/) (auto-activates when entering the directory).
Install [nix-direnv](https://github.com/nix-community/nix-direnv) too, since it caches the
environment between entries and protects it from garbage collection:

```bash
direnv allow
```

> [!NOTE]
> If neither option applies, install [Rust](https://rustup.rs/) (auto-configured via `rust-toolchain.toml`) and optionally [uv](https://docs.astral.sh/uv/) for running integration tests.

## Building with Bazel

Cargo is the primary build. Bazel support is additive and currently covers the `opensovd-core` crate
only; the rest of the workspace is Cargo-only.

```bash
bazel build //...
```

After changing `Cargo.toml` or `Cargo.lock`, regenerate `cargo-bazel-lock.json` and commit it:

```bash
CARGO_BAZEL_REPIN=true bazel query //... > /dev/null
```

`Cargo.lock` is the source of truth for dependency versions; `cargo-bazel-lock.json` is a derived cache.

> [!NOTE]
> Bazel does not read `rust-toolchain.toml`. It builds with the stable Rust version pinned in
> `MODULE.bazel`, while Cargo uses the nightly pinned in `rust-toolchain.toml`.

### Working with MODULE.bazel.lock

Any `bazel mod` subcommand rewrites `MODULE.bazel.lock`, because it evaluates every extension in the
transitive module graph rather than only those the build needs. `bazel mod graph` alone adds a few
thousand lines. Run `git checkout -- MODULE.bazel.lock` afterwards rather than committing it.

`MODULE.bazel.lock` is also bound to the Bazel version that produced it, which is why `.bazelversion`
must be honoured. CI builds with `--lockfile_mode=error`, so a different Bazel fails outright.

### Host-specific Bazel settings

`.bazelrc` ends with `try-import %workspace%/user.bazelrc`, so host-specific startup options go in a
gitignored `user.bazelrc`. On distributions where Bazel's bundled JDK has no usable truststore, every
invocation otherwise fails to reach the Bazel Central Registry with a PKIX error:

```text
startup --host_jvm_args=-Djavax.net.ssl.trustStore=/etc/pki/ca-trust/extracted/java/cacerts
startup --host_jvm_args=-Djavax.net.ssl.trustStorePassword=changeit
```
