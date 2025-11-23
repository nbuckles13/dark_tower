#!/bin/bash
set -e

CERT_DIR="infra/docker/certs"
mkdir -p "$CERT_DIR"

echo "Generating self-signed TLS certificate for localhost..."
echo ""

openssl req -x509 -newkey rsa:4096 -nodes \
  -keyout "$CERT_DIR/auth-localhost.key" \
  -out "$CERT_DIR/auth-localhost.crt" \
  -days 365 \
  -subj "/CN=localhost/O=Dark Tower Dev" \
  2>/dev/null

echo "✅ Certificates generated:"
echo "   Private key: $CERT_DIR/auth-localhost.key"
echo "   Certificate: $CERT_DIR/auth-localhost.crt"
echo ""
echo "📌 Services should pin this certificate for JWKS fetching (MITM protection)"
echo ""
echo "Usage in Auth Controller:"
echo "  export TLS_CERT_PATH=$CERT_DIR/auth-localhost.crt"
echo "  export TLS_KEY_PATH=$CERT_DIR/auth-localhost.key"
echo "  cargo run --bin auth-controller"
