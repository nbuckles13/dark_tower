#!/usr/bin/env bash
# Guard: NO KEY MATERIAL CROSSES THE MC->MH CONTRACT (ADR-0036 §4) — mechanised for
# `proto/dark_tower/internal/v1/internal.proto`.
#
# Rule content: @security. Machinery: @infrastructure (CLAUDE.md guard-ownership split:
# allowlist MEMBERSHIP is content; the construct and its evaluation are machinery).
# Runbook: docs/runbooks/devloop-validation.md §6.3
#
# WHAT IT ENFORCES. Three checks, each a mechanical PROXY for "no key material on the
# internal contract" — not the rule itself, which is why each is stated with its bound:
#
#   1. internal-proto-bytes-field — no `bytes` field in any spelling (bare, `repeated`,
#      `optional`, oneof member, `map<..., bytes>`) and no `google.protobuf.BytesValue`.
#      Every piece of key material in this tree is `bytes`. `\bbytes([[:space:]>]|$)` is what
#      keeps a byte-COUNTER field name (`total_bytes`, `bytes_sent_label`) green and what
#      covers the map form without a separate clause.
#   2. internal-proto-cross-package-type — no qualified type reference except an
#      allowlisted ENUM, compared by EXACT canonical fully-qualified name. Check 1 alone is
#      defeated by embedding a signaling MESSAGE (`MeetingKekUpdate` carries `meeting_kek`)
#      — key material arrives without the word `bytes` in this file. The extractor matches
#      ANY dotted reference with a capitalised last segment, so relative
#      (`signaling.v1.X`), leading-dot and `google.protobuf.*` spellings are all caught.
#      FULLY-QUALIFIED SPELLING IS REQUIRED ON PURPOSE: a red on a relative spelling of an
#      allowlisted enum is intended — fix the spelling, never loosen the comparison.
#      Each allowlist entry must also be declared `enum <Name> {` in signaling.proto; if it
#      is declared as a message the guard reds (internal-proto-allowlist-not-enum), which
#      makes the enums-only boundary structural rather than a comment someone edits past.
#   3. internal-proto-import-not-allowed — every `import` must be an exact allowlisted
#      form. A same-package import would let a message be referenced UNQUALIFIED, which
#      check 2 cannot see. BOUND, stated honestly: this covers internal.proto's DIRECT
#      imports only; transitive visibility via an `import public` inside an allowlisted
#      file is out of scope.
#
# Every check runs on COMMENT-STRIPPED text (`//` to end of line). The strip is load-bearing
# for both checks: it removes the prose mention of `bytes` in the file and every prose
# `dark_tower.signaling.v1.*` SSoT pointer. `/* */` block comments are REJECTED rather than
# half-handled, because the line strip cannot see inside them.
#
# NORMALISED INPUT. The three checks run on a ONE-STATEMENT-PER-LINE form (lines joined,
# whitespace collapsed, split at `;` `{` `}`), so protobuf's permissive whitespace — `bytes`
# and its field name on separate lines, `a . K`, `import"x";` — cannot slip past a
# line-oriented match. The guard is therefore sound standalone and does not rely on the proto
# fmt lane having normalised the file first.
#
# VACUITY (internal-proto-scan-vacuous). A grep over a vanished path returns "no match",
# which reads as green. So the guard refuses to report clean unless it demonstrably read the
# right, non-empty files: both protos exist and are non-empty, internal.proto carries its
# identity markers, every allowlist entry exists in signaling.proto, and every grep exits
# 0 or 1 (anything else is a read failure, not a pass). The vacuity text deliberately
# differs from the content text so a vacuity red is never "fixed" by loosening a pattern.
#
# NO SUPPRESSION PATH: no ignore marker is honoured, no env var skips it, no flag excludes
# a line. A legitimate need is a re-derivation of the rule with @security, not an edit here.
#
# Why bash+grep and not a dt-guard subcommand: this is a grep-shaped anchor guard over two
# fixed files (same class as validate-ts-fmt-proto-excluded / validate-frame-vectors) — no
# parser, no cargo, hermetic in Layer 3. Sources ../common.sh for ONE helper only
# (guard_seam_root, the shared root seam); common.sh's top level only defines functions,
# colour strings and two arrays, so it changes neither set -euo/IFS nor stdout/STATUS.
#
# Emits NO `STATUS=` line: run-guards.sh owns STATUS emission. Exit 0 = clean, 1 = any
# violation or vacuity. Messages go to stderr. Self-test:
# scripts/guards/validate-internal-proto-no-key-material.test.sh
set -euo pipefail
IFS=$'\n\t'

