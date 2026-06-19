# Online Capability Description Example

SOVD online capability description
ISO 17978-3: appending `/docs` to a SOVD endpoint returns a self-contained
OpenAPI 3.1 specification describing how to interact with that endpoint.
For the data API, this example exposes the collection endpoint
`/components/{component-id}/data/docs`.

## Topology

```text
SOVDServer
  └── Component: "engine"
        └── Data Provider
      ├── rpm                  (read-write, CurrentData)
      ├── coolant_temperature  (read-only, CurrentData)
      ├── battery_voltage      (read-only, SysInfo)
      └── serial_number        (read-only, IdentData)
```

## Running

```bash
cargo run -p opensovd-examples-server --example online_capability
```

The server starts on `http://127.0.0.1:7691`.

## Example Requests

```bash
# Retrieve the ONLINE CAPABILITY DESCRIPTION for the data collection
curl -s http://localhost:7691/sovd/v1/components/engine/data/docs | jq

# Read the data resource value
curl -s http://localhost:7691/sovd/v1/components/engine/data/rpm | jq

# Write a new value
curl -s -X PUT http://localhost:7691/sovd/v1/components/engine/data/rpm \
  -H 'Content-Type: application/json' \
  -d '{"data": {"value": 1500.0}}'

# Read another resource from the same collection
curl -s http://localhost:7691/sovd/v1/components/engine/data/coolant_temperature | jq
```

The collection-level `/data/docs` response documents the list endpoint,
including the `groups`, `categories`, `tags`, and `include-schema` query
parameters, and includes an example payload with the currently exposed metadata.

The response documents the collection endpoint in a self-contained way and
includes an example metadata payload, abbreviated below:

```json
{
  "openapi": "3.1.0",
  "info": { "title": "Data collection /components/engine/data", "version": "1.0.0" },
  "paths": {
    "/components/engine/data": {
      "get": {
        "summary": "List the data resources of the entity",
        "parameters": [
          { "name": "groups", "in": "query", "schema": { "type": "array", "items": { "type": "string" } } },
          { "name": "categories", "in": "query", "schema": { "type": "array", "items": { "type": "string" } } },
          { "name": "tags", "in": "query", "schema": { "type": "array", "items": { "type": "string" } } },
          { "name": "include-schema", "in": "query", "schema": { "type": "boolean" } }
        ],
        "responses": {
          "200": {
            "content": {
              "application/json": {
                "example": {
                  "items": [
                    {
                      "id": "rpm",
                      "name": "Engine RPM set-point",
                      "category": "currentData",
                      "translation_id": "engine.rpm.setpoint",
                      "groups": ["actuators", "powertrain"],
                      "tags": ["live", "control"]
                    },
                    {
                      "id": "coolant_temperature",
                      "name": "Coolant Temperature",
                      "category": "currentData",
                      "translation_id": "engine.coolant.temperature",
                      "groups": ["sensors", "thermal"],
                      "tags": ["live", "safety"]
                    }
                  ]
                }
              }
            }
          }
        }
      }
    }
  }
}
```
