# Gateway Mode

The **gateway mode** in `opensovd-core` enables the server to act as a central diagnostic hub within a vehicle. It receives SOVD-REST requests from external clients (e.g., OEM tools, cloud services) and routes them to the appropriate ECUs or HPCs via adapters.

## Purpose

Gateway mode is designed to:
- Centralize diagnostic communication
- Abstract the complexity of multiple ECUs
- Provide a unified interface for external diagnostic tools
- Ensure secure and efficient routing of requests

## Architecture Overview

In gateway mode, the server:
1. Loads configuration from `sovd_server_apps.conf` to identify available endpoints and adapters.
2. Initializes communication channels with ECUs via protocol-specific adapters (e.g., UDS, DoIP, IPC).
3. Listens for incoming SOVD-REST requests on a specified IP and port.
4. Routes requests to the correct ECU based on endpoint mapping.

## Key Components

- **Adapters**: Protocol-specific modules that handle communication with ECUs.
- **Router**: Determines which ECU should handle a given request.

## Routing logic
- **TBD**

## Example CLI Usage

cargo run 127.0.0.1 8080 telematics --sovd_mode gateway

