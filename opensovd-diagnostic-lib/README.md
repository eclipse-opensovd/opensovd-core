# opensovd-diagnostic-lib

A Rust library for embedding diagnostic data exposure into HPC applications. Your app implements one trait, gets an HTTP API for free, and can self-register with a SOVD server at startup.

## How it works

You implement `DataProvider` — three async methods: list your data items, read a value, write a value. The library runs an HTTP server on whatever port you choose and handles all routing, serialization and error mapping.

```text
Your app
  └─ implements DataProvider
       └─ DiagnosticServer exposes /api/data/* on a local port
            └─ SOVD server proxies it through GenericHttpProxy
```

## Usage

```rust
use opensovd_diagnostic_lib::{
    DataCategory, DataItem, DataProvider, DataValue, DiagnosticError,
    DiagnosticServer, Result,
};
use async_trait::async_trait;

struct MyProvider;

#[async_trait]
impl DataProvider for MyProvider {
    async fn list_data(&self) -> Vec<DataItem> {
        vec![DataItem {
            id: "sensor.temp".to_string(),
            name: "Temperature".to_string(),
            category: DataCategory::CurrentData,
            translation_id: None,
            groups: vec![],
            tags: vec![],
            schema: None,
            is_readable: true,
            is_writable: false,
        }]
    }

    async fn read_data(&self, id: &str) -> Result<DataValue> {
        match id {
            "sensor.temp" => Ok(DataValue { value: serde_json::json!(42.5), schema: None }),
            _ => Err(DiagnosticError::NotFound(id.to_string())),
        }
    }

    async fn write_data(&self, id: &str, _value: serde_json::Value) -> Result<()> {
        Err(DiagnosticError::ReadOnly(id.to_string()))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    DiagnosticServer::new(MyProvider, 8081)
        .serve()
        .await?;
    Ok(())
}
```

## Self-registration

Pass an `HttpRegistrar` to register with a SOVD server at startup. The lib retries with exponential backoff so the app and server can start in any order.

```rust
use opensovd_diagnostic_lib::registration::{AppEndpoint, HttpRegistrar};

DiagnosticServer::new(MyProvider, 8081)
    .with_registration(
        HttpRegistrar::new("http://127.0.0.1:7691/register"),
        AppEndpoint {
            app_id:    "my-app".to_string(),
            app_name:  "My App".to_string(),
            port:      8081,
            hosted_on: "HPC".to_string(),
        },
    )
    .serve()
    .await?;
```

Need a different transport (D-Bus, iceoryx2, SOME/IP)? Implement `AppRegistrar` — two async methods: `register` and `deregister` (default no-op).

## Heartbeat

Add a heartbeat so the SOVD server can detect stale apps:

```rust
use std::time::Duration;

DiagnosticServer::new(MyProvider, 8081)
    .with_registration(registrar, endpoint)
    .with_heartbeat(Duration::from_secs(30))
    .serve()
    .await?;
```

The heartbeat re-sends the registration on the configured interval. On a clean shutdown (SIGINT or SIGTERM) the lib calls `deregister` before exiting, removing the app from the SOVD topology immediately.

## Endpoints

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/health` | Liveness check |
| `GET` | `/api/info` | App name, version, description |
| `GET` | `/api/data` | All data items |
| `GET` | `/api/data/{id}` | Read one item |
| `PUT` | `/api/data/{id}` | Write one item |
| `GET` | `/api/stream?data_ids=a,b&interval_ms=100` | SSE stream |

## Data categories

`CurrentData`, `IdentData`, `ConfigData`, `FaultData`, `SysInfo`, `Custom(String)` — maps directly to the SOVD standard.

## License

Apache-2.0
