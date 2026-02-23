# Simple Example

Minimal OpenSOVD server exposing a single Linux system component with live data.

## Topology

```text
SOVDServer
  └── Component: "linux"
        └── Data Provider (7 items)
```

## Data Items

| ID               | Name           | Category  | Type                 |
|------------------|----------------|-----------|----------------------|
| `os.version`     | OS Version     | IdentData | Static               |
| `os.name`        | OS Name        | IdentData | Static               |
| `os.pretty_name` | OS Pretty Name | IdentData | Static               |
| `os.id`          | OS Identifier  | IdentData | Static               |
| `os.uptime`      | System Uptime  | SysInfo   | Dynamic (seconds)    |
| `cpu.usage`      | CPU Usage      | SysInfo   | Dynamic (percentage) |
| `mem.usage`      | Memory Usage   | SysInfo   | Dynamic (percentage) |

Static items are read once at startup from `/etc/os-release`. Dynamic items query the system on each request.

## Running

```bash
cargo run -p opensovd-examples-server --example simple
```

The server starts on `http://127.0.0.1:7690`.

## Example Requests

```bash
# List components
curl -s http://localhost:7690/sovd/v1/components | jq

# Get component details
curl -s http://localhost:7690/sovd/v1/components/linux | jq

# List available data items
curl -s http://localhost:7690/sovd/v1/components/linux/data | jq

# Read a specific data item
curl -s http://localhost:7690/sovd/v1/components/linux/data/os.uptime | jq
```

## Terminal Dashboard

A [sampler](https://github.com/sqshq/sampler) config is included to visualize
CPU and memory usage in a terminal dashboard. With the server running:

```bash
sampler -c examples/server/simple/sampler.yaml
```
