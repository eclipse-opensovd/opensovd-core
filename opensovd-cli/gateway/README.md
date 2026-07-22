<!--
SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
SPDX-License-Identifier: Apache-2.0
-->

# OpenSOVD Gateway

> HTTP gateway server for OpenSOVD vehicle diagnostics.

Exposes [OpenSOVD](https://github.com/eclipse-opensovd/opensovd-core) diagnostic services over HTTP, implementing the [SOVD](https://www.iso.org/standard/86587.html) REST API.

## Usage

```bash
# Listen on localhost:7690 (default)
opensovd-gateway

# Listen on all interfaces
opensovd-gateway --url http://0.0.0.0:8080/sovd

# Listen on a Unix socket
opensovd-gateway --unix-socket /tmp/opensovd.sock

# Listen on an abstract Unix socket (Linux)
opensovd-gateway --unix-socket @opensovd

# Enable mock topology for testing
opensovd-gateway --mock

# Advertise the gateway and discover private-side SOVD services with mDNS
cargo run -p opensovd-gateway --features mdns -- \
	--url http://0.0.0.0:7690/sovd --mdns --mdns-host 192.168.1.10
```

Mock data comes from the shared `opensovd-mocks` crate used across examples and tests.

For mDNS architecture and gateway/private-network design, see [`docs/mdns/README.md`](../../docs/mdns/README.md).

## Options

| Option          | Description                                          |
|-----------------|------------------------------------------------------|
| `--url`         | Server URL with base URI path (default: `http://localhost:7690/sovd`) |
| `--unix-socket` | Unix socket path (`@` prefix for abstract sockets)   |
| `--mock`        | Enable mock entities for testing                     |
| `--serve-dir`   | Serve static files (`PATH:DIRECTORY`)                |

### mDNS Options

Build with `--features mdns` to enable these options.

| Option                   | Description |
|--------------------------|-------------|
| `--mdns`                 | Advertise the TCP gateway and discover private-side SOVD services |
| `--mdns-name NAME`       | mDNS service instance name (default: `opensovd`) |
| `--mdns-host IP`         | Externally reachable IP to advertise; required when binding to an unspecified address |
| `--mdns-identification ID` | Identification TXT record; defaults to `--mdns-name` |

### CORS Options

| Option               | Description                        |
|----------------------|------------------------------------|
| `--cors-origin`      | Allowed origins (`*` for any)      |
| `--cors-method`      | Allowed methods (`*` for any)      |
| `--cors-header`      | Allowed headers (`*` for any)      |
| `--cors-credentials` | Allow credentials                  |
| `--cors-max-age`     | Preflight cache duration (seconds) |

## Contributing

See [CONTRIBUTING.md](../../CONTRIBUTING.md) for guidelines.

## License

This project is licensed under the Apache License 2.0.
