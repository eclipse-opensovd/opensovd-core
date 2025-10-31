# Running the Server
To run the `opensovd-core` server, you can use the compiled binary or run it via Cargo. The server supports two operational modes: `gateway` and `standalone`.

## Prerequisites

- Ensure the project is built using `./build.sh start`
- Configuration file `/config/sovd_server_apps.conf` should be properly set up

## Command Syntax

cargo run <ip_address> <port> <hostname> --sovd_mode <mode>
cargo run 127.0.0.1 8081 chassis-hpc --sovd_mode standalone