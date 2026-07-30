#!/bin/bash
#
# Generate TLS certificates for Dark Tower local development
#
# Creates a self-signed CA and signs service certificates for:
#   - Auth Controller (auth-localhost) — HTTPS for JWKS endpoints (365-day validity)
#   - Meeting Controller (mc-webtransport) — QUIC/WebTransport on port 4433 (14-day validity)
#   - Media Handler (mh-webtransport) — QUIC/WebTransport on port 4434 (14-day validity)
#
# The MC/MH WebTransport leaves are pinned in the browser via Chrome's
# `serverCertificateHashes` (dev self-signed trust path, R-14). Chrome REJECTS a
# pinned cert whose validity window exceeds 14 days, so those two leaves use a
# 14-day window (DAYS_WT_CERT). auth-localhost is trusted via the dev-CA-import
# path — NOT serverCertificateHashes — so it is unaffected by the 14-day cap and
# stays at 365 days (R-36).
#
# This script is idempotent: leaf certs are REUSED across runs unless they are
# missing, expiring within 24h, the CA was (re)generated this run, or --force is
# passed. This keeps the fingerprints stable across setup.sh's two invocations
# (create_mc_tls_secret + create_mh_tls_secret) and across same-day re-runs.
# If the CA already exists, it is preserved unless --force-ca is passed.
#
# On every run it (re)writes the DER-leaf SHA-256 fingerprints for the MC/MH
# WebTransport leaves to `infra/docker/certs/fingerprints.json` (+ a sourceable
# `fingerprints.env`) via MC_CERT_SHA256 / MH_CERT_SHA256 (R-36). Both files are
# gitignored.
#   - fingerprints.json is consumed TODAY by the Vite config
#     (packages/web-app/vite/fingerprints.ts).
#   - fingerprints.env is written for the Playwright global-setup specified in
#     R-36 and landing with story task #18. NOT YET CONSUMED on this branch —
#     do not remove it as dead output; #18 is its intended reader.
#
# Algorithm: ECDSA P-256 (compatible with QUIC/WebTransport TLS stacks)
# See ADR-0027 for approved cryptographic algorithms.
#
# Usage:
#   ./scripts/generate-dev-certs.sh [--force-ca] [--force]
#
#   --force-ca   Regenerate the dev CA (implies regenerating all leaves).
#   --force      Regenerate all leaf certs even if still valid.
#
set -euo pipefail

# Resolve repo root from the script's own location so this script works
# regardless of caller's CWD (setup.sh, manual host invocation, etc.).
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
CERT_DIR="$REPO_ROOT/infra/docker/certs"
CA_KEY="${CERT_DIR}/ca.key"
CA_CERT="${CERT_DIR}/ca.crt"
FINGERPRINTS_JSON="${CERT_DIR}/fingerprints.json"
FINGERPRINTS_ENV="${CERT_DIR}/fingerprints.env"
DAYS_CA=3650      # 10 years for dev CA
DAYS_CERT=365     # 1 year for non-WebTransport service certs (auth-localhost)
DAYS_WT_CERT=14   # 14 days for MC/MH WebTransport leaves (Chrome serverCertificateHashes cap)
FORCE_CA=false
FORCE_LEAF=false
CA_REGENERATED=false   # set true when the CA is (re)generated this run; forces leaf regen

for arg in "$@"; do
  case "$arg" in
    --force-ca) FORCE_CA=true ;;
    --force) FORCE_LEAF=true ;;
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

  # A fresh CA invalidates any leaves signed by the old one (they no longer
  # chain), so force every leaf to regenerate this run.
  CA_REGENERATED=true

  echo "  CA key:  ${CA_KEY}"
  echo "  CA cert: ${CA_CERT}"
fi

