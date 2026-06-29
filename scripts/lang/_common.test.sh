#!/usr/bin/env bash
# _common.test.sh — STATUS aggregation precedence test (test §D).
#
# Encodes the canonical precedence as a spec test that fails if anyone reorders.
# Precedence (task #52 — FAIL-MISSING-VERB inserted at rank 5):
#   UNKNOWN > FAIL-MISSING-VERB > FAIL > N/A > OK > SKIPPED-NO-DIFF > SKIPPED-NO-VERB
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

# Single-step elevations (OK ranks above SKIPPED-*; N/A and FAIL still beat OK).
assert_aggregate "OK"              "OK" "SKIPPED-NO-VERB"   # OK wins over SKIPPED-NO-VERB
assert_aggregate "OK"              "OK" "SKIPPED-NO-DIFF"   # OK wins over SKIPPED-NO-DIFF
assert_aggregate "N/A"             "OK" "N/A"               # N/A beats OK (deliberate documented gap)
assert_aggregate "FAIL"            "OK" "FAIL"              # FAIL beats OK

# Cross-precedence (code-reviewer locked).
assert_aggregate "SKIPPED-NO-DIFF" "SKIPPED-NO-VERB" "SKIPPED-NO-DIFF"   # NO-DIFF beats NO-VERB
assert_aggregate "N/A"             "SKIPPED-NO-DIFF" "N/A"               # N/A beats NO-DIFF
assert_aggregate "N/A"             "N/A" "SKIPPED-NO-VERB"               # N/A beats NO-VERB
assert_aggregate "FAIL"            "FAIL" "N/A"                          # FAIL beats N/A
assert_aggregate "FAIL"            "OK" "FAIL"                           # FAIL beats OK

# FAIL-MISSING-VERB rank 5 (task #52): outranks OK, N/A, and FAIL (a wiring fault must
# not be masked by a sibling lang's clean run or even a sibling's real FAIL); UNKNOWN
# (dispatcher bug) still outranks it. This is the rank that closes cross-lang-masking.
assert_aggregate "FAIL-MISSING-VERB" "OK" "FAIL-MISSING-VERB"                 # beats OK (the masking case)
assert_aggregate "FAIL-MISSING-VERB" "FAIL" "FAIL-MISSING-VERB"               # beats FAIL
assert_aggregate "FAIL-MISSING-VERB" "N/A" "FAIL-MISSING-VERB"                # beats N/A
assert_aggregate "FAIL-MISSING-VERB" "OK" "SKIPPED-NO-DIFF" "FAIL-MISSING-VERB" "FAIL"  # beats a mixed field
assert_aggregate "UNKNOWN"           "FAIL-MISSING-VERB" "UNKNOWN"            # UNKNOWN still on top

# SKIPPED-NO-CLUSTER (task #56): exit-0 clean skip (Layer 7, no devloop cluster in CI).
# Ranks BELOW OK so a sibling's real OK dominates and a green CI run reports TOTAL_RESULT=OK.
assert_aggregate "OK"                 "OK" "SKIPPED-NO-CLUSTER"               # OK dominates → CI TOTAL stays OK
assert_aggregate "SKIPPED-NO-CLUSTER" "SKIPPED-NO-CLUSTER"                    # lone skip (Layer 7 alone)
assert_aggregate "SKIPPED-NO-CLUSTER" "SKIPPED-NO-DIFF" "SKIPPED-NO-CLUSTER" # same skip tier, above NO-DIFF
assert_aggregate "N/A"                "N/A" "SKIPPED-NO-CLUSTER"              # N/A (different tier) still beats a skip

