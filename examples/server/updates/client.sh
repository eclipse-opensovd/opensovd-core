#!/usr/bin/env bash
# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0

# Drives the updates example server through prepare → poll → execute → poll.
# Requires the server to be running: cargo run -p opensovd-examples-server --example updates

set -euo pipefail

BASE="http://127.0.0.1:7690/sovd/v1"
APP_ID="update-example"
ID="firmware-v1"
POLL_INTERVAL=2

call() {
    curl -sf -X "$1" -H "Content-Type: application/json" "$2" | jq .
}

# $3 is a path to a JSON file
call_with_file() {
    curl -sf -X "$1" -H "Content-Type: application/json" -d "@$3" "$2" | jq .
}

poll_until_done() {
    local phase="$1"
    echo "--- polling status ($phase) ---"
    while true; do
        local out
        out=$(curl -sf "${BASE}/updates/${ID}/status")
        echo "$out" | jq .
        local status
        status=$(echo "$out" | jq -r '.data.status // .status // ""' 2>/dev/null || echo "")
        [[ "$status" == "completed" || "$status" == "failed" ]] && break
        sleep "$POLL_INTERVAL"
    done
}

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "=== upload update payload ==="
curl -sf -X POST \
    -H "Content-Type: application/octet-stream" \
    -H "Content-Disposition: attachment; filename=\"${ID}\"" \
    --data-binary @"${SCRIPT_DIR}/json/payload.json" \
    "${BASE}/apps/${APP_ID}/bulk-data/updates" | jq .

echo ""
echo "=== register update ==="
call_with_file POST "${BASE}/updates" "${SCRIPT_DIR}/json/manifest.json"

echo ""
echo "=== available updates ==="
call GET "${BASE}/updates"

echo ""
echo "=== trigger prepare ==="
call PUT "${BASE}/updates/${ID}/prepare"

poll_until_done "prepare"

echo ""
echo "=== trigger execute ==="
call PUT "${BASE}/updates/${ID}/execute"

poll_until_done "execute"

echo ""
echo "=== done ==="
