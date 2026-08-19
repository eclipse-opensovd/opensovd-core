<!--
SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
SPDX-License-Identifier: Apache-2.0
-->

Bazel Notes

This repository provides a native Bazel build based on rules_rust and
crate_universe, aligned with the S-CORE Bazel integration approach used by
other Eclipse OpenSOVD components.

Build Configuration

The Bazel setup uses:

Bazel 8.6.0, pinned through .bazelversion.

Bzlmod through MODULE.bazel.

The S-CORE Bazel registry followed by the Bazel Central Registry.

rules_rust version 0.68.1-score.

Rust edition 2024.

Rust nightly 2026-05-07, matching rust-toolchain.toml.

Isolated crate_universe extension usage.

Explicit Cargo dependency selection through crate_deps().

Shared Rust target wrappers from bazel/rust_crate.bzl.

The relevant .bazelrc configuration is:

common --registry=https://raw.githubusercontent.com/eclipse-score/bazel_registry/main/
common --registry=https://bcr.bazel.build
common --experimental_isolated_extension_usages
common --@rules_rust//rust/toolchain/channel=nightly

build --incompatible_strict_action_env

Crate Universe

Cargo dependencies are resolved from the workspace root:

opensovd_crate_universe = use_extension(
    "@rules_rust//crate_universe:extensions.bzl",
    "crate",
    isolate = True,
)

opensovd_crate_universe.from_cargo(
    name = "crate_index",
    cargo_lockfile = "//:Cargo.lock",
    manifests = ["//:Cargo.toml"],
    skip_cargo_lockfile_overwrite = True,
)

use_repo(opensovd_crate_universe, "crate_index")

The root Cargo.toml defines the workspace members, so individual workspace
manifests do not need to be listed separately in MODULE.bazel.

Cargo.lock remains the Cargo dependency lockfile used by the Bazel dependency
graph.

Rust Target Wrappers

Workspace Rust targets should use the wrappers defined in
bazel/rust_crate.bzl instead of directly depending on all Cargo crates.

Available wrappers include:

workspace_rust_library

workspace_rust_binary

workspace_rust_test

workspace_cargo_build_script

Third-party Cargo dependencies are declared explicitly through crate_deps or
proc_macro_crate_deps, while internal OpenSOVD targets are declared through
normal Bazel deps.

Example:

load("//bazel:rust_crate.bzl", "workspace_rust_binary")

workspace_rust_binary(
    name = "my_feature_example",
    srcs = ["my_feature/my_feature.rs"],
    crate_deps = [
        "tokio",
        "tracing",
    ],
    proc_macro_crate_deps = [
        "async-trait",
    ],
    crate_name = "my_feature",
    crate_root = "my_feature/my_feature.rs",
    version = "0.1.1",
    deps = [
        "//opensovd-core:opensovd_core",
    ],
)

This keeps the Bazel dependency graph explicit and consistent with the Cargo
manifest instead of exposing every Cargo dependency to every Rust target.

Root Targets

The root BUILD.bazel exposes aliases for the main workspace crates and
binaries.

Examples:

bazel build //:opensovd-core
bazel build //:opensovd-gateway
bazel build //:opensovd-mcp
bazel build //:opensovd-examples-client
bazel build //:opensovd-examples-server
bazel build //:opensovd-benches

The complete workspace can be built with:

bazel build //:workspace

The complete Bazel test suite can be run with:

bazel test //:tests

All Bazel targets can be inspected with:

bazel query //...

Validated Commands

The following commands are used for full validation:

bazel build //:workspace
bazel test //:tests
bazel query //...

For a clean validation run:

bazel clean --expunge
bazel build //:workspace
bazel test //:tests
bazel query //...

On systems where Bazel's embedded Java runtime does not trust the local
certificate chain, use the system JDK explicitly, for example:

bazel --server_javabase="$JAVA_HOME" build //:workspace
bazel --server_javabase="$JAVA_HOME" test //:tests
bazel --server_javabase="$JAVA_HOME" query //...

This is a local environment workaround and should not be hardcoded into the
repository .bazelrc.

When Dependencies Change

Rust dependencies continue to be managed through Cargo.

After changing dependencies in a Cargo.toml, update Cargo.lock with Cargo
as usual and validate the Bazel graph again:

cargo check
bazel build //:workspace
bazel test //:tests

Because crate_universe reads the root workspace manifest and Cargo.lock,
there is no separate Bazel-specific Cargo lockfile to maintain.

Adding New Targets

For a new library, binary, example, test, or build script:

Add the source files and update the owning Cargo.toml.

Add a Bazel target in the owning BUILD.bazel.

Use the appropriate wrapper from bazel/rust_crate.bzl.

Declare only the required third-party crates in crate_deps.

Declare proc-macro crates in proc_macro_crate_deps.

Declare internal OpenSOVD targets through normal Bazel deps.

Run the full workspace validation.

Example validation:

bazel build //:workspace
bazel test //:tests
bazel query //...

S-CORE Compatibility

The Bazel setup is designed to be consumable from larger Bazel graphs,
including S-CORE-style integrations.

The main compatibility points are:

S-CORE registry configured before the Bazel Central Registry.

Bazel compatibility declared as >=8.6.0.

rules_rust uses the S-CORE-compatible 0.68.1-score release.

crate_universe uses isolated extension usage.

--experimental_isolated_extension_usages is enabled.

Rust toolchains are registered through Bzlmod.

Cargo dependencies are selected explicitly for individual Bazel targets.

OpenSOVD Core intentionally keeps its nightly Rust toolchain because the
workspace currently depends on nightly Rust functionality. This differs from
components that can use a stable Rust toolchain.