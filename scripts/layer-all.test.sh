#!/usr/bin/env bash
# layer-all.test.sh — orchestrator lane-integrity self-test for scripts/layer-all.sh
# (task #56). Proves the ORCHESTRATOR's exit-code / summary / budget behavior end-to-end —
# the seams a single layer's own self-test (layer7.test.sh) structurally CANNOT cover,
# because it never runs layer-all.sh.
#
# Why this exists: layer-all's loop used to collapse every non-zero layer exit to
# final_exit=1, silently masking the operator lane (exit 2). This test pins the repaired
# `final_exit = max(status_to_exit_code(total_result), worst-observed-rc)` so the
# PRECONDITION_FAILURE (exit 2) and FAIL-MISSING-VERB (exit 2) lanes survive to
# LAYER_ALL_EXIT, the SKIPPED-NO-CLUSTER lane stays green, a lying STATUS line can't demote
# the process exit (the rc FLOOR), Layer 7 is excluded from the per-layer budget warn, and
# the LAYER_SCRIPT_DIR CI-bypass control actually fires.
#
# Hermetic: runs layer-all.sh against PURE-PRINTF STUB layer scripts via the
# DEVLOOP_TEST-gated LAYER_SCRIPT_DIR seam (layer-all.sh:98-101). Stubs only feed a
# STATUS= line on stdout + a chosen exit code (+ a per-layer "ran" marker) — they do NOT
# source _common.sh, so this isolates layer-all's parse→aggregate→final_exit→budget→summary
# behavior from the lifecycle (which _common.test.sh owns). No real layers 1-6, no cluster.
#
# Wired into scripts/layer3.sh (no *.test.sh auto-runner) so it runs every devloop + CI.
set -euo pipefail
IFS=$'\n\t'

__here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lang/_test_helpers.sh
source "${__here}/lang/_test_helpers.sh"

# layer-all exits non-zero on the failure lanes we drive; set -e must NOT abort the harness
# on a captured non-zero. report_results provides the final pass/fail exit code.
set +e

LAYER_ALL="${__here}/layer-all.sh"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# Precondition: layer-all's LOCAL mode (the mode every seam case below runs in — the seam
# is local-only, and GITHUB_ACTIONS is rejected when LAYER_SCRIPT_DIR is set) checks
# `git merge-base origin/main HEAD` before the layer loop. That's the devloop guarantee
# (this test runs inside the devloop clone of main). If it's unreachable, fail LOUD with an
# actionable message rather than letting every local-mode case mis-classify as
# PRECONDITION_FAILURE — a silent-ish skew is exactly what this task exists to kill.
if ! git merge-base origin/main HEAD >/dev/null 2>&1; then
  printf 'layer-all.test.sh: PRECONDITION — `git merge-base origin/main HEAD` unreachable.\n' >&2
  printf '  This test must run inside the devloop repo (origin/main present). Not a code failure.\n' >&2
  exit 2
fi

# --- Stub-layer helpers -------------------------------------------------------
# mk_stub <dir> <n> <status> <reason> <exit> — write a pure-printf stub at <dir>/layer<n>.sh.
# Emits one STATUS line, drops a runtime "ran.layer<n>" marker into $DEVLOOP_TMP (so a case
# can prove whether the stub EXECUTED), and exits with <exit>. ${DEVLOOP_TMP} stays literal
# (resolved at the stub's runtime); status/reason/n/exit are baked in now.
mk_stub() {
  cat > "$1/layer$2.sh" <<EOF
#!/usr/bin/env bash
printf 'STATUS=$3 REASON=$4\n'
: > "\${DEVLOOP_TMP}/ran.layer$2"
exit $5
EOF
  chmod +x "$1/layer$2.sh"
}

# mk_all_ok <dir> — fill layer1.sh..layer7.sh with OK/exit-0 stubs.
mk_all_ok() { local n; for n in 1 2 3 4 5 6 7; do mk_stub "$1" "$n" OK "layer$n-ok" 0; done; }

# new_stubdir — mint a fresh all-OK stub dir; echo its path.
new_stubdir() { local d; d="$(mktemp -d "$WORK/stub.XXXXXX")"; mk_all_ok "$d"; printf '%s\n' "$d"; }

# --- Runner -------------------------------------------------------------------
# run_la <stubdir> [KEY=VAL ...] — run layer-all hermetically (env -i + PATH/HOME only)
# with a fresh DEVLOOP_TMP, capturing stdout/stderr/exit. Sets LA_OUT, LA_ERR, LA_DT, LA_RC.
# Extra KEY=VAL env (DEVLOOP_TEST / LAYER_SCRIPT_DIR / GITHUB_ACTIONS / DEVLOOP_LAYER_BUDGET_SECS)
# is passed through; GITHUB_ACTIONS is ABSENT unless a case sets it (env -i scrubs it).
run_la() {
  local stubdir="$1"; shift
  local t; t="$(mktemp -d "$WORK/run.XXXXXX")"
  LA_DT="$t/dt"; LA_OUT="$t/out"; LA_ERR="$t/err"
  env -i PATH="$PATH" HOME="$HOME" DEVLOOP_TMP="$LA_DT" "$@" \
      bash "$LAYER_ALL" >"$LA_OUT" 2>"$LA_ERR"
  LA_RC=$?
}

