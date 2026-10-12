<!--
SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
SPDX-License-Identifier: Apache-2.0
-->

# OpenSOVD Gateway

> HTTP gateway server for OpenSOVD vehicle diagnostics.

[![CI](https://github.com/eclipse-opensovd/opensovd-core/actions/workflows/ci.yaml/badge.svg?event=push&branch=main)](https://github.com/eclipse-opensovd/opensovd-core/actions/workflows/ci.yaml?query=event%3Apush+branch%3Amain)
[![Coverage](https://img.shields.io/endpoint?url=https://eclipse-opensovd.github.io/opensovd-core/gateway/x86_64-unknown-linux-gnu/coverage/badge.json)](https://eclipse-opensovd.github.io/opensovd-core/gateway/x86_64-unknown-linux-gnu/coverage/html/)
[![GHCR](https://img.shields.io/badge/ghcr.io-opensovd--gateway-blue?logo=github)](https://ghcr.io/eclipse-opensovd/opensovd-gateway)
[![Chat](https://img.shields.io/badge/chat-slack-blue?logo=slack)](https://app.slack.com/client/T02MS1M89UH/C0958MQNGP2)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue)](../../LICENSE)
[![Good First Issues](https://img.shields.io/github/issues-search/eclipse-opensovd/opensovd-core?query=is%3Aopen%20label%3A%22good%20first%20issue%22%20label%3Abin%3Agateway&label=good%20first%20issues&color=blue)](https://github.com/eclipse-opensovd/opensovd-core/issues?q=is%3Aopen%20label%3A%22good%20first%20issue%22%20label%3Abin%3Agateway)

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
| `--mdns ID`        | Vehicle identification to announce, e.g. the VIN              |
| `--mdns-host`      | Host label published as `HOST.local` (default: derived from the identification) |
| `--mdns-interface` | Announce only on these interfaces (comma-separated)           |

### Auth Options

Tokens are verified with the JSON Web Key Set from `--auth-jwks` and must name their key with `kid`; with `--auth-audience`, their `aud` must name one of the audiences. Only RSA keys of 2048 to 8192 bits, P-256, P-384 and Ed25519 keys are used, others are skipped with a warning; a symmetric or private key, two usable keys with the same `kid` or no usable key stops the gateway. Without `--auth-policy`, every valid token is allowed. The policy decides with `data.sovd.authz.allow` on `method` (HEAD reported as GET), `route` (the matched template relative to the mount path, e.g. `/v1/components/{component_id}`), decoded `params` and, for authenticated callers, `principal` (`subject`, `roles`, `scopes`, `claims`). See [the example policy](../../examples/server/auth/sovd_authz.rego).

| Option               | Description                                                                         |
|----------------------|-------------------------------------------------------------------------------------|
| `--auth-jwks`        | JSON Web Key Set file with the token signing keys (`SOVD_AUTH_JWKS`)                |
| `--auth-issuer`      | The `iss` claim tokens must carry; required with `--auth-jwks` (`SOVD_AUTH_ISSUER`) |
| `--auth-audience`    | Accepted `aud` claim, repeatable or comma-separated (`SOVD_AUTH_AUDIENCE`)          |
| `--auth-policy`      | Rego policy file, repeatable; requires `--auth-jwks` (`SOVD_AUTH_POLICY`)           |
| `--auth-policy-data` | JSON data file for the policy, repeatable (`SOVD_AUTH_POLICY_DATA`)                 |

## Contributing

See [CONTRIBUTING.md](../../CONTRIBUTING.md) for guidelines.

## License

This project is licensed under the Apache License 2.0.
