#!/usr/bin/env bash
# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0
#
# Shows the gateway without and with a classic diagnostic adapter (CDA). It runs
# locally with its mock topology and first answers everything itself. The same
# requests then go through a CDA and its ECU simulator, from the CDA checkout's
# testcontainer setup. That stack keeps running until Ctrl-C, which stops what
# the script started. Last, the gateway also checks tokens, with one token for
# gateway and CDA from scripts/dev-token.sh.
#
# Needs docker, cargo, curl, jq and an openssl with Ed25519. Environment:
#   CDA_DIR       CDA checkout (default: ../classic-diagnostic-adapter)
#   CDA_PORT      port of the CDA (default: 20002)
#   GATEWAY_PORT  port of the gateway (default: 17690)

set -euo pipefail

cd "$(dirname "$0")/.."

CDA_DIR=${CDA_DIR:-../classic-diagnostic-adapter}
CDA_PORT=${CDA_PORT:-20002}
GATEWAY_PORT=${GATEWAY_PORT:-17690}
CDA="http://localhost:$CDA_PORT"
GATEWAY="http://localhost:$GATEWAY_PORT/sovd"

ECU=flxc1000 # held by the CDA, not by the gateway

gateway_pid=
started_cda=
keys=

compose() {
    CDA_PORT=$CDA_PORT docker compose --project-directory "$CDA_DIR/testcontainer" "$@"
}

stop_gateway() {
    if [ -n "$gateway_pid" ]; then
        kill "$gateway_pid" 2>/dev/null || true
        wait "$gateway_pid" 2>/dev/null || true
        gateway_pid=
    fi
}

cleanup() {
    stop_gateway
    if [ -n "$keys" ]; then
        rm -rf "$keys"
    fi
    if [ -n "$started_cda" ]; then
        compose down
    fi
}
trap cleanup EXIT
trap 'exit 130' INT TERM

wait_for() { # url expected-status tries
    for _ in $(seq 1 "$3"); do
        [ "$(curl -s -o /dev/null -w '%{http_code}' "$1")" = "$2" ] && return 0
        sleep 1
    done
    echo "Timed out waiting for $1" >&2
    return 1
}

start_gateway() {
    stop_gateway
    if curl -s -o /dev/null "$GATEWAY/version-info"; then
        echo "Port $GATEWAY_PORT is in use, stop what runs there or set GATEWAY_PORT" >&2
        exit 1
    fi
    target/debug/opensovd-gateway --mock --url "$GATEWAY" "$@" >/dev/null 2>&1 &
    gateway_pid=$!
    # With gateway auth, the gateway asks for a token even here.
    case " $* " in
    *" --auth-jwks "*) wait_for "$GATEWAY/version-info" 401 30 ;;
    *) wait_for "$GATEWAY/version-info" 200 30 ;;
    esac
}

# Prints a request through the gateway and its answer, shortened.
show() { # method path
    local body status
    body=$(mktemp)
    status=$(curl -s -o "$body" -w '%{http_code}' -X "$1" \
        ${TOKEN:+-H "Authorization: Bearer $TOKEN"} "$GATEWAY/v1$2")
    printf '%-6s /v1%-34s %s  %s\n' "$1" "$2" "$status" "$(jq -c . "$body" 2>/dev/null | cut -c1-90)"
    rm -f "$body"
}

login() {
    TOKEN=$(curl -s -X POST "$GATEWAY/v1/authorize" \
        -H 'Content-Type: application/json' \
        -d '{"client_id": "test", "client_secret": "secret"}' | jq -r .access_token)
}

requests() {
    echo "Components: $(curl -s ${TOKEN:+-H "Authorization: Bearer $TOKEN"} "$GATEWAY/v1/components" |
        jq -c '[.items[].id]')"
    show GET "/components/ecu"
    show GET "/components/$ECU"
    show GET "/components/$ECU/faults"
    show GET "/functions"
    show GET "/locks"
    show POST "/authorize"
}

# The self-discovery of a client: from the list to the ECU to its faults, by
# following the links of the responses only.
walk() {
    local href
    href=$(curl -s ${TOKEN:+-H "Authorization: Bearer $TOKEN"} "$GATEWAY/v1/components" | jq -r --arg ecu "$ECU" '.items[] | select(.id == $ecu) | .href')
    echo "Listed:     $href"
    href=$(curl -s -H "Authorization: Bearer $TOKEN" "$href" | jq -r .faults)
    echo "Its faults: $href"
    echo "Answer:     $(curl -s -H "Authorization: Bearer $TOKEN" "$href" | jq -c . | cut -c1-90)"
}

if [ ! -f "$CDA_DIR/testcontainer/docker-compose.yml" ]; then
    echo "No CDA checkout at $CDA_DIR, set CDA_DIR" >&2
    exit 1
fi

echo "==> Building the gateway"
cargo build --quiet -p opensovd-gateway

echo
echo "==> Without the CDA: the gateway answers everything itself"
start_gateway
requests

echo
if [ "$(curl -s -o /dev/null -w '%{http_code}' "$CDA/health/ready")" != 204 ]; then
    echo "==> Starting the CDA and the ECU simulator, the first build takes a while"
    started_cda=1
    compose up --build --detach
    wait_for "$CDA/health/ready" 204 300
fi

echo
echo "==> With the CDA: --upstream-url $CDA/vehicle/v15"
start_gateway --upstream-url "$CDA/vehicle/v15" \
    --upstream-authorize-url "$CDA/vehicle/v15"

# The token comes from the CDA, through the gateway.
login
requests
echo
walk

echo
echo "==> With gateway auth: one token for the gateway and the CDA"
keys=$(mktemp -d)
token=$(scripts/dev-token.sh "$keys" dev-stack test)
start_gateway --upstream-url "$CDA/vehicle/v15" \
    --auth-jwks "$keys/dev-jwks.json" --auth-issuer dev-stack
TOKEN=
echo "Without a token: $(curl -s -o /dev/null -w '%{http_code}' "$GATEWAY/v1/components")"
TOKEN=$token
requests
echo
walk

echo
echo "==> Running until Ctrl-C"
echo "Gateway: $GATEWAY/v1 (checking tokens, forwarding to the CDA)"
echo "CDA:     $CDA/vehicle/v15"
echo "Token:   $TOKEN"
wait "$gateway_pid"