__here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"   # scripts/guards/simple
__real_root="$(cd "${__here}/../../.." && pwd)"

# shellcheck source=../common.sh
source "${__here}/../common.sh"
# Root seam (INTERNAL_PROTO_GUARD_ROOT), DEVLOOP_TEST-gated — rationale on guard_seam_root.
# ONE root, both paths derived from it: two file seams would let a self-test redirect one
# file and silently keep the real other. run-guards.sh's `$1` search path is ignored.
ROOT="$(guard_seam_root INTERNAL_PROTO_GUARD_ROOT "$__real_root")"

INTERNAL="${ROOT}/proto/dark_tower/internal/v1/internal.proto"
SIGNALING="${ROOT}/proto/dark_tower/signaling/v1/signaling.proto"

# ---- security-owned CONTENT under a machinery-owned construct ----------------------------
# Cross-package types permitted on the internal contract. ENUMS ONLY: an enum cannot carry
# a `bytes` field, so it needs no per-entry judgement; a MESSAGE type is exactly the hole
# check 2 exists to close. Adding a message type here requires @security sign-off, and the
# guard reds it regardless (internal-proto-allowlist-not-enum). Canonical FQN, one per line.
readonly ALLOWED_TYPES=(
  "dark_tower.signaling.v1.TransportMode"
)
# Imports permitted, as the exact quoted path.
readonly ALLOWED_IMPORTS=(
  "dark_tower/signaling/v1/signaling.proto"
)
# -------------------------------------------------------------------------------------------

RULE='ADR-0036 §4 "No key material crosses the MC->MH contract"'
ROUTE='This guard is a mechanical proxy for that rule. A legitimate need is a re-derivation of the rule with @security — not a field deletion, and not an edit to this guard (it has no suppression path).'

fail=0
vacuous() {
  echo "VIOLATION [internal-proto-scan-vacuous] $* — the guard could not evaluate its input, so it refuses to report clean. This is NOT a content violation: do not loosen a pattern; restore the input." >&2
  fail=1
}
violation() {   # $1 token, $2 check description, $3 offending lines
  echo "VIOLATION [$1] ${2} in proto/dark_tower/internal/v1/internal.proto:" >&2
  printf '%s\n' "$3" | sed 's/^/    /' >&2
  echo "  Enforces ${RULE}. ${ROUTE}" >&2
  fail=1
}

# ---- vacuity preconditions ----
for f in "$INTERNAL" "$SIGNALING"; do
  if [[ ! -f "$f" ]]; then
    vacuous "file not found: ${f#"${ROOT}"/}"
  elif [[ ! -s "$f" ]]; then
    vacuous "file is empty: ${f#"${ROOT}"/}"
  fi
done
[[ "$fail" -eq 0 ]] || exit 1

grep -qE '^package dark_tower\.internal\.v1;' "$INTERNAL" \
  || vacuous "identity marker 'package dark_tower.internal.v1;' absent — the scanned file is not the internal contract"
grep -qE '^service MediaHandlerService' "$INTERNAL" \
  || vacuous "identity marker 'service MediaHandlerService' absent — the scanned file is not the internal contract"
[[ "$fail" -eq 0 ]] || exit 1

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
# Strip `//` comments first: the block-comment check below and `normalise` both need them
# gone. Findings cite statement text, not source line numbers (see `normalise`).
sed 's://.*::' "$INTERNAL" > "${work}/internal.stripped"
sed 's://.*::' "$SIGNALING" > "${work}/signaling.stripped"

# A `/*` OUTSIDE a `//` comment opens a block comment the line strip cannot see into.
# (Checked on stripped text, so prose such as a `/Service/*` path glob inside a `//`
# comment does not trip it.)
if grep -qF '/*' "${work}/internal.stripped"; then
  vacuous "a '/*' block comment is present; the '//' line strip cannot see inside it, so the file cannot be scanned soundly. Use '//' comments"
  exit 1
fi

