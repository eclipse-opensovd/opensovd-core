<!--
SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
SPDX-License-Identifier: Apache-2.0
-->

# OpenSOVD Model Context Protocol

> MCP server for AI-assisted OpenSOVD vehicle diagnostics.

[![CI](https://github.com/eclipse-opensovd/opensovd-core/actions/workflows/ci.yaml/badge.svg?event=push&branch=main)](https://github.com/eclipse-opensovd/opensovd-core/actions/workflows/ci.yaml?query=event%3Apush+branch%3Amain)
[![Coverage](https://img.shields.io/endpoint?url=https://eclipse-opensovd.github.io/opensovd-core/mcp/x86_64-unknown-linux-gnu/coverage/badge.json)](https://eclipse-opensovd.github.io/opensovd-core/mcp/x86_64-unknown-linux-gnu/coverage/html/)
[![Chat](https://img.shields.io/badge/chat-slack-blue?logo=slack)](https://app.slack.com/client/T02MS1M89UH/C0958MQNGP2)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue)](../../LICENSE)
[![Good First Issues](https://img.shields.io/github/issues-search/eclipse-opensovd/opensovd-core?query=is%3Aopen%20label%3A%22good%20first%20issue%22%20label%3Abin%3Amcp&label=good%20first%20issues&color=blue)](https://github.com/eclipse-opensovd/opensovd-core/issues?q=is%3Aopen%20label%3A%22good%20first%20issue%22%20label%3Abin%3Amcp)

Enables AI assistants to interact with [OpenSOVD](https://github.com/eclipse-opensovd/opensovd-core) diagnostic servers via the [Model Context Protocol](https://modelcontextprotocol.io).

This binary is designed to be invoked by AI agents that support the MCP protocol, not for direct manual use.

## Tools

- `list_components` - List all SOVD components
- `list_areas` - List all SOVD areas
- `list_apps` - List all SOVD apps

## Resources

- `sovd://topology` - Vehicle diagnostic topology (components, areas, apps)

## Prompts

- `explore-topology` - Guided exploration of the vehicle topology

## Container image

Published as `ghcr.io/eclipse-opensovd/opensovd-mcp` (tags: `latest`, `nightly`, `vX.Y.Z`).

```bash
docker run -i --rm --network=host ghcr.io/eclipse-opensovd/opensovd-mcp \
    --url http://localhost:7690/sovd/v1
```

## Integration

All agents run the published container image: `docker run -i --rm --network=host ghcr.io/eclipse-opensovd/opensovd-mcp --url http://localhost:7690/sovd/v1`

<details>
<summary>Claude Code</summary>

```bash
claude mcp add --transport stdio --scope project sovd -- \
    docker run -i --rm --network=host ghcr.io/eclipse-opensovd/opensovd-mcp \
    --url http://localhost:7690/sovd/v1
```

</details>

<details>
<summary>OpenCode — <code>opencode.json</code></summary>

Run `opencode mcp add` for an interactive setup, or add to `opencode.json`:

```json
{
  "mcp": {
    "sovd": {
      "type": "local",
      "command": [
        "docker", "run", "-i", "--rm", "--network=host",
        "ghcr.io/eclipse-opensovd/opensovd-mcp",
        "--url", "http://localhost:7690/sovd/v1"
      ]
    }
  }
}
```

</details>

<details>
<summary>GitHub Copilot — <code>.vscode/mcp.json</code></summary>

```json
{
  "servers": {
    "sovd": {
      "command": "docker",
      "args": [
        "run", "-i", "--rm", "--network=host",
        "ghcr.io/eclipse-opensovd/opensovd-mcp",
        "--url", "http://localhost:7690/sovd/v1"
      ]
    }
  }
}
```

</details>

## Contributing

See [CONTRIBUTING.md](../../CONTRIBUTING.md) for guidelines.

## License

This project is licensed under the Apache License 2.0.
