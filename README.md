<!--
SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
SPDX-License-Identifier: Apache-2.0
-->

# OpenSOVD Core

> Standardized diagnostic communication for connected vehicles.

[![CI](https://github.com/eclipse-opensovd/opensovd-core/actions/workflows/ci.yaml/badge.svg?event=push&branch=main)](https://github.com/eclipse-opensovd/opensovd-core/actions/workflows/ci.yaml?query=event%3Apush+branch%3Amain)
[![Coverage](https://eclipse-opensovd.github.io/opensovd-core/coverage/badge.svg)](https://eclipse-opensovd.github.io/opensovd-core/coverage/)
[![GHCR](https://img.shields.io/badge/ghcr.io-opensovd--gateway-blue?logo=github)](https://ghcr.io/eclipse-opensovd/opensovd-gateway)
[![Chat](https://img.shields.io/badge/chat-slack-blue?logo=slack)](https://app.slack.com/client/T02MS1M89UH/C0958MQNGP2)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue)](LICENSE)
[![Good First Issues](https://img.shields.io/github/issues/eclipse-opensovd/opensovd-core/good%20first%20issue?label=good%20first%20issues&color=blue)](https://github.com/eclipse-opensovd/opensovd-core/labels/good%20first%20issue)

Open-source implementation of the [ISO 17978-3:2026](https://www.iso.org/standard/86587.html) SOVD (Service-Oriented Vehicle Diagnostics) standard.

## Quick Start

```bash
# Run the gateway with mock data
docker run -p 7690:7690 ghcr.io/eclipse-opensovd/opensovd-gateway --mock

# Verify it's running
curl -s http://127.0.0.1:7690/sovd/version-info | jq
{
  "sovd_info": [
    {
      "version": "1.1",
      "base_uri": "http://127.0.0.1:7690/sovd/v1",
      "vendor_info": {
        "version": "0.1.1",
        "name": "OpenSOVD"
      }
    }
  ]
}
```

## Development

### Prerequisites

- [Rust](https://rustup.rs/) (version and components auto-configured via `rust-toolchain.toml`)
- [uv](https://docs.astral.sh/uv/) (optional) - Python package manager for running integration tests

> [!TIP]
> Open the project in a [Dev Container](https://containers.dev/) for a ready-to-use environment, or use [devenv](https://devenv.sh/) locally. See [Development docs](docs/development.md) for details.

```bash
# Build
cargo build

# Run the gateway with mock data
cargo run -p opensovd-gateway -- --mock
```

For testing instructions, see the [Testing guide](docs/testing.md).

## Examples

See [examples/](examples/) for usage samples.

## Documentation

- [Architecture](docs/architecture.md)
- [Development](docs/development.md)
- [Testing](docs/testing.md)
- [CI/CD](docs/ci.md)

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

## License

This project is licensed under the [Apache License 2.0](LICENSE).