# PRECONDITION_FAILURE (task #56): exit-2 operator/infra lane (Layer 7 Phase-1 gate).
# Rank 6 (@test/@team-lead): outranks FAIL (an infra precondition dominates a sibling test
# failure) but BELOW FAIL-MISSING-VERB (a missing-wrapper pipeline-machinery defect is more
# fundamental than a transient cluster-down) and UNKNOWN.
assert_aggregate "PRECONDITION_FAILURE" "OK" "PRECONDITION_FAILURE"                  # beats OK
assert_aggregate "PRECONDITION_FAILURE" "FAIL" "PRECONDITION_FAILURE"                # beats a sibling test FAIL
assert_aggregate "PRECONDITION_FAILURE" "N/A" "PRECONDITION_FAILURE"                 # beats N/A
assert_aggregate "FAIL-MISSING-VERB"    "PRECONDITION_FAILURE" "FAIL-MISSING-VERB"   # wiring fault outranks it (@test)
assert_aggregate "UNKNOWN"              "PRECONDITION_FAILURE" "UNKNOWN"              # UNKNOWN still on top
assert_aggregate "PRECONDITION_FAILURE" "OK" "SKIPPED-NO-DIFF" "PRECONDITION_FAILURE" "FAIL"  # wins a realistic mixed field

# Multi-arg cases.
assert_aggregate "FAIL"            "OK" "OK" "FAIL" "OK"
assert_aggregate "N/A"             "OK" "SKIPPED-NO-DIFF" "N/A" "SKIPPED-NO-VERB"
assert_aggregate "OK"              "OK" "OK" "OK"

# Multi-lang success path: one OK lang among SKIPPED-* siblings aggregates to OK, so a
# clean edit reports "loud success" rather than a SKIPPED-* state. (SKIPPED-NO-VERB here
# is the all-langs-filtered enum — its only producer since task #52; a genuinely missing
# verb is FAIL-MISSING-VERB, which would NOT aggregate to OK. This locks OK > SKIPPED-*.)
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

# -----------------------------------------------------------------------------
# Task #52 — pure-f(enum) status_to_exit_code + representative-reason
# worst_reason_for_status + the parallel __LAYER_REASONS collection.
# -----------------------------------------------------------------------------

assert_exit_code() {
  local label="$1" expected="$2" status="$3"
  local actual; actual=$(status_to_exit_code "$status")
  if [[ "$actual" == "$expected" ]]; then
    PASS=$((PASS + 1))
  else
    FAIL=$((FAIL + 1))
    FAILURES+=("[exit-code:${label}] status_to_exit_code '${status}' → expected=${expected} actual=${actual}")
  fi
}

# Exit code is now a pure function of the enum (task #52) — no REASON consulted.
# SKIPPED-NO-VERB (only producer now: all-langs-filtered) → 0; FAIL-MISSING-VERB → 2
# (the wiring-fault class, joining UNKNOWN); everything else unchanged.
assert_exit_code "skv"               "0" "SKIPPED-NO-VERB"
assert_exit_code "ok"                "0" "OK"
assert_exit_code "no-diff"           "0" "SKIPPED-NO-DIFF"
assert_exit_code "na"                "0" "N/A"
assert_exit_code "fail"              "1" "FAIL"
assert_exit_code "fail-missing-verb" "2" "FAIL-MISSING-VERB"
assert_exit_code "unknown"           "2" "UNKNOWN"
# task #56 enums: SKIPPED-NO-CLUSTER joins the exit-0 skip set; PRECONDITION_FAILURE
# joins the exit-2 "investigate the environment" class.
assert_exit_code "skipped-no-cluster"   "0" "SKIPPED-NO-CLUSTER"
assert_exit_code "precondition-failure" "2" "PRECONDITION_FAILURE"

# worst_reason_for_status: returns a representative real reason among children whose
# enum == winner (for the stderr LAYER= cause), with a non-empty fallback. It no longer
# ranks by exit code (status_to_exit_code is pure f(enum) — nothing to tie-break).
__wr=$(worst_reason_for_status FAIL-MISSING-VERB \
  OK rust-audit-passed \
  FAIL-MISSING-VERB ts-audit-verb-missing-or-not-executable)
if [[ "$__wr" == "ts-audit-verb-missing-or-not-executable" ]]; then
  PASS=$((PASS + 1))
