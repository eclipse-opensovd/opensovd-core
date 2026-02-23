<!--
SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
SPDX-License-Identifier: Apache-2.0
-->

# OpenSOVD Server

> HTTP server implementing the SOVD (Service-Oriented Vehicle Diagnostics) API.

## Features

- `jsonschema` (default) - JSON Schema support

## Example

```rust,no_run
use opensovd_core::Topology;
use opensovd_server::Server;
use tokio::net::TcpListener;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind("127.0.0.1:7690").await?;
    let topology = Topology::default();

    let server = Server::builder()
        .base_uri("http://127.0.0.1:7690/sovd")?
        .listener(listener)
        .topology(topology)
        .build()?;

    server.serve().await?;
    Ok(())
}
```

## Contributing

See [CONTRIBUTING.md](../CONTRIBUTING.md) for guidelines.

## License

This project is licensed under the Apache License 2.0.
