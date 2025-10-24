
# Integration Test Documentation

## Overview

This document provides an overview of the integration tests implemented for the `sovd-server` application. These tests validate the behavior of various REST API endpoints exposed by the server, ensuring that they respond correctly and return expected data.

## Test Environment

- **Language**: Rust
- **Test Framework**: `tokio::test` for asynchronous testing
- **HTTP Client**: `reqwest`
- **Application Configuration**: Loaded from `../config/sovd_server_apps.conf`
- **Server Mode**: Standalone
- **Component**: `chassis-hpc`

### Setup Instructions

1. Ensure the configuration file exists at `../config/sovd_server_apps.conf`.
2. Run all tests using `cargo test -p sovd-server`. or single test `cargo test get_component_info`

## Test Cases Summary

| Test Name                             | Endpoint Path                                                                 | Description                                      |
|---------------------------------------|-------------------------------------------------------------------------------|--------------------------------------------------|
| `get_component_info`                  | `v1/components`                                                               | Fetches general component information            |
| `get_component_data`                  | `v1/components/chassis-hpc`                                                   | Fetches data for the `chassis-hpc` component     |
| `get_component_specific_data`         | `v1/components/chassis-hpc/data`                                              | Fetches detailed data for the component          |
| `get_component_specific_cpu_usage`    | `v1/components/chassis-hpc/data/chassis-hpc-cpu`                              | Fetches CPU usage data                           |
| `get_component_specific_disk_usage`   | `v1/components/chassis-hpc/data/chassis-hpc-disk`                             | Fetches disk usage data                          |
| `get_component_specific_memory_usage` | `v1/components/chassis-hpc/data/chassis-hpc-memory`                           | Fetches memory usage data                        |
| `get_related_apps`                    | `v1/components/chassis-hpc/related-apps`                                      | Fetches related applications                     |
| `get_specific_app`                    | `v1/apps/sovd-server-<pid>`                                                   | Fetches specific app info using process PID      |
| `get_specific_app_data`               | `v1/apps/sovd-server-<pid>/data`                                              | Fetches data for a specific app                  |
| `get_specific_app_cpu`                | `v1/apps/sovd-server-<pid>/data/sovd-server-<pid>-cpu`                        | Fetches CPU data for a specific app              |
| `get_specific_app_memory`             | `v1/apps/sovd-server-<pid>/data/sovd-server-<pid>-memory`                     | Fetches memory data for a specific app           |
| `get_specific_app_disk`               | `v1/apps/sovd-server-<pid>/data/sovd-server-<pid>-disk`                       | Fetches disk data for a specific app             |
| `get_specific_app_all`                | `v1/apps/sovd-server-<pid>/data/sovd-server-<pid>-all`                        | Fetches all data for a specific app              |

## Endpoint Behavior

Each test performs the following steps:
- Starts the test server if not already running.
- Constructs the appropriate endpoint URL.
- Sends a GET request using `reqwest`.
- Asserts that the response status is successful.
- Asserts that the response body is not empty.

## Notes

- The server address is dynamically assigned and stored in a static variable.
- The process PID is retrieved using `get_process_pid("sovd-server")`.
- The tests are asynchronous and use a shared HTTP client.