# ---------------------------------------------------------------------------
# Helper: generate a service certificate signed by the CA
#
# Args: <name> <days> <cn> <san...>
#
# Conditional (idempotent) regeneration: an existing leaf is REUSED unless it is
# missing, expiring within 24h, the CA was regenerated this run, or --force was
# passed. Reuse keeps the on-disk cert — and therefore its fingerprint and the
# deployed TLS Secret — byte-stable across setup.sh's double invocation and
# same-day re-runs (avoids a fingerprints.json ⇄ served-cert skew, R-36).
# ---------------------------------------------------------------------------
generate_service_cert() {
  local name="$1"       # file prefix, e.g. "auth-localhost"
  local days="$2"       # validity window in days
  local cn="$3"         # Common Name
  shift 3
  local sans=("$@")     # Subject Alternative Names (DNS entries)

  local key_file="${CERT_DIR}/${name}.key"
  local csr_file="${CERT_DIR}/${name}.csr"
  local cert_file="${CERT_DIR}/${name}.crt"
  local ext_file="${CERT_DIR}/${name}.ext"

  # Reuse path: present, key present, NOT expiring within 24h (86400s), AND
  # remaining validity below the intended window length (`days`) — a PROXY for
  # the validity-window cap, not the window itself (see the NOTE at the end of
  # this block).
  #
  # The upper bound is load-bearing for R-36's migration path AND enforces
  # @security's invariant: no reused MC/MH leaf has >14 days remaining validity.
  # A dev tree that predates this story carries 365-day mc/mh WebTransport leaves
  # (minted by the old script for the MC/MH QUIC env-tests). Those pass the 24h
  # lower check and would be reused forever — but Chrome REJECTS a
  # serverCertificateHashes pin whose validity exceeds 14 days regardless of hash
  # match, so a too-long leaf MUST be regenerated down to `days`. `-checkend days`
  # returns 0 only for a cert that will NOT expire within `days` (i.e. remaining
  # > days — an over-long legacy leaf); the `!` flips that to "regenerate". A
  # correctly-sized leaf always has remaining strictly < `days` at any reuse-check,
  # because the check never runs in the same invocation that minted the cert
  # (setup.sh's two generate calls are seconds apart), so it is reused — no thrash.
  # Regenerating exactly at the boundary is the safe direction (also covers a
  # backward clock skew). No slack, so reuse strictly implies remaining < `days`.
  #
  # NOTE: this bounds REMAINING validity as a proxy for the validity WINDOW
  # (Chrome caps notAfter-notBefore <= days). Catches the common legacy case
  # (fresh 365d leaf -> remaining ~365d -> regenerated); the residual edge — a
  # ~year-old 365d-window leaf decayed to <=days remaining — is fail-closed
  # (Chrome refuses the >days-window pin -> dev re-runs -> self-corrects). We
  # deliberately avoid exact notAfter-notBefore date math (GNU-vs-BSD `date -d`
  # fragility) for an edge case that can't bite in practice — proxy is the right
  # altitude for a dev script.
  # Both `-checkend` calls are guarded probes (set-e-safe).
  if [ "$FORCE_LEAF" = false ] && [ "$CA_REGENERATED" = false ] \
     && [ -f "$cert_file" ] && [ -f "$key_file" ] \
     && openssl x509 -checkend 86400 -noout -in "$cert_file" >/dev/null 2>&1 \
     && ! openssl x509 -checkend $(( days * 86400 )) -noout -in "$cert_file" >/dev/null 2>&1; then
    echo ""
    echo "Reusing existing ${name} certificate (present, valid, within its ${days}-day window)."
    echo "  Cert: ${cert_file}"
    return 0
  fi

  echo ""
  echo "Generating ${name} certificate (ECDSA P-256, ${days}-day validity)..."

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
    -days "$days" \
    -extfile "$ext_file" \
    2>/dev/null
  chmod 644 "$cert_file"

  # Clean up intermediate files
  rm -f "$csr_file" "$ext_file"

  echo "  Key:  ${key_file}"
  echo "  Cert: ${cert_file}"
}

