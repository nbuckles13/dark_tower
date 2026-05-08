#!/usr/bin/env bash
# _layer_skeleton.test.sh — meta-test enforcing layer-script skeleton conventions.
#
# Per dry-reviewer: structurally enforce that layerN.sh bodies stay minimal and
# that the lifecycle-helper ownership of timestamps + LAYER/STATUS emission +
# STATUS aggregation + traps is not bypassed by future contributors.
#
# Forbidden patterns in scripts/layer{1..N}.sh (and any future layer*.sh):
#   - direct `date +%s` / `date "+%s"` calls (lifecycle owns timestamps)
#   - raw `echo "LAYER=` / `printf 'LAYER=` (lifecycle owns LAYER stderr)
#   - raw `echo "STATUS=` / `printf 'STATUS=` (use emit_status / run_and_emit)
#   - `aggregate_worst_status` calls (only __layer_lifecycle_end may call it)
#   - `trap ` installations (only the lifecycle helper installs traps)
#
# The lifecycle helper itself (in _common.sh) is allowed to do all of the above —
# this test only scans layerN.sh files.
#
# Hermetic: scans live workspace files (read-only).

set -euo pipefail
IFS=$'\n\t'

__here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SCRIPTS_ROOT="$(cd "${__here}/.." && pwd)"

PASS=0
FAIL=0
FAILURES=()

# Collect layer scripts (layer1.sh ... layerN.sh, excluding layer-all.sh).
shopt -s nullglob
declare -a LAYERS=()
for f in "${SCRIPTS_ROOT}"/layer[0-9]*.sh; do
  LAYERS+=("$f")
done
shopt -u nullglob

if [[ ${#LAYERS[@]} -eq 0 ]]; then
  echo "_layer_skeleton.test.sh: no layer scripts found under ${SCRIPTS_ROOT}" >&2
  exit 1
fi

# Args: $1=layer-file  $2=label  $3=extended-regex
# Side effect: PASS++ on no match, FAIL++ + FAILURES[] on match
__assert_no_match() {
  local file="$1" label="$2" pattern="$3"
  if grep -nE "$pattern" "$file" >/dev/null 2>&1; then
    FAIL=$((FAIL + 1))
    local hits
    hits=$(grep -nE "$pattern" "$file")
    FAILURES+=("[$(basename "$file")] forbidden pattern '${label}' matched:
${hits}
  → move this responsibility into _common.sh lifecycle helper")
  else
    PASS=$((PASS + 1))
  fi
}

# Forbidden patterns. Tuned to catch real anti-patterns without false-positives:
#  - "echo \"STATUS=" / "printf 'STATUS=" → raw stdout emission (use emit_status)
#  - "echo \"LAYER=" / "printf 'LAYER=" → raw stderr emission (lifecycle owns it)
#  - "date +%s" / 'date "+%s"' / "date '+%s'" → direct timestamp call
#  - "aggregate_worst_status" anywhere except _common.sh internals
#  - "^\s*trap " → trap installation in body
for layer in "${LAYERS[@]}"; do
  __assert_no_match "$layer" "echo STATUS="    '(^|\s)(echo|printf)\s+["'\'']?STATUS='
  __assert_no_match "$layer" "echo LAYER="     '(^|\s)(echo|printf)\s+["'\'']?LAYER='
  __assert_no_match "$layer" "date +%s"        '\bdate\b[^|]*\+%s'
  __assert_no_match "$layer" "aggregate_worst_status" '\baggregate_worst_status\b'
  __assert_no_match "$layer" "trap installation"     '^\s*trap\s'
done

# Required-invocation check: layer3.sh MUST invoke _test_changed_predicates.sh
# per ADR-0033 implementation note (test-reviewer Finding 2).
# A future refactor that moves guard logic out without preserving this call
# silently weakens predicate-drift detection — catch it here.
if grep -q '_test_changed_predicates' "${SCRIPTS_ROOT}/layer3.sh"; then
  PASS=$((PASS + 1))
else
  FAIL=$((FAIL + 1))
  FAILURES+=("[layer3.sh] missing required invocation of _test_changed_predicates.sh
  → ADR-0033 implementation note: meta-test must run on every devloop, not just at PR time")
fi

printf '\n_layer_skeleton.test.sh: %d passed, %d failed\n' "$PASS" "$FAIL"
if [[ $FAIL -gt 0 ]]; then
  printf 'Failures:\n'
  for f in "${FAILURES[@]}"; do
    printf '%s\n' "$f"
  done
  exit 1
fi
exit 0
