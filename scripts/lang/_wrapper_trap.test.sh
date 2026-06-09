#!/usr/bin/env bash
# _wrapper_trap.test.sh — task #50 silent-skip-at-pipeline-edge coverage.
#
# Two blocks:
#   A. Unit — the wrapper EXIT trap (install_wrapper_exit_trap / __wrapper_early_exit_trap
#      + the _status_emitted guard) in isolation.
#   D. End-to-end — the LAYER EDGE: layer_lifecycle_begin → (synthetic dispatcher) |
#      tee_collect_statuses → __layer_lifecycle_end. This is where the silent-skip
#      actually escaped (the dispatcher is the LEFT of the pipe, so its rc is discarded;
#      __layer_lifecycle_end's EXIT trap owns the final exit). Proven here, not just at
#      the dispatcher boundary (test-reviewer Q1, REQUIRED).
#
# Hermeticity (test §A): everything runs in a tempdir; synthetic wrappers/layer
# scripts source the LIVE _common.sh. The layer-edge driver is invoked as a REAL
# SUBPROCESS (never sourced) — __layer_lifecycle_end ends in `exit "$rc"`, so sourcing
# it would kill this harness or false-green (test-reviewer item 1, CRITICAL).
set -euo pipefail
IFS=$'\n\t'

__here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

PASS=0
FAIL=0
FAILURES=()

# Run a snippet as a real subprocess that sources the live _common.sh; capture
# combined stdout+stderr and the exit code. NEVER source into this harness.
run_snippet() {
  local body="$1"
  local script; script=$(mktemp)
  {
    printf '#!/usr/bin/env bash\n'
    printf 'set -euo pipefail\n'
    printf 'IFS=$'"'"'\\n\\t'"'"'\n'
    printf 'source %q\n' "${__here}/_common.sh"
    printf '%s\n' "$body"
  } > "$script"
  local out rc=0
  out=$(bash "$script" 2>&1) || rc=$?
  rm -f "$script"
  # Emit captured output then the rc marker on its own trailing line.
  printf '%s\n__rc=%s\n' "$out" "$rc"
}

rc_of()   { grep -oE '__rc=[0-9]+' <<<"$1" | tail -n1 | cut -d= -f2; }
body_of() { sed '/^__rc=[0-9]*$/d' <<<"$1"; }

assert_eq() {
  local label="$1" expected="$2" actual="$3"
  if [[ "$actual" == "$expected" ]]; then
    PASS=$((PASS + 1))
  else
    FAIL=$((FAIL + 1))
    FAILURES+=("[${label}] expected '${expected}', got '${actual}'")
  fi
}

assert_contains() {
  local label="$1" needle="$2" hay="$3"
  if grep -qF "$needle" <<<"$hay"; then
    PASS=$((PASS + 1))
  else
    FAIL=$((FAIL + 1))
    FAILURES+=("[${label}] expected to contain '${needle}', got: ${hay}")
  fi
}

# =============================================================================
# Block A — wrapper EXIT trap unit
# =============================================================================

