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

# Forward what the gateway does not serve to a classic diagnostic adapter (CDA)
opensovd-gateway --mock --upstream-url http://localhost:20002/vehicle/v15
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

### Upstream Options

The upstream is optional: without `--upstream-url`, or with an empty `SOVD_UPSTREAM_URL`, the gateway answers every request itself. With it, requests the gateway does not serve itself go to an upstream SOVD server, such as a [classic diagnostic adapter](https://github.com/eclipse-opensovd/classic-diagnostic-adapter) (CDA), after auth. The path below the gateway's API version, without a trailing slash, and the query, are appended to the URL: `/sovd/v1/components/flxc1000/faults?x=1` goes to `http://localhost:20002/vehicle/v15/components/flxc1000/faults?x=1`. Method, headers (but hop-by-hop ones) and body pass unchanged, and the upstream's response is not returned verbatim: its status and headers are kept, but its references to itself are replaced.
URLs of the upstream's API and its path, in `Location` and in the text of JSON bodies, including error messages, refer to the gateway, so clients discover the upstream's resources through the gateway and its auth. This also applies to diagnostic data in JSON that holds such a text.

`GET /components` and `GET /apps` list the upstream's entities next to the gateway's, whose entity wins when both have an id. If the upstream is unreachable or does not answer the list in time, the gateway lists its own entities.

`GET /` adds the links of the upstream's own root, pointing at the gateway, which keeps what it advertises itself. An upstream without a root adds nothing. The server-level resource collections, such as `/data` and `/faults`, go to the upstream, as the gateway holds none but `/updates` with an update provider.

| Request below `{base}/v1`                                          | Answered by |
|--------------------------------------------------------------------|-------------|
| `/components/{id}/**`, `/apps/{id}/**` with an `id` the gateway does not hold | upstream |
| `/functions`, `/functions/**`                                      | upstream |
| `/locks`, `/locks/{lock_id}` (vehicle level)                       | gateway, on all upstreams |
| `/authorize`                                                       | gateway, or the upstream of `--upstream-authorize-url` |
| `/x-*`, `/x-*/**` (custom collections)                             | upstream |
| Everything else, including all paths below held entities           | gateway     |

Without `--upstream-authorize-url`, which names one of the `--upstream-url`s, the gateway answers `/authorize` with 404, as clients get tokens from their issuer. The upstreams and their routes are logged at startup, at debug level. An unreachable upstream answers **502** with the `upstream-unreachable` vendor code. If no upstream has an entity and one of them does not answer in time, the gateway answers **504** with the `upstream-timeout` vendor code. With `--auth-policy`, a forwarded request on a path the gateway does not route has no `route` in the policy input.

A vehicle lock applies to everything the gateway serves from its upstreams, so it holds on all of them. `POST /locks` acquires a lock from each upstream, in the configured order, and answers with one id of the gateway. If an upstream refuses, including one without locks, or cannot be reached, the locks acquired so far are released and its answer is the answer. `GET`, `PUT` and `DELETE` go to all parts of the lock, and upstreams decide who owns one, by the client's token. If a part is gone, the lock is over: the other parts are released and the gateway answers **409** with `lock-broken`.
`break_lock` goes to all upstreams, and a part an upstream broke but keeps visible, as a CDA does, stays part of the lock: `GET` shows it, `PUT` answers the upstream's `lock-broken`, and the client's `DELETE` clears all parts. If an upstream refuses after another one broke its part, that part stays broken. The gateway enforces no locks itself: upstreams protect what they serve, while what the gateway answers itself, its held entities and `/updates`, stays open to other clients during a vehicle lock.

The gateway remembers in memory which locks of the upstreams make up a lock, but no tokens. After a restart it does not know its locks anymore and answers 404 for them, while the locks of the upstreams expire by themselves. `GET /locks` lists the locks acquired through the gateway only. A lock acquired directly from an upstream is not listed, but `POST /locks` fails on it. Entities the gateway serves itself are not locked yet.

With several upstreams, by repeating `--upstream-url` or separating the URLs by commas, all of them serve their entities and list them, the first one that has an id answers for it, and everything else in the table goes to the first upstream. Entities may come and go at runtime. The gateway passes the client's token on unchanged and stores none, so every upstream has to accept the same token and see the same `sub`, as locks are owned by it. How an upstream validates the token is up to its implementor. The locks of an entity stay with the entity.

| Option                  | Description                                                                 |
|-------------------------|-----------------------------------------------------------------------------|
| `--upstream-url`             | Base URL of an upstream API, `http://` only, repeatable or comma-separated (`SOVD_UPSTREAM_URL`, comma-separated only) |
| `--upstream-authorize-url`   | The `--upstream-url` that takes `/authorize` (`SOVD_UPSTREAM_AUTHORIZE_URL`)                     |
| `--upstream-connect-timeout` | Seconds to wait for a connection to the upstream, default 5 (`SOVD_UPSTREAM_CONNECT_TIMEOUT`) |
| `--upstream-discovery-timeout` | Seconds an upstream gets to answer `/`, `/components` and `/apps`, and the look for the owner of an entity, default 5 (`SOVD_UPSTREAM_DISCOVERY_TIMEOUT`). Other requests have no timeout |

### Auth Options

Tokens are verified with the JSON Web Key Set from `--auth-jwks` and must name their key with `kid`; with `--auth-audience`, their `aud` must name one of the audiences. Only RSA keys of 2048 to 8192 bits, P-256, P-384 and Ed25519 keys are used, others are skipped with a warning; a symmetric or private key, two usable keys with the same `kid` or no usable key stops the gateway. Without `--auth-policy`, every valid token is allowed. The policy decides with `data.sovd.authz.allow` on `method` (HEAD reported as GET), `route` (the matched template relative to the mount path, e.g. `/v1/components/{component_id}`), decoded `params` and, for authenticated callers, `principal` (`subject`, `roles`, `scopes`, `claims`). See [the example policy](../../examples/server/auth/sovd_authz.rego).

| Option               | Description                                                                         |
|----------------------|-------------------------------------------------------------------------------------|
| `--auth-jwks`        | JSON Web Key Set file with the token signing keys (`SOVD_AUTH_JWKS`)                |
| `--auth-issuer`      | The `iss` claim tokens must carry; required with `--auth-jwks` (`SOVD_AUTH_ISSUER`) |
| `--auth-audience`    | Accepted `aud` claim, repeatable or comma-separated (`SOVD_AUTH_AUDIENCE`)          |
| `--auth-policy`      | Rego policy file, repeatable; requires `--auth-jwks` (`SOVD_AUTH_POLICY`)           |
| `--auth-policy-data` | JSON data file for the policy, repeatable (`SOVD_AUTH_POLICY_DATA`)                 |

With gateway auth, whoever issues tokens has to be covered by `--auth-jwks`, `--auth-issuer` and `--auth-audience`, and `/authorize` is checked like any other request. The default security plugin of the CDA issues HS256 tokens without `kid`, which never pass.

### Development Tokens

Without an identity provider, `scripts/dev-token.sh` signs tokens for a stack of gateway and CDAs. It creates the key on first use, and the JWKS next to it, which goes to `--auth-jwks` and to every upstream that verifies tokens:

```bash
TOKEN=$(scripts/dev-token.sh keys dev-stack tester)
opensovd-gateway --upstream-url http://localhost:20002/vehicle/v15 \
  --auth-jwks keys/dev-jwks.json --auth-issuer dev-stack
curl -H "Authorization: Bearer $TOKEN" http://localhost:7690/sovd/v1/components
```

Anyone with the key can sign any token, so this is for development and testing only. To move to a real provider, point `--auth-jwks` and `--auth-issuer` at the provider.

The gateway passes the token on unchanged, so what a CDA does with it depends on the CDA. By default it does not verify signatures, so it accepts these tokens, and anyone who reaches it directly can forge one. To let it verify them, give it the same key set and issuer through its environment. This needs [CDA#696](https://github.com/eclipse-opensovd/classic-diagnostic-adapter/pull/696), and no build feature:

| CDA environment      | Value                                                       |
|----------------------|-------------------------------------------------------------|
| `CDA_AUTH_JWKS_FILE` | Path to `dev-jwks.json`, as the CDA sees it                 |
| `CDA_AUTH_ISSUER`    | The same value as `--auth-issuer`, here `dev-stack`         |
| `CDA_AUTH_AUDIENCE`  | Optional accepted `aud` values, comma-separated             |

With the CDA's testcontainer setup, `CDA_AUTH_JWKS_DIR` names the host directory, which is mounted at `/app/auth`:

```bash
export CDA_AUTH_JWKS_DIR=$PWD/keys
export CDA_AUTH_JWKS_FILE=/app/auth/dev-jwks.json
export CDA_AUTH_ISSUER=dev-stack
docker compose up
```

The CDA then answers its own `/authorize` with 403, and a broken key set stops it at startup. Every CDA behind the gateway needs these settings, as each one verifies tokens on its own.

## Contributing

See [CONTRIBUTING.md](../../CONTRIBUTING.md) for guidelines.

## License

This project is licensed under the Apache License 2.0.
