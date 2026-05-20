#!/usr/bin/env bash
#
# verify-codegen.sh — smoke-test the TS proto codegen pipeline.
#
# Invariant we check: `buf generate` (run from `proto/`) produces non-empty
# `_pb.ts` files in `packages/sdk-core/src/proto/`, each containing the
# expected exported symbol from its source `.proto`. Catches:
#   - silent codegen plugin failure (exit 0 but no output)
#   - plugin/option drift that produces files with different names or strips
#     message classes (e.g. wrong target=, wrong import_extension, plugin swap) —
#     the pre-generate clean ensures stale outputs from a prior config can't
#     mask a broken current config (per @test Gate 3 finding 2026-05-06)
#   - wrong output directory layout
#
# Invoked by the Nx target `proto-gen:test` (cwd: packages/proto-gen).
set -euo pipefail

# Resolve repo root from this script's location (allow direct invocation too).
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"

OUT_DIR="${REPO_ROOT}/packages/sdk-core/src/proto"

# Clean any prior outputs so this run's assertions cannot be satisfied by stale
# files left behind by a previous (possibly differently-configured) codegen run.
mkdir -p "${OUT_DIR}"
find "${OUT_DIR}" -type f \( -name '*_pb.ts' -o -name '*_pb.js' -o -name '*_pb.d.ts' \) -delete

cd "${REPO_ROOT}/proto"
pnpm exec buf generate

assert_generated() {
  local file="$1"
  local symbol="$2"
  local path="${OUT_DIR}/${file}"

  if [[ ! -f "${path}" ]]; then
    echo "verify-codegen: FAIL — expected file not produced: ${path}" >&2
    exit 1
  fi
  if [[ ! -s "${path}" ]]; then
    echo "verify-codegen: FAIL — generated file is empty: ${path}" >&2
    exit 1
  fi
  if ! grep -q -- "${symbol}" "${path}"; then
    echo "verify-codegen: FAIL — generated file missing expected symbol '${symbol}': ${path}" >&2
    exit 1
  fi
  echo "verify-codegen: OK — ${file} (contains ${symbol})"
}

assert_generated "dark_tower/signaling/v1/signaling_pb.ts" "JoinRequest"
assert_generated "dark_tower/internal/v1/internal_pb.ts" "RegisterRequest"
assert_generated "dark_tower/internal/v1/internal_pb.ts" "FastHeartbeatResponse"
assert_generated "dark_tower/internal/v1/internal_pb.ts" "ComprehensiveHeartbeatResponse"

echo "verify-codegen: all checks passed"
