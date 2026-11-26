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
- Crates from the root Cargo.toml and Cargo.lock through an OpenSOVD-specific
  `crate_universe` extension identity. There is no separate lockfile and no
  repin step.

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

## Gateway Target

The root `BUILD.bazel` exposes `//:opensovd-gateway` as the public executable.

```bash
bazel build //:opensovd-gateway
```

### Reproducible gateway build metadata

The gateway build script normally falls back to Git for its revision and date.
Git metadata is often unavailable in Bazel sandboxes, especially when this
repository is an external dependency. To supply reproducible metadata, define a
target that produces one environment file with `KEY=VALUE` lines, for example:

```text
OPENSOVD_COMMIT_SHA=f61eeeb4133e5c5a7c3541861489f12e306e4ee4
SOURCE_DATE_EPOCH=1704067200
```

Then point the public build setting at that target (which must be visible to
OpenSOVD):

```bash
bazel build //:my-gateway \
  --@opensovd_core//opensovd-cli/gateway:build_stamp=//my-package:stamp_env
```

The file is a declared build input. Changing it causes the build script to run
again. The default is an empty file, preserving the existing fallback behavior.
`SOURCE_DATE_EPOCH` is a Unix timestamp in seconds; it determines the UTC build
date. The caller is responsible for matching the revision and timestamp to the
OpenSOVD source being built.

Run the build-stamp regression check from the workspace root with
`bash opensovd-cli/gateway/tests/build_stamp_test.sh`.

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

The gateway's default URL is `http://127.0.0.1:7690/sovd`; set `--url` or
`SOVD_URL` to override it.

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

## S-CORE Compatibility

- Bazel compatibility declared as >=8.6.0.
- The crate universe uses a separate extension identity, so it can coexist
  with other modules' `crate_index` repositories without experimental flags.
- The Rust toolchain and the `rules_rust` override apply only to builds of
  this repository; an integrating module brings its own.
- `rules_rust` releases from 0.72.0 on require a crate_universe lockfile
  from non-root modules, which this module does not ship. Integrate it with
  an earlier release.

Run `python3 tests/bazel_consumer_test.py` to check that a downstream module
can use its own `crate_index` alongside OpenSOVD without experimental flags.
