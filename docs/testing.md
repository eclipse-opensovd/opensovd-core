# Testing

pytest serves as the unified test driver, orchestrating tests across different tools:

- **cargo**: Rust unit and integration tests
- **pytest**: Python integration tests
- **Bruno**: API conformance tests

Run all tests:

```bash
uv run pytest
```

## Testing with pytest

### Prerequisites

1. Install [uv](https://docs.astral.sh/uv/getting-started/installation/)
2. Install dependencies: `uv sync`

### Options

```bash
# Build and run the gateway (default)
uv run pytest

# Use pre-built binary
uv run pytest --opensovd-binary target/release/opensovd-gateway

# Test Docker image
uv run pytest --opensovd-docker ghcr.io/eclipse-opensovd/opensovd-gateway:latest
```

### Example Test

The `gateway` fixture automatically builds and spawns the gateway process, waits for it to be ready, and provides an HTTP client for making requests. It tears down the process after the test completes.

```python
import signal

def test_graceful_shutdown(gateway):
    response = gateway.get("/version-info")
    assert response.status_code == 200

    gateway.process.send_signal(signal.SIGINT)
    # wait_for blocks until the pattern appears in stdout (times out if not found)
    gateway.wait_for("Shutdown complete")
```

## Testing with Bruno

[Bruno](https://www.usebruno.com/) is used for interactive API testing and conformance testing against the SOVD specification.

> **Note:** pytest invokes the Bruno CLI (`bru run`) as a subprocess when running Bruno tests. See [`tests/bruno/conftest.py`](../tests/bruno/conftest.py) for the implementation.
>
> **Note:** Bruno tests are automatically skipped when the `bru` CLI is not installed. Install it with `npm install -g @usebruno/cli` to enable them.

### Interactive API Testing

1. Download and install Bruno from [usebruno.com](https://www.usebruno.com/downloads)
2. Open Bruno and select **Open Collection**
3. Navigate to [`tests/bruno/`](../tests/bruno/) and open the collection
4. Select the **local** environment from [`environments/local.bru`](../tests/bruno/environments/local.bru)
5. Run individual requests (e.g., [`version-info/`](../tests/bruno/version-info/)) or the entire collection

### CLI Testing

Run all tests:

```bash
cd tests/bruno
bru run --env local
```

Run a single test:

```bash
bru run version-info/get-version-info.bru --env local
version-info/get-version-info (200 OK) - 48 ms
Assertions
   ✓ res.status: eq 200
   ✓ res.headers['content-type']: contains application/json
   ✓ res.body.sovd_info: isDefined
   ✓ res.body.sovd_info[0].version: eq {{sovd_version}}
   ✓ res.body.sovd_info[0].base_uri: eq {{versioned_url}}

📊 Execution Summary
┌───────────────┬──────────────┐
│ Metric        │    Result    │
├───────────────┼──────────────┤
│ Status        │    ✓ PASS    │
├───────────────┼──────────────┤
│ Requests      │ 1 (1 Passed) │
├───────────────┼──────────────┤
│ Assertions    │     5/5      │
└───────────────┴──────────────┘
```

## Testing with curl

### Setup

Start the gateway with mock entities (default: `localhost:7690`). Mock data is provided by the shared `opensovd-mocks` crate:

```bash
cargo run -p opensovd-gateway -- --mock
```

### API Examples

```bash
# List all components
curl -s http://localhost:7690/sovd/v1/components | jq
{
  "items": [
    {
      "id": "ecu",
      "name": "Engine Control Unit",
      "translation_id": "ecu.name",
      "href": "http://localhost:7690/sovd/v1/components/ecu",
      "tags": ["powertrain", "critical"]
    },
    {
      "id": "gateway",
      "name": "Vehicle Gateway",
      "href": "http://localhost:7690/sovd/v1/components/gateway",
      "tags": ["network"]
    }
  ]
}

# List data items for a component
curl -s http://localhost:7690/sovd/v1/components/gateway/data | jq
{
  "items": [
    { "id": "sw.version", "name": "Software Version", "category": "identData" },
    { "id": "sw.build_date", "name": "Build Date", "category": "identData" },
    { "id": "sw.sha1", "name": "Git SHA1", "category": "identData" },
    { "id": "hw.version", "name": "Hardware Version", "category": "identData" },
    { "id": "hw.revision", "name": "Hardware Revision", "category": "identData" },
    { "id": "hw.sn", "name": "Hardware Serial Number", "category": "identData" },
    { "id": "uptime", "name": "System Uptime", "category": "currentData" }
  ]
}

# Read a specific data item
curl -s http://localhost:7690/sovd/v1/components/gateway/data/sw.version | jq
{
  "id": "sw.version",
  "data": {
    "value": "0.1.0-mock"
  }
}

# Read data with schema included
curl -s http://localhost:7690/sovd/v1/components/gateway/data/sw.version?include-schema=true | jq
{
  "id": "sw.version",
  "data": {
    "value": "0.1.0-mock"
  },
  "schema": {
    "$schema": "https://json-schema.org/draft/2020-12/schema",
    "title": "Value_for_String",
    "type": "object",
    "properties": {
      "value": {
        "type": "string"
      }
    },
    "required": ["value"]
  }
}
```

### Schema Validation

Validate that data responses conform to their JSON schemas using the Python `jsonschema` CLI:

```bash
uv tool install jsonschema
```

Validate all data for all components:

```bash
BASE=http://localhost:7690/sovd/v1
for comp in $(curl -s $BASE/components | jq -r '.items[].id'); do
  echo "Component: $comp"
  for id in $(curl -s $BASE/components/$comp/data | jq -r '.items[].id'); do
    resp=$(curl -s "$BASE/components/$comp/data/${id}?include-schema=true")
    echo "$resp" | jq '.data' | jsonschema <(echo "$resp" | jq '.schema') 2>/dev/null && echo "  ${id}: VALID"
  done
done
```

## Testing with opensovd-cli

The example client exercises the `opensovd-client` API against a running gateway. It supports three transport types: TCP, Unix socket (filesystem path), and abstract Unix socket (Linux only).

### HTTP (TCP)

Start the gateway and run the client:

```bash
# Terminal 1 – start the gateway
cargo run --bin opensovd-gateway -- --mock

# Terminal 2 – run the example client
cargo run --example client
component: ecu (Engine Control Unit)
  data: voltage (Battery Voltage)
  data: temperature (Engine Temperature)
  data: sw.version (Software Version)
  data: sw.build_date (Build Date)
  data: sw.sha1 (Git SHA1)
  data: hw.version (Hardware Version)
  data: hw.revision (Hardware Revision)
  data: hw.sn (Hardware Serial Number)
component: gateway (Vehicle Gateway)
app: engine_control (Engine Control Application)
app: diagnostics (Diagnostic Services)
app: ota_manager (OTA Update Manager)
area: powertrain (Powertrain Domain)
area: network (Network Domain)
```

### Unix Socket

```bash
# Terminal 1
cargo run --bin opensovd-gateway -- --mock --unix-socket /tmp/opensovd.sock

# Terminal 2
cargo run --example client -- --unix-socket /tmp/opensovd.sock --url http://localhost/sovd/v1
```

<!-- Output is identical to the HTTP example above. -->

### Abstract Unix Socket (Linux only)

```bash
# Terminal 1
cargo run --bin opensovd-gateway -- --mock --unix-socket @opensovd

# Terminal 2
cargo run --example client -- --unix-socket @opensovd --url http://localhost/sovd/v1
```

<!-- Output is identical to the HTTP example above. -->

> **Note:** When using Unix sockets the `--url` flag is still required to set the base path (`/sovd/v1`); the host portion is ignored since the connection goes through the socket.

## Testing with cargo

Run Rust unit and integration tests directly:

> **Note:** pytest invokes `cargo test` as a subprocess when running the full test suite. See [`tests/test_rust.py`](../tests/test_rust.py) for the implementation.

```bash
# Run all Rust tests
cargo test

# Run tests for a specific crate
cargo test -p opensovd-gateway
```

| Type        | Location      | Description                   |
|-------------|---------------|-------------------------------|
| Unit        | `**/src/**`   | In-module tests               |
| Integration | `**/tests/**` | Crate-level integration tests |