# ---- normalise to ONE STATEMENT PER LINE ----
# All three checks match per line, and protobuf accepts whitespace that would defeat a
# line-oriented scan: `bytes` and its field name on separate lines, a qualified type split
# across lines or written `a . K`, `import"x";` with no space. `buf format` would normalise
# these, but this guard runs standalone through run-guards.sh and must be sound on its own
# input rather than lean on another layer. So: join lines, collapse whitespace around `.`
# and elsewhere, and break at `;` `{` `}`. Runs AFTER the `//` strip. Reported findings
# cite the offending STATEMENT text, since source line numbers do not survive the join.
normalise() {
  tr '\n' ' ' < "$1" \
    | sed -E 's/[[:space:]]*\.[[:space:]]*/./g; s/[[:space:]]+/ /g' \
    | tr ';{}' '\n\n\n'
}
normalise "${work}/internal.stripped" > "${work}/internal.stmts"
normalise "${work}/signaling.stripped" > "${work}/signaling.stmts"

# ---- check 1: bytes fields ----
# `|$` (security): a `bytes` token ending a normalised statement is still a bytes field.
out="$(grep -E '\bbytes([[:space:]>]|$)|BytesValue' "${work}/internal.stmts")" && rc=0 || rc=$?
case "$rc" in
  0) violation internal-proto-bytes-field "a bytes-typed field" "$out" ;;
  1) ;;
  *) vacuous "check 1 grep failed (rc=${rc})" ;;
esac

# ---- check 2: cross-package types, exact canonical FQN, enums only ----
refs="$(grep -oE '\.?\b([A-Za-z_][A-Za-z0-9_]*\.)+[A-Z][A-Za-z0-9_]*' "${work}/internal.stmts")" && rc=0 || rc=$?
case "$rc" in
  0|1) ;;
  *) vacuous "check 2 grep failed (rc=${rc})"; refs="" ;;
esac
bad_refs=""
while IFS= read -r name; do
  [[ -n "$name" ]] || continue
  allowed=0
  for t in "${ALLOWED_TYPES[@]}"; do [[ "$name" == "$t" ]] && allowed=1; done
  [[ "$allowed" -eq 1 ]] || bad_refs+="${name}"$'\n'
done <<< "$refs"
if [[ -n "$bad_refs" ]]; then
  violation internal-proto-cross-package-type \
    "a cross-package type that is not an allowlisted enum spelled by its canonical fully-qualified name (allowed: ${ALLOWED_TYPES[*]})" \
    "${bad_refs%$'\n'}"
fi
for t in "${ALLOWED_TYPES[@]}"; do
  short="${t##*.}"
  if grep -qE "^[[:space:]]*enum[[:space:]]+${short}[[:space:]]*$" "${work}/signaling.stmts"; then
    :
  elif grep -qE "^[[:space:]]*message[[:space:]]+${short}[[:space:]]*$" "${work}/signaling.stmts"; then
    echo "VIOLATION [internal-proto-allowlist-not-enum] allowlist entry '${t}' is declared as a MESSAGE in signaling.proto. The allowlist is enums-only: a message type can carry a bytes field, which is the hole check 2 closes. Remove it from ALLOWED_TYPES; widening the boundary is a re-derivation with @security." >&2
    fail=1
  else
    vacuous "allowlist entry '${t}' is not declared in signaling.proto at all"
  fi
done

# ---- check 3: imports ----
# `\b`, not `[[:space:]]`, after `import`: buf accepts `import"x";`. Any non-canonical form
# then reds under the exact-form comparison, consistent with check 2's canonical-spelling rule.
imports="$(grep -E '^[[:space:]]*import\b' "${work}/internal.stmts")" && rc=0 || rc=$?
case "$rc" in
  0|1) ;;
  *) vacuous "check 3 grep failed (rc=${rc})"; imports="" ;;
esac
bad_imports=""
while IFS= read -r body; do
  [[ -n "$body" ]] || continue
  ok=0
  for imp in "${ALLOWED_IMPORTS[@]}"; do
    [[ "$body" =~ ^[[:space:]]*import[[:space:]]+\"${imp//./\\.}\"[[:space:]]*$ ]] && ok=1
  done
  [[ "$ok" -eq 1 ]] || bad_imports+="${body}"$'\n'
done <<< "$imports"
if [[ -n "$bad_imports" ]]; then
  violation internal-proto-import-not-allowed \
    "an import outside the allowlist (allowed: ${ALLOWED_IMPORTS[*]}, plain form only)" \
    "${bad_imports%$'\n'}"
fi

exit "$fail"
