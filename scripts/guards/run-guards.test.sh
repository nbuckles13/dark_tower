#!/usr/bin/env bash
# coverage-exempt: exercises run-guards.sh exit-code classification in-process, does not drive the dt-guard binary via $DT_GUARD
# run-guards.test.sh — self-test for scripts/guards/run-guards.sh's exit-code /
# STATUS-emission contract (fast-fail-guards devloop). There is no *.test.sh
# auto-runner, so this is wired into scripts/layer3.sh to run every devloop + CI.
#
# WHAT CHANGED (Change 1): a guard TIMEOUT (exit 124) or KILL (exit 137) moved
# from the implementer lane (STATUS=FAIL) to the OPERATOR lane
# (STATUS=PRECONDITION_FAILURE) — a timeout is a machine fact, not a diff defect.
# run-guards.sh's OWN exit code was made honest to match, via a separate
# PRECONDITION_GUARDS counter with a LADDER-MIRROR precedence:
#   PRECONDITION_GUARDS>0 -> exit 2 (DOMINANT) ; elif FAILED_GUARDS>0 -> exit 1 ; else 0.
# (Ladder-mirror mirrors aggregate_worst_status: PRECONDITION_FAILURE outranks
#  FAIL. It is safe to dominate a real FAIL because the violation stays
#  independently legible — see the MIXED case below.)
#
# HERMETIC — NO REAL TIMEOUT: a PATH-stubbed `timeout` shadows /usr/bin/timeout
# and returns the injected exit code WITHOUT running the guard, so
# classify_guard_exit's arms are driven deterministically and instantly. A
# sleepy-guard approach would be wall-clock-bound and flaky; this is neither.
# run-guards discovers the real scripts/guards/simple/*.sh set, but the stub
# never executes any of them, so assertions are substring/count-agnostic and
# survive guards being added or removed.
#
# SSoT EXIT CODES: expected exit codes are DERIVED from status_to_exit_code (the
# single-source ladder), never hardcoded 2/1 — a future ladder reorder must red
# this test, not slip through.
set -euo pipefail
IFS=$'\n\t'

__here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${__here}/../.." && pwd)"
# shellcheck source=../lang/_test_helpers.sh
source "${REPO_ROOT}/scripts/lang/_test_helpers.sh"
# shellcheck source=../lang/_common.sh
source "${REPO_ROOT}/scripts/lang/_common.sh"   # status_to_exit_code — SSoT exit ladder

# run-guards exits non-zero on the timeout/violation lanes we drive; set -e must
# NOT abort the harness on a captured non-zero. report_results sets the final code.
set +e

RUN_GUARDS="${REPO_ROOT}/scripts/guards/run-guards.sh"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# Expected exit codes DERIVED from the ladder (never literal 2/1).
EXP_PRECOND=$(status_to_exit_code PRECONDITION_FAILURE)   # operator lane (2)
EXP_FAIL=$(status_to_exit_code FAIL)                      # implementer lane (1)

# --- timeout stubs ------------------------------------------------------------
# run-guards.sh invokes `timeout --kill-after=Ns Ms <guard> <path>`. A PATH-stub
# named `timeout`, first on PATH, shadows the real one and returns a chosen code
# without running the guard — exercising classify_guard_exit's arm for that code.
#
# mk_fixed_timeout <dir> <rc>: every guard gets <rc>.
mk_fixed_timeout() {
  mkdir -p "$1"
  cat > "$1/timeout" <<EOF
#!/usr/bin/env bash
exit $2
EOF
  chmod +x "$1/timeout"
}
# mk_split_timeout <dir>: 124 on the FIRST guard invocation, 1 on the rest — one
# precondition + N-1 violations, deterministically via a counter file ($TO_COUNTER).
# This is the ONLY way to drive the mixed lane through a single global `timeout`
# stub; it needs no sleepy guard and no per-guard-name coupling.
mk_split_timeout() {
  mkdir -p "$1"
  cat > "$1/timeout" <<'EOF'
#!/usr/bin/env bash
n=$(cat "$TO_COUNTER" 2>/dev/null || echo 0); n=$((n + 1)); printf '%s' "$n" > "$TO_COUNTER"
[ "$n" -eq 1 ] && exit 124 || exit 1
EOF
  chmod +x "$1/timeout"
}

# run_rg <stubdir> [KEY=VAL ...] — run run-guards.sh with <stubdir> first on PATH
# (so its `timeout` wins) against an empty scan dir (guards never actually run).
# Extra KEY=VAL env is passed through. Sets RG_OUT (stdout), RG_ERR (stderr), RG_RC.
run_rg() {
  local stubdir="$1"; shift
  local t; t="$(mktemp -d "$WORK/run.XXXXXX")"; mkdir -p "$t/scan"
  RG_OUT="$t/out"; RG_ERR="$t/err"
  env PATH="${stubdir}:$PATH" "$@" bash "$RUN_GUARDS" "$t/scan" >"$RG_OUT" 2>"$RG_ERR"
  RG_RC=$?
}

