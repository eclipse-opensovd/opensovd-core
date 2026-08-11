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
use std::time::Duration;

use opensovd_client::{Client, Error, RetryPolicy, SovdInfo, VendorInfo};

# async fn run() -> Result<(), Box<dyn std::error::Error>> {
// Point at the SOVD server root (no version identifier).
let discovery = Client::builder()
    .base_uri("http://localhost:7690/sovd")?
    .timeout(Duration::from_secs(5))
    .retry(RetryPolicy::new(3).backoff(Duration::from_millis(200)))
    .discovery()?;

// See what the server advertises.
for info in discovery.versions::<VendorInfo>().await? {
    println!("{} -> {}", info.version, info.base_uri.0);
}

// Select a version and exercise it (or match on `vendor_info` via the `V` payload).
let client = discovery.select(|s: &SovdInfo<VendorInfo>| s.version == "1.1").await?;
match client.list_components().send().await {
    Ok(list) => {
        for c in &list.data.items {
            println!("component: {} ({})", c.id, c.name);
        }
        println!("{} components", list.data.items.len());
    }
    Err(Error::Timeout(d)) => eprintln!("request timed out after {d:?}"),
    Err(other) => return Err(other.into()),
}
# Ok(())
# }
```

If no timeout is configured, requests have no deadline. Without a retry policy,
failures are returned immediately. Retries are opt-in via [`RetryPolicy`] and
apply only to GET requests; retryable conditions are transport errors, HTTP
502/503/504, and per-attempt timeouts.

On Unix, `Discovery::connect_unix` / `connect_unix_abstract` reach `version-info`
over a Unix domain socket. A runnable example lives in `examples/client`.

To combine Unix sockets with builder configuration such as request timeouts,
switch the builder transport explicitly:

```rust,no_run
use opensovd_client::Client;

# #[cfg(unix)]
use std::time::Duration;

# async fn run() -> Result<(), Box<dyn std::error::Error>> {
# #[cfg(unix)]
# {
let client = Client::builder()
    .base_uri("http://localhost/sovd/v1")?
    .timeout(Duration::from_secs(5))
    .unix_socket("/run/sovd.sock")
    .build()?;

let _components = client.list_components().send().await?;
# }
# Ok(())
# }
```

Part of [OpenSOVD Core](https://github.com/eclipse-opensovd/opensovd-core).
