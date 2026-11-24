<!--
SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
SPDX-License-Identifier: Apache-2.0
-->

# Bazel Notes

This repository provides a native Bazel build based on rules_rust and
crate_universe, aligned with the S-CORE Bazel integration approach used by
other Eclipse OpenSOVD components.

## Build Configuration

The Bazel setup uses:

- Bazel 8.6.0, pinned through `.bazelversion`.
- Bzlmod through `MODULE.bazel`.
- The S-CORE Bazel registry followed by the Bazel Central Registry.
- `rules_rust` version 0.68.1-score.
- Rust edition 2024.
- Rust nightly 2026-05-07, matching `rust-toolchain.toml`.
- Isolated `crate_universe` extension usage.
- Explicit Cargo dependency selection through `crate_deps()`.
- Shared Rust target wrappers from `bazel/rust_crate.bzl`.

The relevant .bazelrc configuration is:

```text
common --registry=<https://raw.githubusercontent.com/eclipse-score/bazel_registry/main/>
common --registry=<https://bcr.bazel.build>
common --experimental_isolated_extension_usages
common --@rules_rust//rust/toolchain/channel=nightly

build --incompatible_strict_action_env
```

## Crate Universe

Cargo dependencies are resolved from the workspace root:

```starlark
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
```

The root Cargo.toml defines the workspace members, so individual workspace
manifests do not need to be listed separately in MODULE.bazel.

Cargo.lock remains the Cargo dependency lockfile used by the Bazel dependency
graph.

## Rust Target Wrappers

Workspace Rust targets should use the wrappers defined in
`bazel/rust_crate.bzl` instead of directly depending on all Cargo crates.

Available wrappers include:

- `workspace_rust_library`
- `workspace_rust_binary`
- `workspace_cargo_build_script`

Third-party Cargo dependencies are declared explicitly through `crate_deps` or
`proc_macro_crate_deps`, while internal OpenSOVD targets are declared through
normal Bazel deps.

Example:

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

This keeps the Bazel dependency graph explicit and consistent with the Cargo
manifest instead of exposing every Cargo dependency to every Rust target.

## Gateway Target

The root `BUILD.bazel` exposes `//:opensovd-gateway` as the public executable.

```bash
bazel build //:opensovd-gateway
```

For an application with its own in-process providers, the public Rust library
`@opensovd_core//:opensovd-gateway-lib` exposes the same gateway runner as the
binary. Construct a topology, parse gateway options with
`opensovd_gateway::cli::try_parse_from`, and pass both to
`opensovd_gateway::run`. The runner supports the same `SOVD_*` settings,
federation, authentication, TLS and mDNS as the binary; it does not initialize
tracing. Use `run_with_shutdown` to supply a shutdown future. A caller-supplied
topology cannot be combined with `--mock`.

The Bazel library enables mDNS as well as the other gateway features. Consumers
can construct a topology using `@opensovd_core//:opensovd-core`,
`@opensovd_core//:opensovd-models` and `@opensovd_core//:opensovd-providers`.
They must provide compatible Rust and C/C++ toolchains.

The root package also exposes `opensovd-client`, `opensovd-server`, and
`opensovd-extra`, plus shared third-party crates such as `tokio`, `axum`, and
`jsonwebtoken`, as public aliases. Using the same targets preserves compatible
Rust crate identities when exchanging types with the gateway.

For race-free listener setup, `run_with_listener` takes a bound TCP listener
and a shutdown future. It uses the supplied listener instead of binding from
the URL, using a Unix socket, or using systemd socket activation. The default
URL is `http://127.0.0.1:7690/sovd`; set `--url` or `SOVD_URL` to override it.

## Reusable HTTP Upstream Proxy

The gateway's HTTP proxy is exported as `opensovd_extra::UpstreamProxy`. Cargo
consumers enable `http-proxy`, which also enables `federation`:

```toml
opensovd-extra = { git = "https://github.com/eclipse-opensovd/opensovd-core", rev = "<revision-containing-the-extraction>", default-features = false, features = ["http-proxy"] }
```

```rust
use std::time::Duration;
use opensovd_extra::{Federation, Upstream, UpstreamProxy};

let base = "http://cda:20002/vehicle/v15".parse()?;
let proxy = UpstreamProxy::new(&base, Duration::from_secs(5))?;
let upstream = Upstream::new(proxy.into_service());
let layer = Federation::new(vec![upstream], topology)?.base("/sovd").into_layer();
// Install `layer` around the server's routes, inside any authorization layer.
```

