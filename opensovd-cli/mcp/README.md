<!--
SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
SPDX-License-Identifier: Apache-2.0
-->

# OpenSOVD Model Context Protocol

> MCP server for AI-assisted OpenSOVD vehicle diagnostics.

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

## Integration

All agents use the same command: `opensovd-mcp --url http://localhost:7690/sovd/v1`

<details>
<summary>Claude Code</summary>

```bash
claude mcp add --transport stdio --scope project sovd -- opensovd-mcp --url http://localhost:7690/sovd/v1
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
      "command": ["opensovd-mcp", "--url", "http://localhost:7690/sovd/v1"]
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
      "command": "opensovd-mcp",
      "args": ["--url", "http://localhost:7690/sovd/v1"]
    }
  }
}
```

</details>

## Contributing

See [CONTRIBUTING.md](../../CONTRIBUTING.md) for guidelines.

## License

This project is licensed under the Apache License 2.0.
