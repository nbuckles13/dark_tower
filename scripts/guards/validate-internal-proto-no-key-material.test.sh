#!/usr/bin/env bash
# coverage-exempt: drives validate-internal-proto-no-key-material.sh (a shell guard) against fixture trees; it never runs the dt-guard binary via $DT_GUARD
#
# Self-test for scripts/guards/simple/validate-internal-proto-no-key-material.sh
# (ADR-0036 §4 "No key material crosses the MC->MH contract"; rule @security, machinery
# @infrastructure).
#
# WHY IT EXISTS. The guard is green BY CONSTRUCTION on the real tree, so a green run
# exercises none of its failure branches: a path typo would turn it into a permanent no-op
# that reads exactly like a pass. Every case here asserts BOTH the exit code AND the
# distinct reason token, so a red for the wrong reason (e.g. vacuity where a content
# violation was meant) fails the case instead of passing it.
#
# NOT under guards/simple/: run-guards.sh discovers with `find … -name '*.sh'`, which
# matches `*.test.sh` too. Hermetic: mktemp + EXIT trap, no committed fixtures, no
# cargo/buf/network. Each case copies the REAL protos into a synthetic root and applies ONE
# mutation, so a fixture cannot drift from what it models.
set -euo pipefail
IFS=$'\n\t'
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
# shellcheck source=../lang/_test_helpers.sh
source "${REPO_ROOT}/scripts/lang/_test_helpers.sh"

GUARD="${REPO_ROOT}/scripts/guards/simple/validate-internal-proto-no-key-material.sh"
[[ -x "$GUARD" ]] || { printf '  - [precondition] guard missing/not executable at %s\n' "$GUARD"; printf '\n%s: 0 passed, 1 failed\n' "$0"; exit 1; }

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

REL_INTERNAL="proto/dark_tower/internal/v1/internal.proto"
REL_SIGNALING="proto/dark_tower/signaling/v1/signaling.proto"
ROOT="${WORK}/root"
INTERNAL="${ROOT}/${REL_INTERNAL}"
SIGNALING="${ROOT}/${REL_SIGNALING}"

reset_tree() {
  rm -rf "$ROOT"
  mkdir -p "$(dirname "$INTERNAL")" "$(dirname "$SIGNALING")"
  cp "${REPO_ROOT}/${REL_INTERNAL}" "$INTERNAL"
  cp "${REPO_ROOT}/${REL_SIGNALING}" "$SIGNALING"
}
run_guard() {
  OUT="$(DEVLOOP_TEST=1 INTERNAL_PROTO_GUARD_ROOT="$ROOT" bash "$GUARD" 2>&1)" && RC=0 || RC=$?
}
# Insert a line into RegisterMeetingRequest, right after its last pre-existing field, so
# the mutation sits in real field position (not in a comment, not outside a message).
plant_field() {
  local line="$1"
  local anchor='  repeated MutedSource server_muted_sources = 7;'
  grep -qxF "$anchor" "$INTERNAL" || { echo "self-test precondition: anchor not found in internal.proto" >&2; exit 1; }
  awk -v a="$anchor" -v l="$line" '{print} $0==a {print l}' "$INTERNAL" > "${INTERNAL}.new"
  mv "${INTERNAL}.new" "$INTERNAL"
}

# --- P0: REAL-TREE positive control, no seam (the run-in-place case) ---
# The sentinel's ABSENCE is CONSTRUCTED, never inherited (precedent:
# validate-subdomain-regex-sync.test.sh): an ambient DEVLOOP_TEST or a stale root in the
# caller's environment must not change which tree this case measures.
OUT="$(env -u DEVLOOP_TEST -u INTERNAL_PROTO_GUARD_ROOT bash "$GUARD" 2>&1)" && RC=0 || RC=$?
assert_exit "p0-real-tree-passes" 0 "$RC"

# --- P1: synthetic baseline = copy of the real tree passes (so a later red is the mutation) ---
reset_tree; run_guard
assert_exit "p1-synthetic-baseline-passes" 0 "$RC"