# (a) explicit exit 1 before any emit → trap emits FAIL with the captured rc, exit 1.
test_trap_fires_on_explicit_early_exit() {
  local r; r=$(run_snippet '
install_wrapper_exit_trap
exit 1
emit_status OK never-reached
')
  local rc; rc=$(rc_of "$r")
  local out; out=$(body_of "$r")
  assert_eq       "trap-explicit-exit:rc"     "1" "$rc"
  assert_contains "trap-explicit-exit:status" "STATUS=FAIL REASON=wrapper-aborted-early-exit-1" "$out"
}

# (a/Q2) set -e abort (a FAILING command, not explicit exit) before run_and_emit →
# same FAIL-via-trap. This is the rust/test.sh DB-bringup-exit shape.
test_trap_fires_on_set_e_abort() {
  local r; r=$(run_snippet '
install_wrapper_exit_trap
false           # set -e abort before any emit
run_and_emit "x" true
')
  local rc; rc=$(rc_of "$r")
  local out; out=$(body_of "$r")
  assert_eq       "trap-set-e:rc"     "1" "$rc"
  assert_contains "trap-set-e:status" "STATUS=FAIL REASON=wrapper-aborted-early-exit-1" "$out"
}

# (neg) normal OK path → trap silent: EXACTLY ONE STATUS line, exit 0.
test_trap_silent_on_normal_ok() {
  local r; r=$(run_snippet '
install_wrapper_exit_trap
run_and_emit "x" true
')
  local rc; rc=$(rc_of "$r")
  local out; out=$(body_of "$r")
  local n; n=$(grep -c '^STATUS=' <<<"$out" || true)
  assert_eq       "trap-silent-ok:rc"      "0" "$rc"
  assert_eq       "trap-silent-ok:count"   "1" "$n"
  assert_contains "trap-silent-ok:status"  "STATUS=OK REASON=x-passed" "$out"
}

# (neg) explicit emit_status FAIL; exit 1 → trap silent (the _status_emitted guard
# suppresses a double-emit): EXACTLY ONE STATUS line, exit 1.
test_trap_silent_on_explicit_fail() {
  local r; r=$(run_snippet '
install_wrapper_exit_trap
emit_status FAIL my-fail
exit 1
')
  local rc; rc=$(rc_of "$r")
  local out; out=$(body_of "$r")
  local n; n=$(grep -c '^STATUS=' <<<"$out" || true)
  assert_eq       "trap-silent-fail:rc"     "1" "$rc"
  assert_eq       "trap-silent-fail:count"  "1" "$n"
  assert_contains "trap-silent-fail:status" "STATUS=FAIL REASON=my-fail" "$out"
}

# =============================================================================
# Block D — LAYER EDGE end-to-end (test-reviewer Q1)
#
# A synthetic layer script sources _common.sh, calls layer_lifecycle_begin, pipes a
# synthetic "dispatcher" (a here-doc producing STATUS lines) through
# tee_collect_statuses, and lets the EXIT trap (__layer_lifecycle_end) fire. Invoked
# as a real subprocess.
# =============================================================================

# Build + run a synthetic layer whose "dispatcher" emits the given STATUS lines.
# $1 = label, $2 = newline-separated STATUS lines (the child stream).
run_synth_layer() {
  local lines="$1"
  local tmp; tmp=$(mktemp -d)
  local stream="${tmp}/stream.txt"
  printf '%s\n' "$lines" > "$stream"
  local layer="${tmp}/synth_layer.sh"
  {
    printf '#!/usr/bin/env bash\n'
    printf 'set -uo pipefail\n'
    printf 'IFS=$'"'"'\\n\\t'"'"'\n'
    printf 'source %q\n' "${__here}/_common.sh"
    printf 'layer_lifecycle_begin 99\n'
    # cat the canned child stream through the real tee_collect_statuses, at top-level
    # command position so lastpipe applies (parent-shell __LAYER_* mutation).
    printf 'cat %q | tee_collect_statuses\n' "$stream"
    # layer_lifecycle_begin installed the EXIT trap → __layer_lifecycle_end fires here.
  } > "$layer"
  local out rc=0
  out=$(bash "$layer" 2>&1) || rc=$?
  rm -rf "$tmp"
  printf '%s\n__rc=%s\n' "$out" "$rc"
}

# (b-e2e) unexpected verb-missing as the winning child → LAYER exits == 2.
test_layer_unexpected_verb_missing_exits_2() {
  local r; r=$(run_synth_layer 'STATUS=SKIPPED-NO-VERB REASON=rust-test-UNEXPECTED-verb-missing-or-not-executable')
  local rc; rc=$(rc_of "$r")
  local out; out=$(body_of "$r")
  assert_eq       "layer-unexpected:rc"     "2" "$rc"
  assert_contains "layer-unexpected:reason" "RESULT=SKIPPED-NO-VERB REASON=rust-test-UNEXPECTED-verb-missing-or-not-executable" "$out"
}

# (b-neg-e2e) intentional-gap reason as winning child → LAYER exits == 0, aggregate intact.
test_layer_intentional_gap_exits_0() {
  local r; r=$(run_synth_layer 'STATUS=SKIPPED-NO-VERB REASON=proto-test-sh-missing-or-not-executable')
  local rc; rc=$(rc_of "$r")
  assert_eq "layer-intentional:rc" "0" "$(rc_of "$r")"
}

# (b-tiebreak-e2e) intentional-gap SKIPPED-NO-VERB co-running with an UNEXPECTED
# verb-missing (same enum, different reasons) → worst-exit-driving reason wins → == 2.
test_layer_tiebreak_unexpected_wins() {
  local r; r=$(run_synth_layer 'STATUS=SKIPPED-NO-VERB REASON=proto-test-sh-missing-or-not-executable
STATUS=SKIPPED-NO-VERB REASON=rust-test-UNEXPECTED-verb-missing-or-not-executable')
  local rc; rc=$(rc_of "$r")
  local out; out=$(body_of "$r")
  assert_eq       "layer-tiebreak:rc"     "2" "$rc"
  assert_contains "layer-tiebreak:reason" "REASON=rust-test-UNEXPECTED-verb-missing-or-not-executable" "$out"
}

# (e-e2e) a child emitting NO STATUS line → __LAYER_STATUSES gets UNKNOWN → exit == 2
# AND stderr RESULT=UNKNOWN. Assert BOTH halves (test-reviewer item 2): code proves the
# gate fails, token proves it failed for the RIGHT cause (not an unrelated FAIL).
test_layer_no_status_child_is_unknown_exit_2() {
  # The synthetic dispatcher emits ONLY non-STATUS chatter, then the single-lang
  # parse_status_line yields empty → dispatcher would substitute UNKNOWN. Simulate the
  # child stream the layer sees when a wrapper crashed AND its trap didn't fire: a
  # line that is not a STATUS line, plus an explicit UNKNOWN the dispatcher injects.
  local r; r=$(run_synth_layer 'some wrapper chatter with no status
STATUS=UNKNOWN REASON=rust-test-no-status-emitted')
  local rc; rc=$(rc_of "$r")
  local out; out=$(body_of "$r")
  assert_eq       "layer-unknown:rc"      "2" "$rc"
  assert_contains "layer-unknown:result"  "RESULT=UNKNOWN" "$out"
}

# (a-e2e) a crashing wrapper that the trap converted to STATUS=FAIL, routed through the
# layer → LAYER exits == 1 (FAIL aggregate), distinct from the no-status UNKNOWN==2 case.
test_layer_trapped_crash_is_fail_exit_1() {
  local r; r=$(run_synth_layer 'STATUS=FAIL REASON=wrapper-aborted-early-exit-1')
  local rc; rc=$(rc_of "$r")
  local out; out=$(body_of "$r")
  assert_eq       "layer-trapped-crash:rc"     "1" "$rc"
  assert_contains "layer-trapped-crash:result" "RESULT=FAIL" "$out"
}

# (no-regression) a clean OK child → LAYER exits == 0.
test_layer_ok_exits_0() {
  local r; r=$(run_synth_layer 'STATUS=OK REASON=rust-test-passed')
  assert_eq "layer-ok:rc" "0" "$(rc_of "$r")"
}

# SHARED-CONTRACT (test-reviewer item 4): the dispatcher DERIVES the unexpected reason
# token and the layer/`__is_intentional_gap_reason` MATCHES it — two independent touch
# points on the same literal. This test pins both sides to the single
# DEVLOOP_UNEXPECTED_VERB_SUFFIX constant end-to-end: it sources _common.sh, builds the
# token exactly as _dispatch.sh does (`<lang>-<verb>${DEVLOOP_UNEXPECTED_VERB_SUFFIX}`),
# feeds THAT string to the matcher + the exit classifier, and asserts bug-classification
# (exit 2). If a future rename desyncs producer and matcher, this fails loud.
test_unexpected_suffix_single_source_of_truth() {
  local r; r=$(run_snippet '
# Producer side (mirrors _dispatch.sh derivation) using the shared constant:
reason="rust-test${DEVLOOP_UNEXPECTED_VERB_SUFFIX}"
echo "DERIVED=$reason"
# Matcher side (layer edge) must classify it as NOT-intentional (the bug case):
if __is_intentional_gap_reason "$reason"; then echo "MATCH=intentional-WRONG"; else echo "MATCH=unexpected-ok"; fi
# Exit classifier must map it to 2:
echo "CODE=$(status_to_exit_code SKIPPED-NO-VERB "$reason")"
# And the constant must carry the literal UNEXPECTED segment (cosmetics + grep contract):
case "$DEVLOOP_UNEXPECTED_VERB_SUFFIX" in *UNEXPECTED*) echo "LITERAL=ok" ;; *) echo "LITERAL-missing" ;; esac
')
  local out; out=$(body_of "$r")
  assert_contains "sssot:derived"  "DERIVED=rust-test-UNEXPECTED-verb-missing-or-not-executable" "$out"
  assert_contains "sssot:match"    "MATCH=unexpected-ok" "$out"
  assert_contains "sssot:code"     "CODE=2" "$out"
  assert_contains "sssot:literal"  "LITERAL=ok" "$out"
}

# =============================================================================
# Run all
# =============================================================================

test_trap_fires_on_explicit_early_exit
test_trap_fires_on_set_e_abort
test_trap_silent_on_normal_ok
test_trap_silent_on_explicit_fail
test_layer_unexpected_verb_missing_exits_2
test_layer_intentional_gap_exits_0
test_layer_tiebreak_unexpected_wins
test_layer_no_status_child_is_unknown_exit_2
test_layer_trapped_crash_is_fail_exit_1
test_layer_ok_exits_0
test_unexpected_suffix_single_source_of_truth

printf '\n_wrapper_trap.test.sh: %d passed, %d failed\n' "$PASS" "$FAIL"
if [[ $FAIL -gt 0 ]]; then
  printf 'Failures:\n'
  for f in "${FAILURES[@]}"; do
    printf '  - %s\n' "$f"
  done
  exit 1
fi
exit 0
