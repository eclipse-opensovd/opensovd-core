# APP01 — ECU Battery Voltage Monitor

A minimal example application demonstrating how to integrate `opensovd-diagnostic-lib` into a Rust ECU application and make it diagnosable via a SOVD server.

## Overview

APP01 simulates an ECU that monitors battery voltage. It exposes 4 diagnostic data items and self-registers with `HPC01-sovd-server` at startup — no manual server configuration needed.

## Data Items

| ID | Name | Category | Writable |
|----|------|----------|----------|
| `ecu.id` | ECU Identifier | IdentData | No |
| `ecu.version` | Software Version | IdentData | No |
| `battery.voltage` | Battery Voltage | CurrentData | No |
| `battery.uptime` | ECU Uptime | SysInfo | No |

`battery.voltage` oscillates between 11.8 V and 14.4 V to simulate a real sensor.

## Running

Start `HPC01-sovd-server` first, then APP01:

```bash
cargo run --example HPC01-sovd-server -p opensovd-examples-server
cargo run --bin APP01
```

APP01 starts its diagnostic HTTP server on port 8081 and automatically registers with the SOVD server at `http://127.0.0.1:7691/register`.

## Diagnostic API

Once running, the diagnostic endpoints are available directly:

```bash
# Health check
curl http://localhost:8081/health

# List all data items
curl http://localhost:8081/api/data

# Read live voltage
curl http://localhost:8081/api/data/battery.voltage
```

## Via SOVD

After registration, data is also accessible through the SOVD server:

```bash
curl http://localhost:7690/sovd/v1/apps/APP01/data
curl http://localhost:7690/sovd/v1/apps/APP01/data/battery.voltage
```

## Ports

| Port | Purpose |
|------|---------|
| 7690 | SOVD server |
| 7691 | App registration endpoint |
| 8081 | APP01 diagnostic HTTP server |

## License

Apache-2.0
