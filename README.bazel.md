<!--
SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
SPDX-License-Identifier: Apache-2.0
-->

# Bazel Notes

This repository now has a native Bazel build based on `rules_rust` and `crate_universe`.

## What Was Added

- Bazel defaults in `.bazelrc`.
- Bzlmod module definition in `MODULE.bazel` with `rules_rust` and `crate_universe`.
- Root aliases and a root test suite in `BUILD.bazel`.
- Package-local `BUILD.bazel` files for Rust crates and examples.
- Generated `cargo-bazel-lock.json` used by `crate_universe`.

## Important Repository Details

- Bazel is pinned to `8.3.0` via `.bazelversion`.
- The setup is Bzlmod-based (`--enable_bzlmod`).
- `crate_universe` uses isolated extension usage to avoid module collisions in downstream Bazel integrations.

## S-CORE Compatibility

This Bazel setup is designed to be consumable from larger Bazel graphs (including S-CORE-style integration) without crate universe name clashes:

- `use_extension(..., isolate = True)` is enabled for `crate_universe` in `MODULE.bazel`.
- `--experimental_isolated_extension_usages` is enabled in `.bazelrc`.
- All workspace Cargo manifests are tracked in `crate.from_cargo(... manifests = [...])`.

## Validated Commands

The following command is validated and succeeds:

```bash
bazel build //opensovd-core:opensovd_core
```

Useful root aliases from [BUILD.bazel](/home/ioan/opensovd-fork/opensovd-core/BUILD.bazel):

```bash
bazel build //:opensovd-core
bazel build //:opensovd-gateway
bazel build //:opensovd-examples-client
bazel build //:opensovd-examples-server
bazel test //:tests
```

## When Dependencies Change

If you change Rust dependencies or add a new Cargo manifest, repin `crate_universe`:

```bash
CARGO_BAZEL_REPIN=1 bazel sync --only=crate_index
```

This refreshes Bazel's generated crate graph in `cargo-bazel-lock.json`.

## Adding Targets

For a new binary/example target:

1. Add the source and update the owning crate `Cargo.toml`.
2. Add a matching Bazel target in the crate's `BUILD.bazel`.
3. If dependencies changed, repin with `CARGO_BAZEL_REPIN=1 bazel sync --only=crate_index`.

Example `rust_binary` pattern:

```starlark
rust_binary(
    name = "my_feature_example",
    srcs = ["my_feature/my_feature.rs"],
    aliases = aliases(normal = True, proc_macro = True),
    crate_features = ["tls"],  # only if your example needs that feature
    crate_name = "my_feature",
    crate_root = "my_feature/my_feature.rs",
    edition = "2024",
    proc_macro_deps = all_crate_deps(proc_macro = True),
    version = "0.1.1",
    deps = COMMON_DEPS,
)
```
