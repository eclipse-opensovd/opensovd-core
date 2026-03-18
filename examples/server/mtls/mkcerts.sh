#!/usr/bin/env bash
# Generate test certificates for the mTLS example.
# Run this once from the workspace root:
#   bash examples/server/mtls/mkcerts.sh
#
# Files created in examples/server/mtls/:
#   ca.key / ca.crt         — self-signed CA
#   server.key / server.crt — server cert signed by the CA
#   client.key / client.crt — client cert signed by the CA (for curl testing)
#

set -euo pipefail

DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$DIR"

echo "==> Generating CA key and self-signed cert..."
openssl req -x509 -newkey rsa:4096 -days 3650 -nodes \
    -keyout ca.key -out ca.crt \
    -subj "/CN=OpenSOVD Test CA/O=OpenSOVD"

echo "==> Generating server key and CSR..."
openssl req -newkey rsa:4096 -nodes \
    -keyout server.key -out server.csr \
    -subj "/CN=127.0.0.1/O=OpenSOVD"

echo "==> Signing server cert with CA (adds SAN for 127.0.0.1)..."
openssl x509 -req -days 365 -in server.csr -CA ca.crt -CAkey ca.key \
    -CAcreateserial -out server.crt \
    -extfile <(printf "subjectAltName=IP:127.0.0.1,DNS:localhost")

echo "==> Generating client key and CSR..."
openssl req -newkey rsa:4096 -nodes \
    -keyout client.key -out client.csr \
    -subj "/CN=test-client/O=OpenSOVD"

echo "==> Signing client cert with CA (adds clientAuth EKU required by rustls)..."
openssl x509 -req -days 365 -in client.csr -CA ca.crt -CAkey ca.key \
    -CAcreateserial -out client.crt \
    -extfile <(printf "extendedKeyUsage=clientAuth\nbasicConstraints=CA:FALSE")

rm -f server.csr client.csr ca.srl

echo "Done!"