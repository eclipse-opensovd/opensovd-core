<!-- SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation -->
<!-- SPDX-License-Identifier: Apache-2.0 -->

# opensovd-client

> Async HTTP client for the SOVD (ISO 17978-3) API.

- `Client` is bound to a single SOVD API version: it targets a version-specific
  `base_uri` and exposes the resources (components, apps, areas, data).
- `Discovery` is the version-agnostic entry point: built from the server root, it
  reads the unversioned `version-info` endpoint, lists the advertised versions, and
  hands you a `Client` for one of them, reusing the same transport (custom connector,
  TLS, tower layers).

## Usage

```rust,no_run
use opensovd_client::{Client, SovdInfo, VendorInfo};

# async fn run() -> Result<(), Box<dyn std::error::Error>> {
// Point at the SOVD server root (no version identifier).
let discovery = Client::builder()
    .base_uri("http://localhost:7690/sovd")?
    .discovery()?;

// See what the server advertises.
for info in discovery.versions::<VendorInfo>().await? {
    println!("{} -> {}", info.version, info.base_uri.0);
}

// Select a version and exercise it (or match on `vendor_info` via the `V` payload).
let client = discovery.select(|s: &SovdInfo<VendorInfo>| s.version == "1.1").await?;
for c in &client.list_components().send().await?.data.items {
    println!("component: {} ({})", c.id, c.name);
}
# Ok(())
# }
```

On Unix, `Discovery::connect_unix` / `connect_unix_abstract` reach `version-info`
over a Unix domain socket. A runnable example lives in `examples/client`.

Part of [OpenSOVD Core](https://github.com/eclipse-opensovd/opensovd-core).
