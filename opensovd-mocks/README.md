<!-- SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation -->
<!-- SPDX-License-Identifier: Apache-2.0 -->

# opensovd-mocks

This crate provides a shared mock topology via `create_mock_topology()`, used across the project for development, testing, and examples:

- **`opensovd-gateway --mock`** — the gateway binary loads mock data when the `--mock` flag is passed (`opensovd-cli/gateway/src/main.rs`)
- **Unit tests** — route handler tests in `opensovd-server` for components, areas, and apps
- **Examples** — `simple`, `auth`, and `systemd` server examples (`examples/server/`)

## Entity Hierarchy

```text
SOVDServer
├── Areas
│   ├── powertrain ("Powertrain Domain")
│   │   ├── Component: ecu ("Engine Control Unit")
│   │   └── App: engine_control ("Engine Control Application") → hosted on ecu
│   └── network ("Network Domain")
│       ├── Component: gateway ("Vehicle Gateway")
│       └── App: diagnostics ("Diagnostic Services") → hosted on gateway
└── Apps (no area)
    └── ota_manager ("OTA Update Manager") → hosted on gateway
```

## Entity Summary

### Components

| id        | name                | area       | tags                 | data items                                                                                   | faults                                       |
|-----------|---------------------|------------|----------------------|----------------------------------------------------------------------------------------------|----------------------------------------------|
| `ecu`     | Engine Control Unit | powertrain | powertrain, critical | 8 (voltage, temperature, sw.version, sw.build_date, sw.sha1, hw.version, hw.revision, hw.sn) | 2 (fuelPressureLow, coolantSensorSignalLost) |
| `gateway` | Vehicle Gateway     | network    | network              | 7 (sw.version, sw.build_date, sw.sha1, hw.version, hw.revision, hw.sn, uptime)               | _(none)_                                     |

### Apps

| id               | name                       | host    | area       | tags                   | data items                                               | faults                                       |
|------------------|----------------------------|---------|------------|------------------------|----------------------------------------------------------|----------------------------------------------|
| `engine_control` | Engine Control Application | ecu     | powertrain | powertrain, critical   | 3 (app.version, app.status, fuel_injection.rate)         | 2 (fuelPressureLow, coolantSensorSignalLost) |
| `diagnostics`    | Diagnostic Services        | gateway | network    | network, service       | 3 (app.version, active_connections, messages_per_second) | _(none)_                                     |
| `ota_manager`    | OTA Update Manager         | gateway | _(none)_   | system, infrastructure | 3 (app.version, update_available, last_check)            | _(none)_                                     |

### Areas

| id           | name              | tags   |
|--------------|-------------------|--------|
| `powertrain` | Powertrain Domain | domain |
| `network`    | Network Domain    | domain |