# --- Local assertion helpers (build on _test_helpers.sh PASS/FAIL/FAILURES) ---
assert_absent() { # $1=label $2=needle $3=haystack — PASS iff needle NOT present
  if [[ "$3" != *"$2"* ]]; then PASS=$((PASS+1)); else
    FAIL=$((FAIL+1)); FAILURES+=("[$1] unexpected substring '$2' WAS present"); fi
}
assert_no_marker() { # $1=label — PASS iff no stub-ran marker exists in LA_DT
  if ! ls "${LA_DT}"/ran.layer* >/dev/null 2>&1; then PASS=$((PASS+1)); else
    FAIL=$((FAIL+1)); FAILURES+=("[$1] a stub layer RAN (ran.layer* present) but should not have"); fi
}
assert_marker() { # $1=label — PASS iff at least one stub-ran marker exists in LA_DT
  if ls "${LA_DT}"/ran.layer* >/dev/null 2>&1; then PASS=$((PASS+1)); else
    FAIL=$((FAIL+1)); FAILURES+=("[$1] no stub layer ran (ran.layer* absent) but the seam should have run them"); fi
}

# =============================================================================
# (a) PRECONDITION_FAILURE at Layer 7 → LAYER_ALL_EXIT=2 + summary RESULT cell.
#     The operator lane survives the orchestrator (the repaired collapse).
# =============================================================================
d="$(new_stubdir)"; mk_stub "$d" 7 PRECONDITION_FAILURE cluster-setup-failed 2
run_la "$d" DEVLOOP_TEST=1 LAYER_SCRIPT_DIR="$d"
out="$(cat "$LA_OUT")"
assert_exit   "a-precondition-exit2"   2 "$LA_RC"
assert_status "a-precondition-cell"    "LAYER=7 RESULT=PRECONDITION_FAILURE" "$out"
assert_status "a-precondition-total"   "TOTAL_RESULT=PRECONDITION_FAILURE"   "$out"

# =============================================================================
# (b) FAIL-MISSING-VERB (exit 2) at Layer 7 → LAYER_ALL_EXIT=2. Regression pin for the
#     pre-existing FMV-exit-2→reported-exit-1 collapse this fix also repairs.
# =============================================================================
d="$(new_stubdir)"; mk_stub "$d" 7 FAIL-MISSING-VERB rust-test-verb-missing 2
run_la "$d" DEVLOOP_TEST=1 LAYER_SCRIPT_DIR="$d"
assert_exit "b-fmv-exit2-not-collapsed-to-1" 2 "$LA_RC"

# =============================================================================
# (c) SKIPPED-NO-CLUSTER (exit 0, below OK) at Layer 7 → LAYER_ALL_EXIT=0, TOTAL stays OK.
#     The CI/no-cluster green lane (Lead-arbitrated SKIPPED-NO-CLUSTER enum).
# =============================================================================
d="$(new_stubdir)"; mk_stub "$d" 7 SKIPPED-NO-CLUSTER no-cluster-ci 0
run_la "$d" DEVLOOP_TEST=1 LAYER_SCRIPT_DIR="$d"
out="$(cat "$LA_OUT")"
assert_exit   "c-skipped-no-cluster-exit0" 0 "$LA_RC"
assert_status "c-skipped-no-cluster-cell"  "LAYER=7 RESULT=SKIPPED-NO-CLUSTER" "$out"
assert_status "c-total-stays-ok"           "TOTAL_RESULT=OK"                   "$out"

# =============================================================================
# (d) NON-DEMOTION FLOOR: a layer whose STATUS line says OK but whose process exits 1 must
#     NOT let LAYER_ALL_EXIT demote to 0. The rc FLOOR is the belt to the enum's suspenders.
# =============================================================================
d="$(new_stubdir)"; mk_stub "$d" 7 OK lying-status-line 1
run_la "$d" DEVLOOP_TEST=1 LAYER_SCRIPT_DIR="$d"
# enum maps OK→0, but worst-observed-rc is 1 → max(0,1)=1. Must be non-zero, never 0.
if [[ "$LA_RC" -ne 0 ]]; then PASS=$((PASS+1)); else
  FAIL=$((FAIL+1)); FAILURES+=("[d-non-demotion-floor] STATUS=OK+exit1 demoted LAYER_ALL_EXIT to 0"); fi