else
  FAIL=$((FAIL + 1)); FAILURES+=("[worst-reason] expected the FAIL-MISSING-VERB child's reason, got '${__wr}'")
fi
# Non-empty fallback when no child reason matches the winner.
__wr2=$(worst_reason_for_status FAIL-MISSING-VERB OK x-passed)
if [[ "$__wr2" == "fail-missing-verb-aggregate" ]]; then
  PASS=$((PASS + 1))
else
  FAIL=$((FAIL + 1)); FAILURES+=("[worst-reason-fallback] expected generic fallback, got '${__wr2}'")
fi

# tee_collect_statuses populates the PARALLEL __LAYER_REASONS index-aligned with
# __LAYER_STATUSES, without disturbing the bare-enum __LAYER_STATUSES.
__LAYER_STATUSES=()
__LAYER_REASONS=()
__r_in=$(mktemp); __r_out=$(mktemp)
printf 'STATUS=OK REASON=a-passed\nintermediate\nSTATUS=FAIL-MISSING-VERB REASON=rust-test-verb-missing-or-not-executable\n' > "$__r_in"
tee_collect_statuses < "$__r_in" > "$__r_out"
rm -f "$__r_in" "$__r_out"
if [[ "${#__LAYER_STATUSES[@]}" -eq 2 \
   && "${__LAYER_STATUSES[0]}" == "OK" \
   && "${__LAYER_STATUSES[1]}" == "FAIL-MISSING-VERB" \
   && "${#__LAYER_REASONS[@]}" -eq 2 \
   && "${__LAYER_REASONS[0]}" == "a-passed" \
   && "${__LAYER_REASONS[1]}" == "rust-test-verb-missing-or-not-executable" ]]; then
  PASS=$((PASS + 1))
else
  FAIL=$((FAIL + 1))
  FAILURES+=("[layer-reasons] __LAYER_REASONS not collected 1:1 with __LAYER_STATUSES; statuses=(${__LAYER_STATUSES[*]:-}) reasons=(${__LAYER_REASONS[*]:-})")
fi

# emit_step_duration format pin (task #56; observability-owned contract, @test harness).
# The per-step timing line is an operator grep anchor (`DURATION=` once → layer + per-step);
# lock its SHAPE so a future edit can't silently drop LAYER= or rename STEP=/DURATION=. The
# step name is GENERIC (`foo`) on purpose — the helper is layer-agnostic; coupling its unit
# test to layer7's real 5-step list would be a layering violation (per-step-name coverage,
# if wanted, lives in layer7.test.sh). Strict `^...$` forces a conscious test update on any
# field add/rename. (emit_step_duration writes only stderr, so 2>&1 captures it.)
__sd_ts=$(layer_now)
__LAYER_NUM=42   # double-digit deliberately exercises LAYER=[0-9]+ (observability's quantifier ask)
__sd_out=$(emit_step_duration foo "$__sd_ts" 2>&1)
__sd_re='^LAYER=42 STEP=foo DURATION=[0-9]+$'
if [[ "$__sd_out" =~ $__sd_re ]]; then
  PASS=$((PASS + 1))
else
  FAIL=$((FAIL + 1)); FAILURES+=("[step-duration-format] expected 'LAYER=42 STEP=foo DURATION=<secs>', got '${__sd_out}'")
fi
# Graceful-degradation fallback: called outside a lifecycle (__LAYER_NUM unset) → LAYER=?
unset __LAYER_NUM
__sd_out2=$(emit_step_duration foo "$__sd_ts" 2>&1)
__sd_re2='^LAYER=[?] STEP=foo DURATION=[0-9]+$'
if [[ "$__sd_out2" =~ $__sd_re2 ]]; then
  PASS=$((PASS + 1))
else
  FAIL=$((FAIL + 1)); FAILURES+=("[step-duration-fallback] expected 'LAYER=? STEP=foo DURATION=<secs>', got '${__sd_out2}'")
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
