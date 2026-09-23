# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

Output opensovd-cli/mcp/demo/pi.gif
Require pi

Set Shell bash
Set FontFamily "JetBrains Mono, JetBrainsMono Nerd Font Mono"
Set FontSize 18
Set Width 1200
Set Height 800
Set Padding 20
Set Theme "GitHub Dark"
Set WindowBar RingsRight
Set WindowBarSize 40
Set BorderRadius 10
Set Margin 24
Set MarginFill "#30363d"
Set Framerate 20
Set TypingSpeed 60ms

Hide
Type `export PS1='$ ' ENGINE=${ENGINE:-docker} PORT=${PORT:-7690}` Enter
Type `$ENGINE pull -q ghcr.io/eclipse-opensovd/opensovd-gateway >/dev/null` Enter
Type `docker() { command $ENGINE "${@/#7690:7690/$PORT:7690}"; }` Enter
Type `curl() { command curl "${@/:7690\//:$PORT/}"; }` Enter
Type `MCP=$(mktemp --suffix .json)` Enter
Type `printf '{"mcpServers":{"vehicle":{"command":"%s","args":["run","-i","--rm","--network=host","ghcr.io/eclipse-opensovd/opensovd-mcp","--url","http://127.0.0.1:%s/sovd/v1"],"directTools":true}}}' $ENGINE $PORT > $MCP` Enter
Type `PI=$(mktemp -d) && ln -s ~/.pi/agent/* $PI/ && rm $PI/settings.json && jq '.quietStartup=true | .theme="opensovd-demo"' ~/.pi/agent/settings.json > $PI/settings.json` Enter
Type `alias pi='PI_CODING_AGENT_DIR=$PI command pi --offline --no-session -e opensovd-cli/mcp/demo/pi-demo.ts --theme opensovd-cli/mcp/demo/pi-demo-theme.json -na -nc -ns -np --model anthropic/claude-opus-5-5 --mcp-config $MCP --tools vehicle_list_areas,vehicle_list_components,vehicle_list_apps'` Enter
Type "clear" Enter
Wait+Line@120s /^\$\s*$/
Show

Sleep 500ms
Type@25ms "# OpenSOVD: talk to a vehicle in natural language." Enter
Type@25ms "# The opensovd-core gateway serves the SOVD API, and the agent (pi)" Enter
Type@25ms "# reaches it through the opensovd MCP server." Enter
Sleep 2s
Enter
Type@25ms "# 1. Start the gateway with a mock vehicle" Enter
Sleep 500ms
Type "docker run -d --rm --name sovd-gateway -p 7690:7690 ghcr.io/eclipse-opensovd/opensovd-gateway --mock"
Sleep 500ms
Enter
Sleep 2s
Type@25ms "# 2. Verify the gateway is up and list the supported SOVD versions" Enter
Sleep 500ms
Type "curl -s 127.0.0.1:7690/sovd/version-info | jq"
Sleep 300ms
Enter
Wait+Screen@10s /sovd_info/
Sleep 3s
Hide
Type "clear" Enter
Show
Type@25ms "# 3. Start pi with the opensovd MCP server and ask about the vehicle" Enter
Sleep 500ms
Type "pi"
Sleep 300ms
Enter
Wait+Screen@30s /opus/
Sleep 1s

Type "What is the vehicle topology?"
Sleep 500ms
Enter
Sleep 35s

Hide
Ctrl+C
Ctrl+C
Type "$ENGINE stop sovd-gateway >/dev/null" Enter