Federation supplies the existing `ServerUri` request extension used to rewrite
`Location` and JSON string values. Direct service users can insert that extension
themselves. Transport behavior is unchanged: plain HTTP only, a connection timeout
(not a request timeout), bounded JSON decoding/rewriting, and streaming non-JSON
responses. Pass an `http://` base URI without a query; the existing constructor
uses its authority and path only. Configure routes and discovery timeouts on
`Upstream`, not on the proxy.

Bazel consumers add the public target to their Rust target's `deps`:

```starlark
deps = ["@opensovd_core//:opensovd-http-proxy"]
```

The equivalent package target is
`@opensovd_core//opensovd-extra:opensovd_extra_http_proxy`. Its Rust crate name is
`opensovd_extra`; it enables only `federation` and `http-proxy`, not authentication,
TLS, or tracing middleware. No visibility patches are needed for this target;
the workspace's full-featured extra target is also available as
`@opensovd_core//:opensovd-extra`. Consumers still provide their own compatible
Rust toolchain and any other libraries they use directly.

Proxy regression tests and a downstream-style federation usage test run with:

```bash
cargo test -p opensovd-extra --no-default-features --features http-proxy
bazel test //opensovd-extra:http_proxy_test //opensovd-extra:http_proxy_usage_test //opensovd-cli/gateway:upstream_config_test
```

## mDNS Announcer

The public `@opensovd_core//:opensovd-mdns` target exposes the mDNS announcer
as `opensovd_extra`. It enables only the `mdns` feature. When using it together
with the HTTP proxy in the same Rust target, give one dependency a distinct
Rust extern name using the `aliases` attribute:

```starlark
rust_binary(
    # ...
    aliases = {"@opensovd_core//:opensovd-mdns": "opensovd_extra_mdns"},
    deps = [
        "@opensovd_core//:opensovd-http-proxy",
        "@opensovd_core//:opensovd-mdns",
    ],
)
```

The proxy is then available as `opensovd_extra` and the announcer as
`opensovd_extra_mdns`. These are separate builds of the crate, so types from
one variant are not interchangeable with types from the other.

Run the announcer's unit tests and the combined-usage test with:

```bash
bazel test //opensovd-extra:mdns_test //opensovd-extra:mdns_http_proxy_usage_test
```

All Bazel targets can be inspected with:

```bash
bazel query //...
```

## Validated Commands

The following commands validate the Bazel integration:

```bash
bazel build //:opensovd-gateway
bazel query //...
```

For a clean validation run:

```bash
bazel clean --expunge
bazel build //:opensovd-gateway
bazel query //...
```

On systems where Bazel's embedded Java runtime does not trust the local
certificate chain, use the system JDK explicitly, for example:

```bash
bazel --server_javabase="$JAVA_HOME" build //:opensovd-gateway
bazel --server_javabase="$JAVA_HOME" query //...
```

This is a local environment workaround and should not be hardcoded into the
repository .bazelrc.

## When Dependencies Change

Rust dependencies continue to be managed through Cargo.

After changing dependencies in a Cargo.toml, update Cargo.lock with Cargo
as usual and validate the Bazel graph again:

```bash
cargo check
bazel build //:opensovd-gateway
```

Because crate_universe reads the root workspace manifest and Cargo.lock,
there is no separate Bazel-specific Cargo lockfile to maintain.

## Maintaining the Gateway Target

When gateway code or its transitive dependencies change, update the relevant
package-local BUILD.bazel targets and validate the gateway target:

```bash
bazel build //:opensovd-gateway
bazel query //...
```

## S-CORE Compatibility

The Bazel setup is designed to be consumable from larger Bazel graphs,
including S-CORE-style integrations.

The main compatibility points are:

- S-CORE registry configured before the Bazel Central Registry.
- Bazel compatibility declared as >=8.6.0.
- `rules_rust` uses the S-CORE-compatible 0.68.1-score release.
- `crate_universe` uses isolated extension usage.
- `--experimental_isolated_extension_usages` is enabled.
- Rust toolchains are registered through Bzlmod.
- Cargo dependencies are selected explicitly for individual Bazel targets.

OpenSOVD Core intentionally keeps its nightly Rust toolchain because the
workspace currently depends on nightly Rust functionality. This differs from
components that can use a stable Rust toolchain.
