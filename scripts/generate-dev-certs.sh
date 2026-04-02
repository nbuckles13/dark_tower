#!/bin/bash
#
# Generate TLS certificates for Dark Tower local development
#
# Creates a self-signed CA and signs service certificates for:
#   - Auth Controller (auth-localhost) — HTTPS for JWKS endpoints
#   - Meeting Controller (mc-webtransport) — QUIC/WebTransport on port 4433
#   - Media Handler (mh-webtransport) — QUIC/WebTransport on port 4434
#
# This script is idempotent: re-running regenerates all service certs.
# If the CA already exists, it is preserved unless --force-ca is passed.
#
# Algorithm: ECDSA P-256 (compatible with QUIC/WebTransport TLS stacks)
# See ADR-0027 for approved cryptographic algorithms.
#
# Usage:
#   ./scripts/generate-dev-certs.sh [--force-ca]
#
set -euo pipefail

CERT_DIR="infra/docker/certs"
CA_KEY="${CERT_DIR}/ca.key"
CA_CERT="${CERT_DIR}/ca.crt"
DAYS_CA=3650    # 10 years for dev CA
DAYS_CERT=365   # 1 year for service certs
FORCE_CA=false

for arg in "$@"; do
  case "$arg" in
    --force-ca) FORCE_CA=true ;;
    *) echo "Unknown argument: $arg"; exit 1 ;;
  esac
done

mkdir -p "$CERT_DIR"

# ---------------------------------------------------------------------------
# 1. Self-signed CA
# ---------------------------------------------------------------------------
if [ -f "$CA_KEY" ] && [ -f "$CA_CERT" ] && [ "$FORCE_CA" = false ]; then
  echo "CA already exists, reusing (pass --force-ca to regenerate)"
else
  echo "Generating self-signed CA (ECDSA P-256)..."
  openssl ecparam -genkey -name prime256v1 -noout -out "$CA_KEY" 2>/dev/null
  chmod 600 "$CA_KEY"

  openssl req -new -x509 -key "$CA_KEY" \
    -out "$CA_CERT" \
    -days "$DAYS_CA" \
    -subj "/CN=Dark Tower Dev CA/O=Dark Tower Dev" \
    2>/dev/null
  chmod 644 "$CA_CERT"

  echo "  CA key:  ${CA_KEY}"
  echo "  CA cert: ${CA_CERT}"
fi

# ---------------------------------------------------------------------------
# Helper: generate a service certificate signed by the CA
# ---------------------------------------------------------------------------
generate_service_cert() {
  local name="$1"       # file prefix, e.g. "auth-localhost"
  local cn="$2"         # Common Name
  shift 2
  local sans=("$@")     # Subject Alternative Names (DNS entries)

  local key_file="${CERT_DIR}/${name}.key"
  local csr_file="${CERT_DIR}/${name}.csr"
  local cert_file="${CERT_DIR}/${name}.crt"
  local ext_file="${CERT_DIR}/${name}.ext"

  echo ""
  echo "Generating ${name} certificate (ECDSA P-256)..."

  # Generate private key
  openssl ecparam -genkey -name prime256v1 -noout -out "$key_file" 2>/dev/null
  chmod 600 "$key_file"

  # Build SAN extension file
  {
    echo "authorityKeyIdentifier=keyid,issuer"
    echo "basicConstraints=CA:FALSE"
    echo "keyUsage=digitalSignature,keyEncipherment"
    echo "extendedKeyUsage=serverAuth"
    printf "subjectAltName="
    local first=true
    for san in "${sans[@]}"; do
      if [ "$first" = true ]; then
        first=false
      else
        printf ","
      fi
      printf "DNS:%s" "$san"
    done
    echo ""
  } > "$ext_file"

  # Generate CSR
  openssl req -new -key "$key_file" \
    -out "$csr_file" \
    -subj "/CN=${cn}/O=Dark Tower Dev" \
    2>/dev/null

  # Sign with CA
  openssl x509 -req -in "$csr_file" \
    -CA "$CA_CERT" -CAkey "$CA_KEY" -CAcreateserial \
    -out "$cert_file" \
    -days "$DAYS_CERT" \
    -extfile "$ext_file" \
    2>/dev/null
  chmod 644 "$cert_file"

  # Clean up intermediate files
  rm -f "$csr_file" "$ext_file"

  echo "  Key:  ${key_file}"
  echo "  Cert: ${cert_file}"
}

# ---------------------------------------------------------------------------
# 2. Auth Controller certificate
# ---------------------------------------------------------------------------
generate_service_cert "auth-localhost" "localhost" \
  "localhost"

# ---------------------------------------------------------------------------
# 3. Meeting Controller WebTransport certificate
# ---------------------------------------------------------------------------
generate_service_cert "mc-webtransport" "mc-service.dark-tower.svc.cluster.local" \
  "localhost" \
  "mc-service" \
  "mc-service.dark-tower" \
  "mc-service.dark-tower.svc.cluster.local"

# ---------------------------------------------------------------------------
# 4. Media Handler WebTransport certificate
# ---------------------------------------------------------------------------
generate_service_cert "mh-webtransport" "mh-service.dark-tower.svc.cluster.local" \
  "localhost" \
  "mh-service" \
  "mh-service.dark-tower" \
  "mh-service.dark-tower.svc.cluster.local"

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------
echo ""
echo "=== Development certificates generated ==="
echo ""
echo "CA (trust root):"
echo "  ${CA_CERT}"
echo "  ${CA_KEY} (not needed at runtime — keep offline)"
echo ""
echo "Auth Controller:"
echo "  export TLS_CERT_PATH=${CERT_DIR}/auth-localhost.crt"
echo "  export TLS_KEY_PATH=${CERT_DIR}/auth-localhost.key"
echo ""
echo "Meeting Controller (WebTransport):"
echo "  export MC_TLS_CERT_PATH=${CERT_DIR}/mc-webtransport.crt"
echo "  export MC_TLS_KEY_PATH=${CERT_DIR}/mc-webtransport.key"
echo ""
echo "Media Handler (WebTransport):"
echo "  export MH_TLS_CERT_PATH=${CERT_DIR}/mh-webtransport.crt"
echo "  export MH_TLS_KEY_PATH=${CERT_DIR}/mh-webtransport.key"
echo ""
echo "Services should pin the CA cert for MITM protection:"
echo "  ${CA_CERT}"