# --- Check 1 POSITIVES: every bytes spelling reds with the bytes token ---
i=0
for decl in \
  '  optional bytes k = 90;' \
  '  bytes k = 90;' \
  '  repeated bytes k = 90;' \
  '  map<string, bytes> k = 90;' \
  '  map<string,bytes> k = 90;' \
  '  google.protobuf.BytesValue k = 90;'; do
  i=$((i + 1))
  reset_tree; plant_field "$decl"; run_guard
  assert_exit   "c1-pos-${i}-reds"  1 "$RC"
  assert_status "c1-pos-${i}-token" "[internal-proto-bytes-field]" "$OUT"
  assert_absent "c1-pos-${i}-not-vacuous" "[internal-proto-scan-vacuous]" "$OUT"
done

# bytes inside a oneof (indented deeper) is still a field
reset_tree; plant_field $'  oneof choice {\n    bytes k = 90;\n  }'; run_guard
assert_exit   "c1-oneof-reds"  1 "$RC"
assert_status "c1-oneof-token" "[internal-proto-bytes-field]" "$OUT"

# --- Check 1 NEGATIVES: must stay green ---
reset_tree; plant_field '  uint64 total_bytes = 90;'; run_guard
assert_exit "c1-neg-total-bytes-green" 0 "$RC"
reset_tree; plant_field '  string bytes_sent_label = 90;'; run_guard
assert_exit "c1-neg-bytes-sent-label-green" 0 "$RC"
reset_tree; plant_field '  // a prose mention: never add a bytes k = 1; field here'; run_guard
assert_exit "c1-neg-comment-prose-green" 0 "$RC"

# --- Check 2 POSITIVES: any non-allowlisted / non-canonical qualified type reds ---
i=0
for decl in \
  '  dark_tower.signaling.v1.MeetingKekUpdate k = 90;' \
  '  signaling.v1.MeetingKekUpdate k = 90;' \
  '  .dark_tower.signaling.v1.MeetingKekUpdate k = 90;' \
  '  google.protobuf.Timestamp t = 90;' \
  '  google.protobuf.Any a = 90;' \
  '  signaling.v1.TransportMode m = 90;'; do
  i=$((i + 1))
  reset_tree; plant_field "$decl"; run_guard
  assert_exit   "c2-pos-${i}-reds"  1 "$RC"
  assert_status "c2-pos-${i}-token" "[internal-proto-cross-package-type]" "$OUT"
  assert_absent "c2-pos-${i}-not-bytes-token" "[internal-proto-bytes-field]" "$OUT"
done

# --- Check 2 NEGATIVES ---
reset_tree; plant_field '  dark_tower.signaling.v1.TransportMode extra_mode = 90;'; run_guard
assert_exit "c2-neg-canonical-transport-mode-green" 0 "$RC"
# The strip, not luck, carries the prose SSoT pointers.
reset_tree; plant_field '  // see dark_tower.signaling.v1.MeetingKekUpdate for why this is absent'; run_guard
assert_exit "c2-neg-comment-prose-green" 0 "$RC"

# --- Enums-only boundary: an allowlisted name declared as a MESSAGE reds ---
reset_tree
sed -i 's/^enum TransportMode {/message TransportMode {/' "$SIGNALING"
grep -q '^message TransportMode {' "$SIGNALING" || { echo "self-test precondition: TransportMode mutation did not apply" >&2; exit 1; }
run_guard
assert_exit   "enum-only-reds"  1 "$RC"
assert_status "enum-only-token" "[internal-proto-allowlist-not-enum]" "$OUT"

# --- Check 3: imports ---
reset_tree
sed -i 's|^import "dark_tower/signaling/v1/signaling.proto";|&\nimport "dark_tower/internal/v1/keys.proto";|' "$INTERNAL"
grep -q 'keys.proto' "$INTERNAL" || { echo "self-test precondition: import mutation did not apply" >&2; exit 1; }
run_guard
assert_exit   "c3-import-reds"  1 "$RC"
assert_status "c3-import-token" "[internal-proto-import-not-allowed]" "$OUT"
reset_tree
sed -i 's|^import "dark_tower/signaling/v1/signaling.proto";|import public "dark_tower/signaling/v1/signaling.proto";|' "$INTERNAL"
run_guard
assert_exit   "c3-import-public-form-reds"  1 "$RC"
assert_status "c3-import-public-form-token" "[internal-proto-import-not-allowed]" "$OUT"

