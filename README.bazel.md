<!--
SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
SPDX-License-Identifier: Apache-2.0
-->

# Bazel Notes

The gateway builds as a native Bazel target through rules_rust and
crate_universe, aligned with the S-CORE Bazel integration approach used by
other Eclipse OpenSOVD components.

```bash
bazel build //:opensovd-gateway
bazel query //...
```

`//:opensovd-gateway` is the public target; package-local targets are
internal. There are no Bazel test targets. Tests run through Cargo, see the
[Testing guide](docs/testing.md).

## Build Configuration

- Bazel 8.6.0, pinned through `.bazelversion`.
- The S-CORE Bazel registry followed by the Bazel Central Registry.
- `rules_rust` 0.70.0 as the declared dependency. Builds of this repository
  use a later upstream commit through `git_override`.
- The Rust version from `rust-toolchain.toml`, edition 2024.
- Crates from the root Cargo.toml and Cargo.lock through an isolated
  `crate_universe` extension. There is no separate lockfile and no repin
  step.

## Rust Target Wrappers

Workspace targets use the wrappers in `bazel/rust_crate.bzl`. Cargo
dependencies are named through `crate_deps` and `proc_macro_crate_deps`,
workspace targets through `deps`:

```starlark
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
```

`crate_deps` and `proc_macro_crate_deps` resolve names against the Cargo
manifest next to the BUILD.bazel file, so the package has to be a workspace
member whose Cargo.toml lists those crates. When a crate's dependencies
change, update its BUILD.bazel.

## S-CORE Compatibility

- Bazel compatibility declared as >=8.6.0.
- `--experimental_isolated_extension_usages` is enabled.
- The Rust toolchain and the `rules_rust` override apply only to builds of
  this repository; an integrating module brings its own.
- `rules_rust` releases from 0.72.0 on require a crate_universe lockfile
  from non-root modules, which this module does not ship. Integrate it with
  an earlier release.