# =============================================================================
# (1) PURE TIMEOUT (exit 124) -> OPERATOR lane. Every guard times out: run-guards
#     emits PRECONDITION_FAILURE per guard and, with FAILED_GUARDS=0, exits 2.
# =============================================================================
d="$WORK/stub124"; mk_fixed_timeout "$d" 124
run_rg "$d"
out="$(cat "$RG_OUT")"; err="$(cat "$RG_ERR")"
assert_exit   "t124-exit-precondition"  "$EXP_PRECOND" "$RG_RC"
# Operator-facing banner (stderr) — the UX heart of Change 1: tell an operator this is a
# machine fact ("operator lane"), NOT a diff defect to hunt (the 2026-08-14 mis-triage).
# Stable substring, not full prose.
assert_status "t124-operator-lane-banner" "operator lane; suspect concurrent machine load" "$err"
assert_status "t124-emits-precondition" "STATUS=PRECONDITION_FAILURE REASON=guard-timeout-" "$out"
# REGRESSION PIN: the OLD behavior emitted STATUS=FAIL for a timeout — the exact
# 2026-08-14 confusion this change kills. It must be gone.
assert_absent "t124-no-old-fail-timeout" "STATUS=FAIL REASON=guard-timeout-" "$out"
# COUNTER-SEPARATION (the ABSENCE is the assertion): a pure-timeout run found NO
# real violation, so neither the guard-violations trace NOR the MIXED lane may
# appear. This proves PRECONDITION_GUARDS and FAILED_GUARDS are genuinely
# separate counters — not both bumped on every failure, which would make
# guard-violations fire unconditionally and stop meaning "a real violation".
assert_absent "t124-no-violations-trace" "STATUS=FAIL REASON=guard-violations" "$out"
assert_absent "t124-no-mixed-lane"       "MIXED_LANE:" "$out"

# =============================================================================
# (2) PURE KILL (exit 137) -> OPERATOR lane, kill token. SIGKILL-after-grace (or
#     an OOM kill) is likewise a machine fact, so it shares the 124 lane.
# =============================================================================
d="$WORK/stub137"; mk_fixed_timeout "$d" 137
run_rg "$d"
out="$(cat "$RG_OUT")"; err="$(cat "$RG_ERR")"
assert_exit   "t137-exit-precondition" "$EXP_PRECOND" "$RG_RC"
assert_status "t137-emits-kill-token"  "STATUS=PRECONDITION_FAILURE REASON=guard-timeout-kill-" "$out"
assert_absent "t137-no-old-fail-kill"  "STATUS=FAIL REASON=guard-timeout-kill-" "$out"
# Kill-specific operator banner (stderr): distinct SIGKILL wording + the same operator-lane hint.
assert_status "t137-kill-banner"       "SIGKILLed after" "$err"
assert_status "t137-operator-lane"     "operator lane" "$err"

# =============================================================================
# (3) PURE VIOLATION (exit 1) -> IMPLEMENTER lane. A real guard failure: run-guards
#     emits its machine-readable violations trace and exits 1. This is the
#     DISTINCT-FROM-TIMEOUT proof — exit 1 here vs exit 2 in (1)/(2) on one ladder.
# =============================================================================
d="$WORK/stub1"; mk_fixed_timeout "$d" 1
run_rg "$d"
out="$(cat "$RG_OUT")"
assert_exit   "v1-exit-fail"        "$EXP_FAIL" "$RG_RC"
assert_status "v1-emits-violations" "STATUS=FAIL REASON=guard-violations" "$out"
# COUNTER-SEPARATION twin of (1): a pure-violation run hit NO timeout, so no
# operator-lane STATUS and no MIXED lane. Absence is the assertion.
assert_absent "v1-no-precondition"  "STATUS=PRECONDITION_FAILURE" "$out"
assert_absent "v1-no-mixed-lane"    "MIXED_LANE:" "$out"

# =============================================================================
# (4) ALL PASS (exit 0) -> baseline. A suite where every case expects non-zero
#     proves nothing; this proves the harness CAN go green.
# =============================================================================
d="$WORK/stub0"; mk_fixed_timeout "$d" 0
run_rg "$d"
assert_exit "ok0-exit-zero" 0 "$RG_RC"

