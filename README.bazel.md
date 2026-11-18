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

`//:opensovd-gateway` is the gateway binary target. Reusable library targets
and proxy tests are described below. For Cargo tests, see the
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
the workspace's full-featured extra target and unrelated internal targets remain
private. Consumers still provide their own compatible Rust toolchain and any
other libraries they use directly. Do not depend on both variants of
`opensovd_extra` in the same Rust target.

Proxy regression tests and a downstream-style federation usage test run with:

```bash
cargo test -p opensovd-extra --no-default-features --features http-proxy
bazel test //opensovd-extra:http_proxy_test //opensovd-extra:http_proxy_usage_test //opensovd-cli/gateway:upstream_config_test
```

## S-CORE Compatibility

- Bazel compatibility declared as >=8.6.0.
- `--experimental_isolated_extension_usages` is enabled.
- The Rust toolchain and the `rules_rust` override apply only to builds of
  this repository; an integrating module brings its own.
- `rules_rust` releases from 0.72.0 on require a crate_universe lockfile
  from non-root modules, which this module does not ship. Integrate it with
  an earlier release.
