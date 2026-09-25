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

# Announce via mDNS on the diagnostic port
opensovd-gateway --url http://0.0.0.0:7690/sovd --mdns ABC123456789 --mdns-interface eth1
```

Mock data comes from the shared `opensovd-mocks` crate used across examples and tests.

## Options

| Option          | Description                                          |
|-----------------|------------------------------------------------------|
| `--url`         | Server URL with base URI path (default: `http://localhost:7690/sovd`) |
| `--unix-socket` | Unix socket path (`@` prefix for abstract sockets)   |
| `--mock`        | Enable mock entities for testing                     |
| `--serve-dir`   | Serve static files (`PATH:DIRECTORY`)                |

### CORS Options

| Option               | Description                        |
|----------------------|------------------------------------|
| `--cors-origin`      | Allowed origins (`*` for any)      |
| `--cors-method`      | Allowed methods (`*` for any)      |
| `--cors-header`      | Allowed headers (`*` for any)      |
| `--cors-credentials` | Allow credentials                  |
| `--cors-max-age`     | Preflight cache duration (seconds) |

### mDNS Options

Announces the gateway as `_sovd._tcp` with the `identification` and `accessurl` TXT records of ISO 17978-3. Loopback addresses are rejected. `0.0.0.0` announces all IPv4 addresses, `[::]` all addresses the listener accepts. UDP port 5353 must be open.

| Option                  | Description                                                   |
|-------------------------|---------------------------------------------------------------|
| `--mdns ID`             | Announce with this vehicle identification, e.g. the VIN. Empty, `false`, `0`, `no` or `off` disable it |
| `--mdns-host`           | Host label published as `HOST.local` (default: identification with spaces and underscores turned into hyphens and other characters dropped, or the system host name if nothing is left) |
| `--mdns-interface`      | Announce only on these interfaces, repeatable or comma-separated. With a specific `--url` IP, the list must include that IP's interface, and only that interface is used |

## Contributing

See [CONTRIBUTING.md](../../CONTRIBUTING.md) for guidelines.

## License

This project is licensed under the Apache License 2.0.
