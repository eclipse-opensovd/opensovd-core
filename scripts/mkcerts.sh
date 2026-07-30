#!/usr/bin/env bash
# SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0
#
# Generate test certificates for TLS / mTLS.
# Can be used for the mTLS example server, CLI integration tests, or any local
# gateway setup that needs a self-signed PKI.
#
# Usage (run from anywhere):
#   bash scripts/mkcerts.sh [OUTPUT_DIR]
#
# OUTPUT_DIR defaults to gen/certs relative to the workspace root.
#
# Files created:
#   ca.key / ca.crt         — self-signed CA (10-year validity)
#   server.key / server.crt — server cert signed by the CA (SAN: 127.0.0.1, localhost)
#   client.key / client.crt — client cert signed by the CA (clientAuth EKU)

set -euo pipefail

# Stop Git Bash from rewriting the leading-slash -subj args into Windows paths.
export MSYS_NO_PATHCONV=1

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="${1:-"$REPO_ROOT/gen/certs"}"

mkdir -p "$OUT"

echo "==> Generating CA key and self-signed cert..."
openssl req -x509 -newkey rsa:4096 -days 3650 -nodes \
    -keyout "$OUT/ca.key" -out "$OUT/ca.crt" \
    -subj "/CN=OpenSOVD Test CA/O=OpenSOVD" \
    -addext "basicConstraints=critical,CA:TRUE" \
    -addext "keyUsage=critical,digitalSignature,keyCertSign,cRLSign" \
    -addext "subjectKeyIdentifier=hash"

echo "==> Generating server key and CSR..."
openssl req -newkey rsa:4096 -nodes \
    -keyout "$OUT/server.key" -out "$OUT/server.csr" \
    -subj "/CN=127.0.0.1/O=OpenSOVD"

echo "==> Signing server cert with CA (adds SAN for 127.0.0.1 and localhost)..."
printf "subjectAltName=IP:127.0.0.1,DNS:localhost\nsubjectKeyIdentifier=hash\nauthorityKeyIdentifier=keyid:always\n" >"$OUT/server.ext"
openssl x509 -req -days 365 -in "$OUT/server.csr" -CA "$OUT/ca.crt" -CAkey "$OUT/ca.key" \
    -CAcreateserial -out "$OUT/server.crt" \
    -extfile "$OUT/server.ext"

echo "==> Generating client key and CSR..."
openssl req -newkey rsa:4096 -nodes \
    -keyout "$OUT/client.key" -out "$OUT/client.csr" \
    -subj "/CN=test-client/O=OpenSOVD"

echo "==> Signing client cert with CA (adds clientAuth EKU required by rustls)..."
printf "extendedKeyUsage=clientAuth\nbasicConstraints=CA:FALSE\nsubjectKeyIdentifier=hash\nauthorityKeyIdentifier=keyid:always\n" >"$OUT/client.ext"
openssl x509 -req -days 365 -in "$OUT/client.csr" -CA "$OUT/ca.crt" -CAkey "$OUT/ca.key" \
    -CAcreateserial -out "$OUT/client.crt" \
    -extfile "$OUT/client.ext"

rm -f "$OUT/server.csr" "$OUT/client.csr" "$OUT/server.ext" "$OUT/client.ext" "$OUT/ca.srl"

echo ""
echo "==> Generated certificates in $OUT:"
echo "      ca.crt      — CA certificate (trust anchor)"
echo "      server.crt  — server certificate"
echo "      server.key  — server private key"
echo "      client.crt  — client certificate"
echo "      client.key  — client private key"
echo ""
echo "==> Gateway usage:"
echo ""
echo "    Plain TLS:"
echo "      opensovd-gateway --tls-cert $OUT/server.crt \\"
echo "                       --tls-key  $OUT/server.key \\"
echo "                       --url https://127.0.0.1:8443/sovd"
echo ""
echo "    mTLS (client cert required):"
echo "      opensovd-gateway --tls-cert      $OUT/server.crt \\"
echo "                       --tls-key       $OUT/server.key \\"
echo "                       --tls-client-ca $OUT/ca.crt \\"
echo "                       --url https://127.0.0.1:8443/sovd"
echo ""
echo "    curl (mTLS):"
echo "      curl --cacert $OUT/ca.crt \\"
echo "           --cert   $OUT/client.crt \\"
echo "           --key    $OUT/client.key \\"
echo "           https://127.0.0.1:8443/sovd/v1/components"