# ---------------------------------------------------------------------------
# Fingerprint helpers (R-36)
#
# Chrome's serverCertificateHashes pins the SHA-256 of the DER encoding of the
# FULL leaf certificate (not the SPKI/pubkey). The browser consumer decodes the
# value with atob(), so it MUST be STANDARD base64 (not base64url) — `openssl
# base64 -A` emits single-line standard base64. Stderr is intentionally NOT
# suppressed on these calls so a pipeline failure surfaces (error-context).
# ---------------------------------------------------------------------------
fingerprint_b64() {
  local cert_file="$1"
  openssl x509 -in "$cert_file" -outform DER | openssl dgst -sha256 -binary | openssl base64 -A
}

fingerprint_hex() {
  local cert_file="$1"
  # `dgst -sha256` prints "…= <hex>"; take the last field (works across the
  # "(stdin)= " / "SHA2-256(stdin)= " prefix variants).
  openssl x509 -in "$cert_file" -outform DER | openssl dgst -sha256 | awk '{print $NF}'
}

# Leaf notAfter as ISO-8601 UTC. GNU `date -d` parses openssl's format; on a
# host whose `date` is BSD (no -d), fall back to the raw openssl string rather
# than aborting — still a useful diagnostic.
cert_expires_at() {
  local cert_file="$1" not_after iso
  not_after="$(openssl x509 -in "$cert_file" -enddate -noout | sed 's/notAfter=//')"
  if iso="$(date -u -d "$not_after" +%Y-%m-%dT%H:%M:%SZ 2>/dev/null)"; then
    printf '%s' "$iso"
  else
    printf '%s' "$not_after"
  fi
}

# Assert a base64 SHA-256 value decodes to exactly 32 bytes — catches a
# malformed/empty fingerprint at generation time rather than as an opaque
# browser WebTransport refusal downstream.
assert_sha256_b64() {
  local label="$1" b64="$2" n
  n="$(printf '%s' "$b64" | openssl base64 -d -A | wc -c | tr -d ' ')"
  if [ "$n" -ne 32 ]; then
    echo "ERROR: ${label} fingerprint did not decode to 32 bytes (got ${n} bytes) — SHA-256 output is malformed." >&2
    exit 1
  fi
}

# ---------------------------------------------------------------------------
# 2. Auth Controller certificate (365-day — not on the serverCertificateHashes path)
# ---------------------------------------------------------------------------
generate_service_cert "auth-localhost" "$DAYS_CERT" "localhost" \
  "localhost"

# ---------------------------------------------------------------------------
# 3. Meeting Controller WebTransport certificate (14-day — Chrome-pinned leaf)
# ---------------------------------------------------------------------------
generate_service_cert "mc-webtransport" "$DAYS_WT_CERT" "mc-service.dark-tower.svc.cluster.local" \
  "localhost" \
  "mc-service" \
  "mc-service.dark-tower" \
  "mc-service.dark-tower.svc.cluster.local"

# ---------------------------------------------------------------------------
# 4. Media Handler WebTransport certificate (14-day — Chrome-pinned leaf)
# ---------------------------------------------------------------------------
generate_service_cert "mh-webtransport" "$DAYS_WT_CERT" "mh-service.dark-tower.svc.cluster.local" \
  "localhost" \
  "mh-service" \
  "mh-service.dark-tower" \
  "mh-service.dark-tower.svc.cluster.local"

# ---------------------------------------------------------------------------
# 5. WebTransport cert fingerprints (R-36)
#
# Recomputed and rewritten EVERY run from the on-disk leaves — so the file can
# never drift from the cert actually deployed — even when the leaves themselves
# were reused above.
# ---------------------------------------------------------------------------
MC_CERT="${CERT_DIR}/mc-webtransport.crt"
MH_CERT="${CERT_DIR}/mh-webtransport.crt"