# =============================================================================
# (e) BUDGET-BREACH EXCLUDES Layer 7. With the per-layer warn budget forced to -1, every
#     0-second stub trips the warn — EXCEPT Layer 7 (the `$n -ne 7` guard). Pin both
#     directions: the warn still fires for 1-6 (didn't break it for everyone), and is
#     suppressed for 7. No 900s sleep needed.
# =============================================================================
d="$(new_stubdir)"
run_la "$d" DEVLOOP_TEST=1 LAYER_SCRIPT_DIR="$d" DEVLOOP_LAYER_BUDGET_SECS=-1
err="$(cat "$LA_ERR")"
assert_status "e-budget-warn-fires-for-1" "BUDGET_BREACH LAYER=1" "$err"
assert_absent "e-budget-warn-excludes-7"  "BUDGET_BREACH LAYER=7" "$err"

# =============================================================================
# (f) BASELINE / no-false-positive: all-OK → LAYER_ALL_EXIT=0 + TOTAL_RESULT=OK. Proves the
#     harness CAN go green (a fixture where every case expects non-zero proves nothing).
# =============================================================================
d="$(new_stubdir)"
run_la "$d" DEVLOOP_TEST=1 LAYER_SCRIPT_DIR="$d"
out="$(cat "$LA_OUT")"
assert_exit   "f-all-ok-exit0"  0 "$LA_RC"
assert_status "f-all-ok-total"  "TOTAL_RESULT=OK" "$out"
assert_marker "f-all-ok-stubs-ran"

# =============================================================================
# (g) WORST-WINS across layers: a FAIL (exit 1) at Layer 4 AND a PRECONDITION (exit 2) at
#     Layer 7 → LAYER_ALL_EXIT=2 (2 > 1 > 0), not 1. Proves the max is across all layers.
# =============================================================================
d="$(new_stubdir)"
mk_stub "$d" 4 FAIL layer4-test-fail 1
mk_stub "$d" 7 PRECONDITION_FAILURE cluster-setup-failed 2
run_la "$d" DEVLOOP_TEST=1 LAYER_SCRIPT_DIR="$d"
assert_exit "g-worst-wins-exit2" 2 "$LA_RC"

# =============================================================================
# BYPASS-CLOSURE (security): the LAYER_SCRIPT_DIR seam must be NON-bypassable in CI. A
# control whose FAIL path is never exercised silently rots (audit-suppressions sentinel-B
# precedent). 7a/7b deliberately KEEP GITHUB_ACTIONS=1 (the inverse of the cases above) so
# assert_no_ci_sentinel_leak fires at the TOP of layer-all, before the layer loop.
# =============================================================================
# 7a — THE pin for the new clause: GITHUB_ACTIONS=1 + LAYER_SCRIPT_DIR, DEVLOOP_TEST UNSET
# is the only config where ONLY the layer-script-dir clause can fire. Exact token + no stub.
d="$(new_stubdir)"
run_la "$d" GITHUB_ACTIONS=1 LAYER_SCRIPT_DIR="$d"
out="$(cat "$LA_OUT")"
assert_exit       "7a-rejected-nonzero"   1 "$LA_RC"
assert_status     "7a-exact-token"        "REASON=layer-script-dir-set-in-ci" "$out"
assert_absent     "7a-no-spurious-green"  "TOTAL_RESULT=OK" "$out"
assert_no_marker  "7a-no-stub-ran"

# 7b — GITHUB_ACTIONS=1 + DEVLOOP_TEST=1 + LAYER_SCRIPT_DIR: BOTH sentinel clauses match,
# but `test-sentinel-set-in-ci` is the earlier `if` and short-circuits. Assert LOOSELY
# (non-zero + one of the two sentinel tokens + no stub) — pinning 7b to the layer-script-dir
# token would make it brittle to clause order.
d="$(new_stubdir)"
run_la "$d" GITHUB_ACTIONS=1 DEVLOOP_TEST=1 LAYER_SCRIPT_DIR="$d"
out="$(cat "$LA_OUT")"
assert_exit "7b-rejected-nonzero" 1 "$LA_RC"
if [[ "$out" == *"REASON=test-sentinel-set-in-ci"* || "$out" == *"REASON=layer-script-dir-set-in-ci"* ]]; then
  PASS=$((PASS+1)); else
  FAIL=$((FAIL+1)); FAILURES+=("[7b-a-sentinel-token] neither sentinel token present in output"); fi
assert_no_marker "7b-no-stub-ran"

# 7c — POSITIVE no-over-fire (security refinement C): GITHUB_ACTIONS UNSET + DEVLOOP_TEST=1
# + LAYER_SCRIPT_DIR → the seam is HONORED (stubs run), NO sentinel rejection. A control
# must also be proven to STAY SILENT when it shouldn't fire, or a too-broad clause would
# break the legit local seam with no test catching it.
d="$(new_stubdir)"
run_la "$d" DEVLOOP_TEST=1 LAYER_SCRIPT_DIR="$d"
out="$(cat "$LA_OUT")"
assert_exit   "7c-honored-exit0"        0 "$LA_RC"
assert_absent "7c-no-sentinel-rejection" "REASON=layer-script-dir-set-in-ci" "$out"
assert_marker "7c-seam-honored-stubs-ran"

report_results "scripts/layer-all.test.sh"
