#!/usr/bin/env bash
# _common.test.sh — STATUS aggregation precedence test (test §D).
#
# Encodes the canonical precedence as a spec test that fails if anyone reorders.
# Precedence (code-reviewer locked, re-confirmed Wave 2 #4 α):
#   FAIL > N/A > OK > SKIPPED-NO-DIFF > SKIPPED-NO-VERB
# Rationale: see _common.sh comment block above aggregate_worst_status.
set -euo pipefail
IFS=$'\n\t'

__here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=_common.sh
source "${__here}/_common.sh"

PASS=0
FAIL=0
FAILURES=()

assert_aggregate() {
  local expected="$1"; shift
  local actual
  actual=$(aggregate_worst_status "$@")
  if [[ "$actual" == "$expected" ]]; then
    PASS=$((PASS + 1))
  else
    FAIL=$((FAIL + 1))
    FAILURES+=("aggregate_worst_status $* → expected=${expected} actual=${actual}")
  fi
}

# Trivial cases.
assert_aggregate "OK" "OK" "OK"
assert_aggregate "OK" "OK"
assert_aggregate "OK"  # zero args → OK

# Single-step elevations (under α: OK ranks above SKIPPED-*; N/A and FAIL still beat OK).
assert_aggregate "OK"              "OK" "SKIPPED-NO-VERB"   # α: OK now wins over SKIPPED-NO-VERB
assert_aggregate "OK"              "OK" "SKIPPED-NO-DIFF"   # α: OK now wins over SKIPPED-NO-DIFF
assert_aggregate "N/A"             "OK" "N/A"               # N/A beats OK (deliberate documented gap)
assert_aggregate "FAIL"            "OK" "FAIL"              # FAIL beats OK

# Cross-precedence (code-reviewer locked, re-confirmed α).
assert_aggregate "SKIPPED-NO-DIFF" "SKIPPED-NO-VERB" "SKIPPED-NO-DIFF"   # NO-DIFF beats NO-VERB
assert_aggregate "N/A"             "SKIPPED-NO-DIFF" "N/A"               # N/A beats NO-DIFF
assert_aggregate "N/A"             "N/A" "SKIPPED-NO-VERB"               # N/A beats NO-VERB
assert_aggregate "FAIL"            "FAIL" "N/A"                          # FAIL beats N/A
assert_aggregate "FAIL"            "OK" "FAIL"                           # FAIL beats OK

# Multi-arg cases.
assert_aggregate "FAIL"            "OK" "OK" "FAIL" "OK"
assert_aggregate "N/A"             "OK" "SKIPPED-NO-DIFF" "N/A" "SKIPPED-NO-VERB"
assert_aggregate "OK"              "OK" "OK" "OK"

# Wave 2 #4 multi-lang success path (α invariant): rust=OK + ts=SKIPPED-NO-VERB
# (no compile.sh yet) + proto=SKIPPED-NO-DIFF (untouched) aggregates to OK.
# This is the regression case the α re-rank exists to fix — without α, the
# layer would report SKIPPED-NO-DIFF on a clean rust-only edit.
assert_aggregate "OK" "OK" "SKIPPED-NO-VERB" "SKIPPED-NO-DIFF"

# emit_status formatting.
out=$(emit_status OK "test-passed")
if [[ "$out" == "STATUS=OK REASON=test-passed" ]]; then
  PASS=$((PASS + 1))
else
  FAIL=$((FAIL + 1))
  FAILURES+=("emit_status formatting: got '${out}'")
fi

# parse_status_line with multiple STATUS lines (last wins).
tmp=$(mktemp)
{
  echo "STATUS=OK REASON=first"
  echo "some intermediate output"
  echo "STATUS=FAIL REASON=second"
} > "$tmp"
parsed=$(parse_status_line "$tmp")
rm -f "$tmp"
if [[ "$parsed" == "FAIL" ]]; then
  PASS=$((PASS + 1))
else
  FAIL=$((FAIL + 1))
  FAILURES+=("parse_status_line: expected FAIL, got '${parsed}'")
fi

# Direct test of tee_collect_statuses (test-reviewer Finding 3).
# Risk: a regression that breaks lastpipe (or makes tee_collect_statuses run
# in a subshell) wouldn't fail anything until layer aggregation silently goes
# UNKNOWN. Test the streaming primitive directly so the regression surfaces here.
#
# NOTE: command substitution `$(... | tee_collect_statuses)` runs the pipeline
# in a subshell regardless of lastpipe — would break the __LAYER_STATUSES
# mutation invariant. So we run the pipeline at top-level command position
# (where lastpipe applies) and capture stdout via tempfile.
__LAYER_STATUSES=()
__tee_in=$(mktemp)
__tee_out=$(mktemp)
printf 'STATUS=OK REASON=a\nintermediate\nSTATUS=FAIL REASON=b\n' > "$__tee_in"
tee_collect_statuses < "$__tee_in" > "$__tee_out"
__tee_stdout=$(<"$__tee_out")
rm -f "$__tee_in" "$__tee_out"

# (a) verbatim streaming: stdout includes both STATUS lines + intermediate text
if [[ "$__tee_stdout" == *'STATUS=OK REASON=a'* ]] \
   && [[ "$__tee_stdout" == *'intermediate'*    ]] \
   && [[ "$__tee_stdout" == *'STATUS=FAIL REASON=b'* ]]; then
  PASS=$((PASS + 1))
else
  FAIL=$((FAIL + 1))
  FAILURES+=("tee_collect_statuses: stdout missing expected verbatim content; got: ${__tee_stdout}")
fi
# (b) STATUS values collected into __LAYER_STATUSES (requires lastpipe parent-shell mutation)
if [[ "${#__LAYER_STATUSES[@]}" -eq 2 ]] \
   && [[ "${__LAYER_STATUSES[0]}" == "OK" ]] \
   && [[ "${__LAYER_STATUSES[1]}" == "FAIL" ]]; then
  PASS=$((PASS + 1))
else
  FAIL=$((FAIL + 1))
  FAILURES+=("tee_collect_statuses: __LAYER_STATUSES expected (OK FAIL), got (${__LAYER_STATUSES[*]:-empty})
  → likely lastpipe disabled or regression made tee_collect_statuses run in a subshell")
fi

# Summary.
printf '\n_common.test.sh: %d passed, %d failed\n' "$PASS" "$FAIL"
if [[ $FAIL -gt 0 ]]; then
  printf 'Failures:\n'
  for f in "${FAILURES[@]}"; do
    printf '  - %s\n' "$f"
  done
  emit_status FAIL "common-tests-failed"
  exit 1
fi
emit_status OK "common-tests-passed"
