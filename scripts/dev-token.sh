#!/usr/bin/env bash
# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0
#
# Prints a token valid for an hour, for development without an identity
# provider. On first use it creates the signing key in DIRECTORY, and with it
# dev-jwks.json, for --auth-jwks and for every upstream that verifies tokens.
# Needs an openssl with Ed25519.
#
# Usage: scripts/dev-token.sh DIRECTORY ISSUER SUBJECT
set -euo pipefail

dir=${1:?usage: dev-token.sh DIRECTORY ISSUER SUBJECT}
issuer=${2:?usage: dev-token.sh DIRECTORY ISSUER SUBJECT}
subject=${3:?usage: dev-token.sh DIRECTORY ISSUER SUBJECT}
key=$dir/dev.pem

b64() { base64 | tr '+/' '-_' | tr -d '=\n'; }

if [ ! -f "$key" ]; then
    mkdir -p "$dir"
    (umask 077 && openssl genpkey -algorithm ed25519 -out "$key")
    # The raw public key is the end of its DER encoding.
    x=$(openssl pkey -in "$key" -pubout -outform DER | tail -c 32 | b64)
    printf '{"keys":[{"kty":"OKP","crv":"Ed25519","kid":"dev","use":"sig","alg":"EdDSA","x":"%s"}]}\n' \
        "$x" >"$dir/dev-jwks.json"
fi

now=$(date +%s)
header=$(printf '{"alg":"EdDSA","kid":"dev"}' | b64)
payload=$(printf '{"iss":"%s","sub":"%s","iat":%d,"exp":%d}' \
    "$issuer" "$subject" "$now" "$((now + 3600))" | b64)
input=$(mktemp)
trap 'rm -f "$input"' EXIT
printf '%s.%s' "$header" "$payload" >"$input"
signature=$(openssl pkeyutl -sign -inkey "$key" -rawin -in "$input" | b64)
printf '%s.%s.%s\n' "$header" "$payload" "$signature"