# --- Whitespace bypasses protobuf ACCEPTS (verified with `buf build`): each must still red.
#     The guard scans a one-statement-per-line normalised form, so line splits and spaced
#     dots cannot slip past it. ---
# This case ALSO pins the SPACE-join in `normalise` (`tr '\n' ' '`): joined with the empty
# string, `bytes` + `raw` would become `bytesraw` and check 1 would miss it. Do not
# "simplify" the join to `tr -d '\n'`.
reset_tree; plant_field $'  bytes\n    raw = 90;'; run_guard
assert_exit   "ws-split-bytes-reds"  1 "$RC"
assert_status "ws-split-bytes-token" "[internal-proto-bytes-field]" "$OUT"
reset_tree; plant_field $'  dark_tower.signaling.v1\n    .MeetingKekUpdate split_type = 90;'; run_guard
assert_exit   "ws-split-type-reds"  1 "$RC"
assert_status "ws-split-type-token" "[internal-proto-cross-package-type]" "$OUT"
reset_tree; plant_field '  dark_tower . signaling . v1 . MeetingKekUpdate spaced_type = 90;'; run_guard
assert_exit   "ws-spaced-type-reds"  1 "$RC"
assert_status "ws-spaced-type-token" "[internal-proto-cross-package-type]" "$OUT"
reset_tree
sed -i 's|^import "dark_tower/signaling/v1/signaling.proto";|&\nimport"dark_tower/internal/v1/keys.proto";|' "$INTERNAL"
grep -q '^import"dark_tower/internal/v1/keys.proto";' "$INTERNAL" || { echo "self-test precondition: spaceless import mutation did not apply" >&2; exit 1; }
run_guard
assert_exit   "ws-spaceless-import-reds"  1 "$RC"
assert_status "ws-spaceless-import-token" "[internal-proto-import-not-allowed]" "$OUT"

# --- Vacuity: each refuses to report clean, with the vacuity token ---
reset_tree; rm -f "$INTERNAL"; run_guard
assert_exit   "vac-internal-missing-reds"  1 "$RC"
assert_status "vac-internal-missing-token" "[internal-proto-scan-vacuous]" "$OUT"
reset_tree; : > "$INTERNAL"; run_guard
assert_exit   "vac-internal-empty-reds"  1 "$RC"
assert_status "vac-internal-empty-token" "[internal-proto-scan-vacuous]" "$OUT"
reset_tree; sed -i 's/^package dark_tower\.internal\.v1;/package dark_tower.other.v1;/' "$INTERNAL"; run_guard
assert_exit   "vac-identity-markers-reds"  1 "$RC"
assert_status "vac-identity-markers-token" "[internal-proto-scan-vacuous]" "$OUT"
reset_tree; rm -f "$SIGNALING"; run_guard
assert_exit   "vac-signaling-missing-reds"  1 "$RC"
assert_status "vac-signaling-missing-token" "[internal-proto-scan-vacuous]" "$OUT"
reset_tree; sed -i 's/^enum TransportMode {/enum RenamedMode {/' "$SIGNALING"; run_guard
assert_exit   "vac-allowlist-entry-absent-reds"  1 "$RC"
assert_status "vac-allowlist-entry-absent-token" "[internal-proto-scan-vacuous]" "$OUT"
reset_tree; plant_field '  /* block comment the line strip cannot see into */'; run_guard
assert_exit   "vac-block-comment-reds"  1 "$RC"
assert_status "vac-block-comment-token" "[internal-proto-scan-vacuous]" "$OUT"

# --- Seam is test-gated: without DEVLOOP_TEST the root override is ignored ---
reset_tree; rm -f "$INTERNAL"
OUT="$(env -u DEVLOOP_TEST INTERNAL_PROTO_GUARD_ROOT="$ROOT" bash "$GUARD" 2>&1)" && RC=0 || RC=$?
assert_exit "seam-ignored-without-devloop-test" 0 "$RC"

report_results "scripts/guards/validate-internal-proto-no-key-material.test.sh"