# =============================================================================
# (5) MIXED (one timeout + N-1 violations) — the case that carries the SECURITY
#     property. Under ladder-mirror the standalone exit is 2 (operator lane) EVEN
#     THOUGH real violations exist. That is honest ONLY because the violations
#     stay independently legible: run-guards emits BOTH the guard-violations
#     STATUS trace AND the MIXED_LANE: line ("the violations above are REAL").
#     Trimming either assertion silently reverts to the "lossy scalar" config the
#     Lead's ladder-mirror ruling rejected — the exit code alone no longer
#     expresses the violation, so THIS TEST is its enforcement point. Do not
#     delete these two asserts as redundant with the exit check; they are not.
#     Exit is DERIVED from status_to_exit_code PRECONDITION_FAILURE (2 dominates 1).
# =============================================================================
d="$WORK/stubmix"; mk_split_timeout "$d"
counter="$WORK/mix.counter"; : > "$counter"
run_rg "$d" TO_COUNTER="$counter"
out="$(cat "$RG_OUT")"; err="$(cat "$RG_ERR")"
assert_exit   "mix-exit-precondition-dominant" "$EXP_PRECOND" "$RG_RC"
assert_status "mix-precondition-present" "STATUS=PRECONDITION_FAILURE REASON=guard-timeout-" "$out"
# guard-violations stays STDOUT (a STATUS= line tee_collect_statuses must vote on); asserting
# it on stdout also pins that it can't be moved to stderr-only, which would drop the layer FAIL vote.
assert_status "mix-violations-legible"   "STATUS=FAIL REASON=guard-violations" "$out"
# MIXED_LANE is emitted to BOTH stdout (summary block) AND stderr (@observability: the §4
# one-pass grep operator reads layer-3.stderr.log alongside the timeout banner). Pin BOTH — a
# future edit dropping either emission would silence one operator surface with nothing else red.
assert_status "mix-lane-line-stdout"     "MIXED_LANE:" "$out"
assert_status "mix-lane-line-stderr"     "MIXED_LANE:" "$err"

# =============================================================================
# (6) The exit-0 arm surfaces `^WARN ` from a PASSING guard — the OPS-8 fix.
#
# Before that arm existed, `classify_guard_exit` discarded $captured on exit 0,
# so a guard that PASSED while reporting a coverage hole was silent on every
# run. Two contracts depended on it and neither was enforceable: §6.3's
# `ts-no-retained-credentials` rule that "a clean run must be a WARN-free run
# ... do not ignore it because the layer passed", and validate-frame-vectors'
# g14 banner, which is the only runtime signal that the cross-language property
# is not yet established.
#
# THIS CASE PINS THE RUNNER LEG ONLY. The stub below is synthetic, so it cannot
# show that any real guard emits the prefix; `validate-frame-vectors.test.sh`
# pins that against the real guard. Composed, the two cover the channel end to
# end with each half beside the code it constrains. Deleting either leaves a
# silent gap.
#
# Both assertions route through the harness (assert_status / assert_absent) so a
# regression lands in FAILURES[] and reddens report_results. An earlier draft of
# this case used `echo FAIL: ...; return 1` and sat AFTER the report_results
# call — so it never ran, and could not have failed the suite if it had, because
# this file runs under `set +e`. That is the same empty-result-reads-as-pass bug
# the arm itself exists to fix, reproduced in its own test.
# =============================================================================
# run-guards.sh derives its guards directory from its OWN location, not from the
# path argument (which is the scan path handed to each guard). So the synthetic
# tree has to contain a copy of the runner, not merely a copy of the guards.
d="$WORK/stubwarn"
mkdir -p "$d/scripts/guards/simple" "$d/.git"
cp "$RUN_GUARDS" "$d/scripts/guards/run-guards.sh"
cp "$(dirname "$RUN_GUARDS")/common.sh" "$d/scripts/guards/common.sh"
cat > "$d/scripts/guards/simple/stub-warn-guard.sh" <<'STUB'
#!/usr/bin/env bash
echo "WARN stub-guard: a coverage hole reported from a PASSING guard"
echo "incidental mention of WARN mid-sentence that must NOT be surfaced"
exit 0
STUB
chmod +x "$d/scripts/guards/simple/stub-warn-guard.sh"

warn_rc=0
warn_out="$(bash "$d/scripts/guards/run-guards.sh" "$d" 2>&1)" || warn_rc=$?
# Exit code rather than the "PASSED: <name>" line: run-guards colourises that
# label, so ANSI escapes sit between "PASSED" and the colon and a substring
# assert on the rendered text would be pinning terminal formatting.
assert_exit   "warn-arm-guard-actually-passed" 0 "$warn_rc"
assert_status "warn-arm-surfaces-prefixed-line" \
  "WARN stub-guard: a coverage hole reported from a PASSING guard" "$warn_out"
assert_absent "warn-arm-anchor-is-line-start" \
  "incidental mention of WARN mid-sentence" "$warn_out"

report_results "scripts/guards/run-guards.test.sh"