MC_SHA256_B64="$(fingerprint_b64 "$MC_CERT")"
MH_SHA256_B64="$(fingerprint_b64 "$MH_CERT")"
MC_SHA256_HEX="$(fingerprint_hex "$MC_CERT")"
MH_SHA256_HEX="$(fingerprint_hex "$MH_CERT")"
MC_EXPIRES_AT="$(cert_expires_at "$MC_CERT")"
MH_EXPIRES_AT="$(cert_expires_at "$MH_CERT")"
GENERATED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

assert_sha256_b64 "mc-webtransport" "$MC_SHA256_B64"
assert_sha256_b64 "mh-webtransport" "$MH_SHA256_B64"

# fingerprints.json — MC_CERT_SHA256 / MH_CERT_SHA256 are the ONE canonical key
# for the load-bearing base64 value (consumed by the packages/web-app vite config;
# also by the Playwright global-setup specified in R-36, pending task #18). The
# *_hex and *_expires_at fields are distinct debug/diagnostic data, not alternate
# encodings of the base64 value.
cat > "$FINGERPRINTS_JSON" <<EOF
{
  "generated_at": "${GENERATED_AT}",
  "MC_CERT_SHA256": "${MC_SHA256_B64}",
  "MH_CERT_SHA256": "${MH_SHA256_B64}",
  "mc_cert_sha256_hex": "${MC_SHA256_HEX}",
  "mh_cert_sha256_hex": "${MH_SHA256_HEX}",
  "mc_cert_expires_at": "${MC_EXPIRES_AT}",
  "mh_cert_expires_at": "${MH_EXPIRES_AT}"
}
EOF
chmod 644 "$FINGERPRINTS_JSON"

# fingerprints.env — sourceable form (shell; and the Playwright global-setup
# specified in R-36, landing with task #18 — no consumer on this branch yet).
cat > "$FINGERPRINTS_ENV" <<EOF
# Generated by scripts/generate-dev-certs.sh (R-36) — DO NOT COMMIT (gitignored).
# Dev-only SHA-256 (STANDARD base64, over the DER of the full leaf cert) used for
# Chrome serverCertificateHashes pinning of the MC/MH WebTransport self-signed
# leaves. Source this file to export the vars:
#   source infra/docker/certs/fingerprints.env
export MC_CERT_SHA256="${MC_SHA256_B64}"
export MH_CERT_SHA256="${MH_SHA256_B64}"
EOF
chmod 644 "$FINGERPRINTS_ENV"

echo ""
echo "WebTransport cert fingerprints written (SHA-256 of DER leaf, standard base64):"
echo "  ${FINGERPRINTS_JSON}  (keys MC_CERT_SHA256 / MH_CERT_SHA256)"
echo "  ${FINGERPRINTS_ENV}   (sourceable exports)"

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
echo ""
echo "WebTransport leaf expiry (${DAYS_WT_CERT}-day validity — Chrome serverCertificateHashes cap):"
echo "  mc-webtransport: notAfter=${MC_EXPIRES_AT}"
echo "  mh-webtransport: notAfter=${MH_EXPIRES_AT}"
echo ""
echo "If the browser later refuses the MC/MH WebTransport handshake, the leaf may have"
echo "expired (routine given the 14-day window) or the deployed cert/fingerprint may be"
echo "stale. Full recovery on a RUNNING cluster (re-running this script alone only rewrites"
echo "the PEMs — it does not update the in-cluster Secret or restart the pods):"
echo "  1) ./infra/kind/scripts/setup.sh"
echo "     # regenerates expiring leaves, recreates mc/mh-service-tls Secrets, redeploys"
echo "  2) kubectl rollout restart deployment/mc-0 deployment/mc-1 deployment/mh-0 deployment/mh-1 -n dark-tower"
echo "     # 'apply' alone does not restart pods on a secret-only change"
echo "     # (why: docs/runbooks/client-dev-local.md F7; automating it is tracked in docs/TODO.md)"
echo "  3) restart 'pnpm dev'"
echo "     # reloads the browser-side fingerprint from fingerprints.json"
