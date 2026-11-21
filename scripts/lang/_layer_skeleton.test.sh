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
# Skip `layerN.test.sh` self-tests: the glob `layer[0-9]*.sh` also matches `layer7.test.sh`
# (and any future `layerN.test.sh`), but a test file legitimately contains the very tokens
# this linter forbids in a layer BODY — raw `STATUS=`/`aggregate_worst_status` in assertion
# strings, a `trap 'rm -rf' EXIT` cleanup, etc. Scanning them would false-fail (task #56).
shopt -s nullglob
declare -a LAYERS=()
for f in "${SCRIPTS_ROOT}"/layer[0-9]*.sh; do
  [[ "$f" == *.test.sh ]] && continue
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
# __check_child_rc <file> — every pipe INTO tee_collect_statuses from a real producer (a
# dispatcher / guard group; `emit_status … |` lines are exempt — a builtin STATUS print cannot
# be cut short) must record the producer's exit: `… | tee_collect_statuses ||
# layer_lifecycle_child_rc "$?"`. A discarded exit (`|| true`, or nothing) lets a dispatcher
# killed after one language reported OK render the layer OK over languages that never ran.
# Prints each offending line; returns 1 if any.
__check_child_rc() {
  local bad
  bad="$(grep -nE '\|[[:space:]]*tee_collect_statuses' "$1" | grep -vE '^[0-9]+:[[:space:]]*#' \
          | grep -vE 'emit_status[^|]*\|[[:space:]]*tee_collect_statuses' \
          | grep -vE '\|[[:space:]]*tee_collect_statuses[[:space:]]*\|\|[[:space:]]*layer_lifecycle_child_rc[[:space:]]+"\$\?"[[:space:]]*$')"
  [[ -z "$bad" ]] && return 0
  printf '%s\n' "$bad"
  return 1
}
# The check must FIRE (negative fixture): a discarded and an unguarded dispatcher pipe.
__neg="$(mktemp)"
printf '%s\n' '"$(dirname "$0")/fmt.sh" 2>&1 | tee_collect_statuses || true' '"$x/test.sh" | tee_collect_statuses' > "$__neg"
if [[ "$(__check_child_rc "$__neg" | wc -l)" -eq 2 ]]; then PASS=$((PASS + 1)); else
  FAIL=$((FAIL + 1)); FAILURES+=("[child-rc-check] negative fixture did not fire on both bad shapes"); fi
printf '%s\n' '"$x/lint.sh" 2>&1 | tee_collect_statuses || layer_lifecycle_child_rc "$?"' 'emit_status OK r | tee_collect_statuses' > "$__neg"
if __check_child_rc "$__neg" >/dev/null; then PASS=$((PASS + 1)); else
  FAIL=$((FAIL + 1)); FAILURES+=("[child-rc-check] positive fixture was flagged"); fi
rm -f "$__neg"

for layer in "${LAYERS[@]}"; do
  __assert_no_match "$layer" "echo STATUS="    '(^|\s)(echo|printf)\s+["'\'']?STATUS='
  __assert_no_match "$layer" "echo LAYER="     '(^|\s)(echo|printf)\s+["'\'']?LAYER='
  __assert_no_match "$layer" "date +%s"        '\bdate\b[^|]*\+%s'
  __assert_no_match "$layer" "aggregate_worst_status" '\baggregate_worst_status\b'
  __assert_no_match "$layer" "trap installation"     '^\s*trap\s'
    # REQUIRED: a dispatcher pipe records its exit (layer_lifecycle_child_rc).
  if __bad_rc="$(__check_child_rc "$layer")"; then
    PASS=$((PASS + 1))
  else
    FAIL=$((FAIL + 1))
    FAILURES+=("[$(basename "$layer")] dispatcher pipe discards its exit (needs '|| layer_lifecycle_child_rc \"\$?\"'):
${__bad_rc}")
  fi
# REQUIRED: the lifecycle normal-end mark (_common.sh layer_lifecycle_done). Without it the
  # layer renders STATUS=UNKNOWN layer-incomplete on every run — loud at runtime anyway, but
  # caught here first, at Layer 3, with the fix named.
  if grep -qE '^\s*layer_lifecycle_done\s*$' "$layer"; then
    PASS=$((PASS + 1))
  else
    FAIL=$((FAIL + 1))
    FAILURES+=("[$(basename "$layer")] missing required 'layer_lifecycle_done' (the lifecycle normal-end mark)
  → call it as the layer's last statement and before every deliberate exit")
  fi
done

printf '\n_layer_skeleton.test.sh: %d passed, %d failed\n' "$PASS" "$FAIL"
if [[ $FAIL -gt 0 ]]; then
  printf 'Failures:\n'
  for f in "${FAILURES[@]}"; do
    printf '%s\n' "$f"
  done
  exit 1
fi
exit 0
