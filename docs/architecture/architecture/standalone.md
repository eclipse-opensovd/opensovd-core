# Standalone Mode Architecture

In **Standalone mode**, the `opensovd-core` server operates as a self-contained diagnostic endpoint. This mode is designed for scenarios where a single ECU or HPC (High-Performance Computer) needs to expose SOVD-compliant diagnostic services without relying on a gateway to route requests.

## Purpose

Standalone mode is ideal for:
- Isolated testing of individual ECUs
- Development environments where gateway infrastructure is not available
- Simulating SOVD behavior for a single component

## How It Works

When launched in standalone mode, the server:
1. Loads configuration from the `sovd_server_apps.conf` file.
2. Initializes the diagnostic service stack for the specified ECU.
3. Binds to a local IP and port to expose SOVD-REST endpoints.
4. Handles incoming requests directly and returns responses without routing or aggregation.

## Key Characteristics

- **No routing logic**: Requests are handled locally.
- **Single endpoint**: Represents one ECU or HPC.
- **Simplified architecture**: No need for adapter orchestration or multi-ECU coordination.

## Example CLI Usage

cargo run 127.0.0.1 8080 chassis-hpc --sovd_mode standalone

## Query of Available Entities under SOVD server Standalone

- **base_uri**: 127.0.0.1/v1

- **Endpoints supported**
|---------|-----------------------------------|---------------------------------------------------------|
| Method  |             Path                  |               Description                               |
|---------|-----------------------------------|---------------------------------------------------------|
|   GET   | {base_uri}/{Entity-Collection}    |  This method provides the list of contained entities    |
|         |                                   |  for each requested entity collection (Components/Apps) |
|---------|-----------------------------------|---------------------------------------------------------|
|   GET   | /{entity-path}/{entity-id}        |      This method returns the capabilities of an entity  |
|         |                                   |                        (data, related-apps)             |
|---------|-----------------------------------|---------------------------------------------------------|
|   GET   | /{entity-path}/data/{id}          |             This method retrieves the value of a        |
|         |                                   |         single data resource, like CPU load, Memory.    |
|---------|-----------------------------------|---------------------------------------------------------|