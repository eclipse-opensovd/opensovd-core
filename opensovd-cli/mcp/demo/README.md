<!--
SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
SPDX-License-Identifier: Apache-2.0
-->

# MCP demo

`pi.tape` records `pi.gif`. The recording starts the gateway with a mock vehicle, checks the gateway with `curl`, and then asks the [pi](https://www.npmjs.com/package/@earendil-works/pi-coding-agent) agent about the vehicle through the MCP server.

## Recording

You need [vhs](https://github.com/charmbracelet/vhs) 0.10 (0.12 does not write the gif) with its dependencies ffmpeg and ttyd, [gifsicle](https://www.lcdf.org/gifsicle/), `jq`, and Docker. `pi` must be logged in to an Anthropic subscription (run `/login` in pi). Run from the repository root:

```bash
vhs opensovd-cli/mcp/demo/pi.tape
gifsicle -b -U opensovd-cli/mcp/demo/pi.gif
gifsicle -b -O3 --lossy=20 --transparent '#30353d' opensovd-cli/mcp/demo/pi.gif
```

Set `ENGINE=podman` to use Podman, or `PORT` to publish the gateway on another host port. The recording always shows `docker` and `7690`. The second `gifsicle` step also makes the margin around the window transparent.
