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

# Absence half of the oracle. Presence greps alone would stay green through an
# entire deletion set: the ADR-0036 reshape removes twelve symbols, and a
# half-done deletion or a rename-back must FAIL this script rather than pass it
# quietly (@test, Gate 1).
#
# Named `assert_not_generated`, deliberately NOT `assert_absent`:
# `scripts/lang/_test_helpers.sh:60` already defines that name with an
# incompatible contract (tally + `report_results` vs this script's fail-fast
# `exit 1`). Two helpers sharing a name would shadow silently if this script
# ever sourced that file, so it pairs with the local `assert_generated` instead.
#
# Matches the EXPORT DECLARATION, not a bare token, so a surviving mention in a
# doc comment cannot produce a false pass.
assert_not_generated() {
  local file="$1"
  local symbol="$2"
  local path="${OUT_DIR}/${file}"

  # Fail loudly on a missing file rather than passing open. Without this, a
  # missing/renamed target makes `grep -qE` return a file-error rc, the `if`
  # below reads it as "not found", and every absence assert prints a false
  # "OK — absent". Today only ordering saves it (a presence assert on the same
  # path runs first); the oracle must not depend on that (@test, Gate 3).
  if [[ ! -f "${path}" ]]; then
    echo "verify-codegen: FAIL — cannot check absence, file not produced: ${path}" >&2
    exit 1
  fi
  if grep -qE "^export (type|enum|const) ${symbol}\\b" "${path}"; then
    echo "verify-codegen: FAIL — deleted symbol '${symbol}' is still generated: ${path}" >&2
    exit 1
  fi
  echo "verify-codegen: OK — ${symbol} absent from ${file}"
}

assert_generated "dark_tower/signaling/v1/signaling_pb.ts" "JoinRequest"
assert_generated "dark_tower/internal/v1/internal_pb.ts" "FastHeartbeatResponse"
assert_generated "dark_tower/internal/v1/internal_pb.ts" "ComprehensiveHeartbeatResponse"

# ADR-0036 internal contract (2026-09-01 reshape) — the MC→MH control-plane
# shapes must exist. `RegisterRequest` was previously asserted here; it is one of
# the eight symbols the reshape deletes and is asserted ABSENT below.
for symbol in \
  RegisterMeetingRequest \
  RegisterMeetingResponse \
  EgressStream \
  SubscriberSlot \
  CandidateSource \
  SelectionRules; do
  assert_generated "dark_tower/internal/v1/internal_pb.ts" "${symbol}"
done

# ADR-0036 internal contract — the retired shapes must be GONE.
#
# All eight, not a sample. A presence-only oracle stays green through a
# half-done deletion or a rename-back, which is the same reasoning that added
# the signalling absence block below. Note `RegisterResponse` is asserted absent
# from `internal_pb.ts` ONLY: `packages/sdk-core/src/http/types.ts` defines an
# unrelated hand-written AC HTTP type of the same name, which is why this
# assertion is file-scoped and why the deletion audit had to check both.
for symbol in \
  RegisterRequest \
  RegisterResponse \
  RoutingOptions \
  CascadeDestination \
  RouteMediaRequest \
  RouteMediaResponse \
  StreamTelemetryRequest \
  StreamTelemetryResponse; do
  assert_not_generated "dark_tower/internal/v1/internal_pb.ts" "${symbol}"
done

# ADR-0036 signalling contract — the media-path shapes must exist.
for symbol in \
  ReceiveCapability \
  ReceiveSlot \
  SendDirective \
  SendTarget \
  StreamAssignment \
  MeetingKekUpdate \
  ServerMuteRequest \
  EncodingParameters; do
  assert_generated "dark_tower/signaling/v1/signaling_pb.ts" "${symbol}"
done

for symbol in MediaKind Codec TransportMode SlotState; do
  assert_generated "dark_tower/signaling/v1/signaling_pb.ts" "${symbol}"
done

# ADR-0036 signalling contract — the superseded shapes must be GONE.
# `StreamAssignment` is deliberately absent from this list: the name is reused
# by the new shape, so an absence assert on it would contradict the presence
# assert above.
for symbol in \
  HostMuteRequest \
  EncryptionKeys \
  SubscribeToLayout \
  UpdateLayout \
  UnsubscribeLayout \
  LayoutConfig \
  LayoutType \
  StreamType \
  MediaType \
  StreamMetadata \
  VideoMetadata \
  SimulcastLayer; do
  assert_not_generated "dark_tower/signaling/v1/signaling_pb.ts" "${symbol}"
done

echo "verify-codegen: all checks passed"
