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

### TLS Options

| Option              | Description                                                        |
|---------------------|--------------------------------------------------------------------|
| `--tls-cert`        | Server certificate chain (PEM); requires an `https://` `--url`     |
| `--tls-key`         | Server private key (PEM)                                           |
| `--tls-client-ca`   | Client CA bundle (PEM), repeatable; enables mTLS                   |
| `--tls-client-auth` | `required` (default) rejects clients without a certificate; `optional` accepts them |

An `https://` `--url` without `--tls-cert` only advertises https, for a TLS-terminating proxy in front of the gateway.

### mDNS Options

Announces the gateway as `_sovd._tcp` with the `identification` and `accessurl` TXT records. Loopback addresses are rejected. `0.0.0.0` announces all IPv4 addresses, `[::]` all addresses the listener accepts. UDP port 5353 must be open.

| Option             | Description                                                   |
|--------------------|---------------------------------------------------------------|
| `--mdns ID`        | Vehicle identification to announce, e.g. the VIN (`off` disables) |
| `--mdns-host`      | Host label published as `HOST.local` (default: derived from the identification) |
| `--mdns-interface` | Announce only on these interfaces (comma-separated)           |

## Contributing

See [CONTRIBUTING.md](../../CONTRIBUTING.md) for guidelines.

## License

This project is licensed under the Apache License 2.0.
