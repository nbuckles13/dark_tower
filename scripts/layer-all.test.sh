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

# assert_absent / assert_marker / assert_no_marker are PROMOTED into
# lang/_test_helpers.sh (sourced above) — they were local closures over LA_DT and
# a hardcoded `ran.layer*` glob, which a second consumer could not reuse without
# them silently always passing. They now take <label> <dir> <glob>; pass "$LA_DT"
# and 'ran.layer*' explicitly at each call site below.

# =============================================================================
# (a) PRECONDITION_FAILURE at Layer 7 → LAYER_ALL_EXIT=2 + summary RESULT cell.
#     The operator lane survives the orchestrator (the repaired collapse).
# =============================================================================
d="$(new_stubdir)"; mk_stub "$d" 7 PRECONDITION_FAILURE cluster-provision-failed 2
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
# (c2) GREEN RUN WITH AN N/A LAYER (2026-10-06): N/A ranks just below OK, so the proto
#      placeholder's N/A layer no longer dominates — TOTAL_RESULT=OK, exit 0 — while the
#      per-layer line keeps saying N/A (the information is not lost, only demoted).
# =============================================================================
d="$(new_stubdir)"; mk_stub "$d" 4 N/A test-aggregate-na 0; mk_stub "$d" 6 N/A audit-aggregate-na 0
run_la "$d" DEVLOOP_TEST=1 LAYER_SCRIPT_DIR="$d"
out="$(cat "$LA_OUT")"
assert_exit   "c2-na-layer-exit0"   0 "$LA_RC"
assert_status "c2-na-layer-cell"    "LAYER=4 RESULT=N/A" "$out"
assert_status "c2-na-layer6-cell"   "LAYER=6 RESULT=N/A" "$out"
assert_status "c2-na-total-ok"      "TOTAL_RESULT=OK"    "$out"

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
assert_marker "f-all-ok-stubs-ran" "$LA_DT" 'ran.layer*'

# =============================================================================
# (g) WORST-WINS across layers (RUN-ALL mode): a FAIL (exit 1) at Layer 4 AND a
#     PRECONDITION (exit 2) at Layer 7 → LAYER_ALL_EXIT=2 (2 > 1 > 0), not 1. This
#     worst-wins-ACROSS-layers property only holds in RUN-ALL mode: under the new
#     interactive fail-fast DEFAULT (Change 2) the run stops at L4 (exit 1) and never
#     reaches L7 — that is case (i) below. Pinned to run-all via the explicit
#     DEVLOOP_FAIL_FAST=0 override (explicit-override→run-all); DEVLOOP_HEADLESS=1 is the
#     env-detection equivalent, covered by case (v).
# =============================================================================
d="$(new_stubdir)"
mk_stub "$d" 4 FAIL layer4-test-fail 1
mk_stub "$d" 7 PRECONDITION_FAILURE cluster-provision-failed 2
run_la "$d" DEVLOOP_TEST=1 LAYER_SCRIPT_DIR="$d" DEVLOOP_FAIL_FAST=0
out="$(cat "$LA_OUT")"; err="$(cat "$LA_ERR")"
assert_exit   "g-worst-wins-exit2" 2 "$LA_RC"
assert_status "g-worst-wins-total" "TOTAL_RESULT=PRECONDITION_FAILURE" "$out"
assert_status "g-run-all-mode"     "PIPELINE_MODE=run-all SOURCE=run-all-env" "$err"
assert_marker "g-all-layers-ran"   "$LA_DT" 'ran.layer7'   # run-all reached L7 (no early stop)

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
assert_no_marker  "7a-no-stub-ran" "$LA_DT" 'ran.layer*'

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
assert_no_marker "7b-no-stub-ran" "$LA_DT" 'ran.layer*'

# 7c — POSITIVE no-over-fire (security refinement C): GITHUB_ACTIONS UNSET + DEVLOOP_TEST=1
# + LAYER_SCRIPT_DIR → the seam is HONORED (stubs run), NO sentinel rejection. A control
# must also be proven to STAY SILENT when it shouldn't fire, or a too-broad clause would
# break the legit local seam with no test catching it.
d="$(new_stubdir)"
run_la "$d" DEVLOOP_TEST=1 LAYER_SCRIPT_DIR="$d"
out="$(cat "$LA_OUT")"
assert_exit   "7c-honored-exit0"        0 "$LA_RC"
assert_absent "7c-no-sentinel-rejection" "REASON=layer-script-dir-set-in-ci" "$out"
assert_marker "7c-seam-honored-stubs-ran" "$LA_DT" 'ran.layer*'

# =============================================================================
# (h) INPUT-SIDE SEAM: a shared helper's stdout must not inject a verdict vote.
# =============================================================================
# This file exists to pin the STATUS parse -> aggregate -> final_exit seam. This
# is that same seam from the INPUT side: `tee_collect_statuses` anchors
# `^STATUS=` at LINE START on stdout, and every match becomes a vote in the
# enclosing layer's verdict. All 10 consumers of lang/_test_helpers.sh run under
# `run_and_emit` inside a layer script, and a FAILURES[] entry routinely
# contains the text `STATUS=...` — any assertion about a status line quotes one.
# `report_results`' leading prefixes are the ONLY thing keeping that text
# mid-line and therefore uncollectable. Nothing asserted it until now: the
# property would have stopped holding with nothing going red.
#
# THE FIXTURE MUST START WITH `STATUS=` AT CHARACTER ZERO. Every FAILURES+=
# producer in _test_helpers.sh begins its entry with `[${label}]`, so an entry
# copying that shape stays mid-line whether or not the prefix exists — measured:
# with a `[case] …` entry the negative half passes with the prefix AND without
# it, so the case certifies a property it cannot see. Only a line-start
# `STATUS=` distinguishes them.
#
# Both halves are required. The negative alone is vacuous — it also passes if
# `report_results` stops printing FAILURES entries at all, which would destroy
# the diagnostics the prefix exists to carry while reporting the property holds.
#
# This case was itself the fifth sighting of the class it exists to serve, and
# of the CORRECT form of the question: "what else produces this same
# observation?" The observation "no line-start STATUS=" was produced by the
# prefix working AND by a fixture that could never trigger it. The narrower
# question ("does this pass if the thing never runs?") clears it — as it clears
# OPS-1, where the mechanism ran and the file was created.
__rr_out="$(
  PASS=0 FAIL=1 FAILURES=("STATUS=FAIL REASON=rr-selftest-needle — injected at line start")
  report_results "rr-selftest" 2>&1 || true
)"
assert_absent "h-report-results-no-collectable-status" "$(printf '\nSTATUS=')" "$(printf '\n%s' "$__rr_out")"
assert_status "h-report-results-still-prints-failures" "rr-selftest-needle" "$__rr_out"

# =============================================================================
# FAIL-FAST (Change 2) — the interactive DEFAULT. run_la scrubs GITHUB_ACTIONS /
# DEVLOOP_HEADLESS / DEVLOOP_FAIL_FAST (env -i), so fail_fast_mode() resolves to
# FAILFAST interactive-default: the cases below set NO mode env and exercise the default.
# =============================================================================

# (i) FAIL-FAST default: FAIL@L4 (rc 1) + PRECONDITION@L7 — the SAME stub tree as (g), but
#     interactive ⇒ STOP at L4. The paired contrast with (g): the exit is the FAILING layer's
#     (1), NOT the max-across-layers (2) — proving L7's PRECONDITION is never even reached.
#     L5-7 render RESULT=NOT-RUN, their stubs never ran (no markers), and none reads OK (would
#     forge GATE2=PASS over un-run work) or UNKNOWN (the `${:-UNKNOWN}` default → exit 2, which
#     would inflate the verdict) — the honesty + no-leak constraints (@security S2).
d="$(new_stubdir)"
mk_stub "$d" 4 FAIL layer4-test-fail 1
mk_stub "$d" 7 PRECONDITION_FAILURE cluster-provision-failed 2
run_la "$d" DEVLOOP_TEST=1 LAYER_SCRIPT_DIR="$d"
out="$(cat "$LA_OUT")"; err="$(cat "$LA_ERR")"
assert_exit      "i-failfast-exit1-not-2" 1 "$LA_RC"          # L4's rc, NOT L7's 2
assert_status    "i-failfast-total-fail"  "TOTAL_RESULT=FAIL" "$out"
assert_status    "i-failfast-mode"        "PIPELINE_MODE=fail-fast SOURCE=interactive-default" "$err"
assert_status    "i-stopped-early-line"   "STOPPED_EARLY LAYER=4 RESULT=FAIL NOT_RUN=5,6,7" "$err"
assert_status    "i-l7-not-run-cell"      "LAYER=7 RESULT=NOT-RUN" "$out"
assert_marker    "i-l4-ran"               "$LA_DT" 'ran.layer4'
assert_no_marker "i-l5-not-run"           "$LA_DT" 'ran.layer5'
assert_no_marker "i-l7-not-run"           "$LA_DT" 'ran.layer7'
assert_absent    "i-no-unknown-cell"      "RESULT=UNKNOWN" "$out"
assert_absent    "i-l7-not-ok"            "LAYER=7 RESULT=OK" "$out"
# Fail-fast skipped layer 6 → the fast-tier budget cannot be measured; loud skip token.
assert_status    "i-budget-skipped"       "WARN BUDGET_TOTAL_SKIPPED REASON=layers-not-run LAST_RAN=4" "$err"

# (ii) FAIL-FAST + PRECONDITION@L3 (rc 2): the composed Change-1×Change-2 case — an L3
#      guard-timeout-style operator-lane precondition STOPS the interactive run at L3, before
#      L4-7 (incl. L7's cluster bring-up — exactly the 2026-08-14 waste both changes exist to
#      kill). exit 2, TOTAL=PRECONDITION_FAILURE.
d="$(new_stubdir)"
mk_stub "$d" 3 PRECONDITION_FAILURE guard-timeout-validate-kustomize 2
run_la "$d" DEVLOOP_TEST=1 LAYER_SCRIPT_DIR="$d"
out="$(cat "$LA_OUT")"; err="$(cat "$LA_ERR")"
assert_exit      "ii-precondition-exit2" 2 "$LA_RC"
assert_status    "ii-total-precondition" "TOTAL_RESULT=PRECONDITION_FAILURE" "$out"
assert_status    "ii-stopped-at-3"       "STOPPED_EARLY LAYER=3 RESULT=PRECONDITION_FAILURE NOT_RUN=4,5,6,7" "$err"
assert_no_marker "ii-l4-not-run"         "$LA_DT" 'ran.layer4'
assert_no_marker "ii-l7-not-run"         "$LA_DT" 'ran.layer7'

# (S1) FAIL-CLOSED (@security S1): a layer whose status is an EXIT-0 skip
#      (N/A / SKIPPED-NO-DIFF / SKIPPED-NO-CLUSTER) must NOT trigger the fail-fast stop — the
#      run continues and STILL reaches layer 7. This GUARDS AGAINST a future edit that keys the
#      stop on "status != OK" instead of `rc != 0`: that edit would halt on a benign exit-0
#      skip and forge a GATE2=PASS over layers that never ran. It reads as a tautology against
#      today's `rc != 0` stop — the tautology IS the point until someone breaks it; do not
#      delete it as redundant.
for skip in "N/A" "SKIPPED-NO-DIFF" "SKIPPED-NO-CLUSTER"; do
  d="$(new_stubdir)"
  mk_stub "$d" 3 "$skip" "layer3-${skip}" 0
  run_la "$d" DEVLOOP_TEST=1 LAYER_SCRIPT_DIR="$d"
  err="$(cat "$LA_ERR")"
  assert_exit   "S1-${skip}-exit0"         0 "$LA_RC"
  assert_marker "S1-${skip}-l7-still-ran"  "$LA_DT" 'ran.layer7'
  assert_absent "S1-${skip}-no-early-stop" "STOPPED_EARLY" "$err"
done

# (k) AUTHORITY-LANE REFUSAL (@security R-B, Model B): an unattended run (DEVLOOP_HEADLESS=1)
#     with an ambient DEVLOOP_FAIL_FAST=1 must NOT fail-fast — the CI/story authority run's
#     one-pass coverage cannot be truncated by an ambient var. fail_fast_mode() returns
#     `RUNALL unattended-override-refused`; the run goes run-all (all layers), and the refusal
#     is LOUD (grep-able), never silent (it cannot forge green — a red run stays red). Same
#     stub tree as (g)/(i) → exit 2, all ran.
d="$(new_stubdir)"
mk_stub "$d" 4 FAIL layer4-test-fail 1
mk_stub "$d" 7 PRECONDITION_FAILURE cluster-provision-failed 2
run_la "$d" DEVLOOP_TEST=1 LAYER_SCRIPT_DIR="$d" DEVLOOP_HEADLESS=1 DEVLOOP_FAIL_FAST=1
err="$(cat "$LA_ERR")"
assert_exit   "k-refused-runs-all-exit2" 2 "$LA_RC"
assert_marker "k-refused-l7-ran"         "$LA_DT" 'ran.layer7'
assert_status "k-refused-mode-line"      "PIPELINE_MODE=run-all SOURCE=unattended-override-refused" "$err"
assert_status "k-refused-warn"           "WARN FAIL_FAST_OVERRIDE_IGNORED REQUESTED=1 MODE=run-all" "$err"

# (v) ENV-DETECTION run-all: DEVLOOP_HEADLESS=1 (no explicit knob) ⇒ RUNALL headless — the
#     story-runner path. Same tree as (g); proves unattended detection alone forces run-all.
d="$(new_stubdir)"
mk_stub "$d" 4 FAIL layer4-test-fail 1
mk_stub "$d" 7 PRECONDITION_FAILURE cluster-provision-failed 2
run_la "$d" DEVLOOP_TEST=1 LAYER_SCRIPT_DIR="$d" DEVLOOP_HEADLESS=1
err="$(cat "$LA_ERR")"
assert_exit   "v-headless-runs-all-exit2" 2 "$LA_RC"
assert_marker "v-headless-l7-ran"         "$LA_DT" 'ran.layer7'
assert_status "v-headless-mode-line"      "PIPELINE_MODE=run-all SOURCE=headless" "$err"

# (vii) BAD VALUE fail-closed: DEVLOOP_FAIL_FAST=ture → fail_fast_mode()=INVALID → the caller
#       exits 2 LOUD *before the layer loop* (a typo must never silently pick a mode). No stub
#       runs. Greppable boolean-error message, distinct from the merge-base PRECONDITION line.
d="$(new_stubdir)"
run_la "$d" DEVLOOP_TEST=1 LAYER_SCRIPT_DIR="$d" DEVLOOP_FAIL_FAST=ture
err="$(cat "$LA_ERR")"
assert_exit      "vii-invalid-exit2"   2 "$LA_RC"
assert_status    "vii-invalid-message" "DEVLOOP_FAIL_FAST=ture is not a recognized boolean" "$err"
assert_no_marker "vii-no-stub-ran"     "$LA_DT" 'ran.layer*'

# =============================================================================
# D8 (ADR-0037) — FAILURE_TRIAGE point-of-failure directive. Emitted on STDERR ($LA_ERR), one per
# ACTIONABLE-failed layer, in a single block after the human table. ADD-ONLY: the a–vii exit-code/
# summary asserts above are UNEDITED (byte-identity proof). Directive keys on
# status_to_exit_code(status)!=0 UNION rc!=0, with NOT-RUN excluded first.
# =============================================================================

# (D8-1) FAIL@L4 → directive on stderr naming BOTH logs + the anti-pattern + a prefixed teaser;
#        NOT on stdout (would collide with the summary-cell substring assertions). Default
#        fail-fast stops at L4, so L5-7 are NOT-RUN and get no directive.
d="$(new_stubdir)"; mk_stub "$d" 4 FAIL layer4-test-fail 1
run_la "$d" DEVLOOP_TEST=1 LAYER_SCRIPT_DIR="$d"
out="$(cat "$LA_OUT")"; err="$(cat "$LA_ERR")"
assert_status "d8-1-triage-l4"        "FAILURE_TRIAGE LAYER=4 " "$err"
assert_status "d8-1-triage-log"       "layer-4.log"             "$err"
assert_status "d8-1-triage-stderrlog" "layer-4.stderr.log"      "$err"
assert_status "d8-1-antipattern"      "do NOT re-run the layer" "$err"
assert_status "d8-1-teaser-prefixed"  "    | STATUS=FAIL"       "$err"   # prefix (ops OPS-D) + teaser present
assert_absent "d8-1-not-on-stdout"    "FAILURE_TRIAGE"          "$out"
assert_absent "d8-1-no-triage-l7"     "FAILURE_TRIAGE LAYER=7"  "$err"   # NOT-RUN layer gets nothing

# (D8-2) all-OK → NO directive at all (the negative half; without it the assertion is vacuous —
#        it would pass if the directive printed unconditionally).
d="$(new_stubdir)"
run_la "$d" DEVLOOP_TEST=1 LAYER_SCRIPT_DIR="$d"
err="$(cat "$LA_ERR")"
assert_absent "d8-2-all-ok-no-triage" "FAILURE_TRIAGE" "$err"

# (D8-3) SKIPPED-NO-CLUSTER@L7 (exit 0, non-OK status) → NO directive (test's required negative:
#        a `!= OK` predicate would wrongly fire; keying on status_to_exit_code()==0 does not). This
#        is the realistic CI false-positive (L7 green-skips every cluster-less CI run).
d="$(new_stubdir)"; mk_stub "$d" 7 SKIPPED-NO-CLUSTER no-cluster-ci 0
run_la "$d" DEVLOOP_TEST=1 LAYER_SCRIPT_DIR="$d"
err="$(cat "$LA_ERR")"
assert_absent "d8-3-skipped-no-triage" "FAILURE_TRIAGE" "$err"

# (D8-4) RUN-ALL multi-red (FAIL@L4 + PRECONDITION@L7) → a directive for BOTH failing layers.
d="$(new_stubdir)"; mk_stub "$d" 4 FAIL layer4-test-fail 1; mk_stub "$d" 7 PRECONDITION_FAILURE cluster-provision-failed 2
run_la "$d" DEVLOOP_TEST=1 LAYER_SCRIPT_DIR="$d" DEVLOOP_FAIL_FAST=0
err="$(cat "$LA_ERR")"
assert_status "d8-4-triage-l4" "FAILURE_TRIAGE LAYER=4 " "$err"
assert_status "d8-4-triage-l7" "FAILURE_TRIAGE LAYER=7 " "$err"

# (D8-5) FAIL-FAST (same tree, default mode) → stops at L4: directive for L4, ABSENT for NOT-RUN L7.
d="$(new_stubdir)"; mk_stub "$d" 4 FAIL layer4-test-fail 1; mk_stub "$d" 7 PRECONDITION_FAILURE cluster-provision-failed 2
run_la "$d" DEVLOOP_TEST=1 LAYER_SCRIPT_DIR="$d"
err="$(cat "$LA_ERR")"
assert_status "d8-5-ff-triage-l4"    "FAILURE_TRIAGE LAYER=4 " "$err"
assert_absent "d8-5-ff-no-triage-l7" "FAILURE_TRIAGE LAYER=7"  "$err"

# (D8-6) LYING STATUS=OK; exit 1 (rc-keyed, not status-keyed): status_to_exit_code(OK)=0 but rc=1
#        → the directive STILL fires. The most operator-hostile case (table reads OK, exit non-zero).
d="$(new_stubdir)"; mk_stub "$d" 7 OK lying-status-line 1
run_la "$d" DEVLOOP_TEST=1 LAYER_SCRIPT_DIR="$d"
err="$(cat "$LA_ERR")"
assert_status "d8-6-lying-triage-l7" "FAILURE_TRIAGE LAYER=7 " "$err"

# (D8-7) NO-MATCH TEASER + exit survives (security G3, BLOCKING): two PRECONDITION layers in run-all
#        whose logs contain NONE of the teaser anchors (STATUS=PRECONDITION_FAILURE matches neither
#        ^STATUS=FAIL nor ^PRECONDITION_FAILURE:). The `|| true` must keep set -e from aborting on
#        the grep no-match, so: (a) LAYER_ALL_EXIT stays the TRUE final_exit (2, not demoted to 1),
#        and (b) the loop COMPLETES — BOTH L3 and L7 get a directive (a mid-loop abort would drop L7).
d="$(new_stubdir)"; mk_stub "$d" 3 PRECONDITION_FAILURE guard-timeout 2; mk_stub "$d" 7 PRECONDITION_FAILURE cluster-provision-failed 2
run_la "$d" DEVLOOP_TEST=1 LAYER_SCRIPT_DIR="$d" DEVLOOP_FAIL_FAST=0
err="$(cat "$LA_ERR")"
assert_exit   "d8-7-exit2-survives-nomatch" 2 "$LA_RC"
assert_status "d8-7-triage-l3"              "FAILURE_TRIAGE LAYER=3 "       "$err"
assert_status "d8-7-triage-l7-loop-done"    "FAILURE_TRIAGE LAYER=7 "       "$err"
assert_status "d8-7-no-teaser-line"         "no teaser — read the log(s)"   "$err"

# (D8-8) PER-LOG TEASER BUDGET (obs F3): the stderr-only detail must surface even when the stdout
#        log already has multiple matching lines — a guard-timeout `PRECONDITION_FAILURE:` line is
#        stderr-only and coexists with stdout violations (runbook §6.3 mixed lane). A single `head`
#        over the concatenation would let the ≥2 stdout matches starve it; the per-log `head -2`
#        guarantees each channel is represented. Custom stub: 3 stdout VIOLATION lines + 1 stderr
#        PRECONDITION line. (Would RED on the old concatenation approach — non-vacuous.)
d="$(new_stubdir)"
cat > "$d/layer3.sh" <<'STUB'
#!/usr/bin/env bash
printf 'VIOLATION: guard-a\nVIOLATION: guard-b\nVIOLATION: guard-c\nSTATUS=FAIL REASON=guard-violations\n'
printf 'PRECONDITION_FAILURE: guard xyz timed out after 30s\n' >&2
exit 1
STUB
chmod +x "$d/layer3.sh"
run_la "$d" DEVLOOP_TEST=1 LAYER_SCRIPT_DIR="$d" DEVLOOP_FAIL_FAST=0
err="$(cat "$LA_ERR")"
assert_status "d8-8-stdout-teaser-line"  "    | VIOLATION: guard-a"              "$err"
assert_status "d8-8-stderr-surfaces"     "    | PRECONDITION_FAILURE: guard xyz" "$err"

# =============================================================================
# (F) FAST LANE — `--max-layer N` / scripts/layer-fast.sh (developer inner-loop).
#     Layers 1..N only, NO Gate-2 verdict, REFUSED in an attesting context. This is
#     the seam that keeps a diligent implementer's self-check off the shared Layer-7
#     cluster the Lead's Gate 2 owns. run_la doesn't forward positional args, so a
#     tiny arg-forwarding twin drives these (same env -i + fresh DEVLOOP_TMP).
# =============================================================================
run_la_args() {
  shift  # $1 is the stubdir (call-site symmetry with run_la); the dir reaches layer-all via LAYER_SCRIPT_DIR.
  local la_args="$1"; shift
  local t; t="$(mktemp -d "$WORK/run.XXXXXX")"
  LA_DT="$t/dt"; LA_OUT="$t/out"; LA_ERR="$t/err"
  # The suite's IFS=$'\n\t' (line 23) has no space, so split la_args on spaces here.
  local IFS=' '
  # la_args is a deliberate word-split list of literal flags (e.g. "--max-layer 6").
  # shellcheck disable=SC2086
  env -i PATH="$PATH" HOME="$HOME" DEVLOOP_TMP="$LA_DT" "$@" \
      bash "$LAYER_ALL" $la_args >"$LA_OUT" 2>"$LA_ERR"
  LA_RC=$?
}

# (F1) HAPPY PATH: --max-layer 6 runs layers 1-6, STOPS before 7, exits 0, prints the
#      FAST_LANE banner, and writes NO gate2-verdict (the no-authority guarantee).
d="$(new_stubdir)"
run_la_args "$d" "--max-layer 6" DEVLOOP_TEST=1 LAYER_SCRIPT_DIR="$d"
out="$(cat "$LA_OUT")"; err="$(cat "$LA_ERR")"
assert_exit      "f1-fast-exit0"          0 "$LA_RC"
assert_status    "f1-fast-banner"         "FAST_LANE=1 LAYERS=1-6"          "$err"
assert_marker    "f1-l6-ran"              "$LA_DT" 'ran.layer6'
assert_no_marker "f1-l7-not-run"          "$LA_DT" 'ran.layer7'
assert_status    "f1-summary-has-l6"      "LAYER=6 RESULT=OK"               "$out"
assert_absent    "f1-summary-no-l7"       "LAYER=7"                         "$out"
assert_no_marker "f1-no-gate2-verdict"    "$LA_DT" 'gate2-verdict'

# (F2) CONTROL: a bare (full) run over the SAME stubs DOES run layer 7 and DOES write the
#      verdict — proves F1's absences are the fast lane, not a broken fixture.
d="$(new_stubdir)"
run_la "$d" DEVLOOP_TEST=1 LAYER_SCRIPT_DIR="$d"
assert_marker "f2-full-l7-ran"        "$LA_DT" 'ran.layer7'
assert_marker "f2-full-gate2-verdict" "$LA_DT" 'gate2-verdict'

# (F3) ATTESTING-CONTEXT REFUSAL (security): --max-layer < 7 under GITHUB_ACTIONS is rejected
#      exit 2 with the greppable token, and NO stub runs — the authority pipeline can never be
#      shortened. (DEVLOOP_TEST=1 present so the seam is honored; the refusal precedes the loop.)
d="$(new_stubdir)"
run_la_args "$d" "--max-layer 6" DEVLOOP_TEST=1 GITHUB_ACTIONS=1 LAYER_SCRIPT_DIR="$d"
err="$(cat "$LA_ERR")"
assert_exit      "f3-refuse-exit2"    2 "$LA_RC"
assert_status    "f3-refuse-token"    "REASON=fast-lane-in-attesting-context" "$err"
assert_no_marker "f3-no-stub-ran"     "$LA_DT" 'ran.layer*'

# (F4) ARG VALIDATION: out-of-range and unknown flags fail loud (exit 2), no stub runs.
d="$(new_stubdir)"
run_la_args "$d" "--max-layer 9" DEVLOOP_TEST=1 LAYER_SCRIPT_DIR="$d"
assert_exit      "f4-range-exit2"     2 "$LA_RC"
assert_status    "f4-range-token"     "--max-layer must be an integer 1..7" "$(cat "$LA_ERR")"
assert_no_marker "f4-range-no-stub"   "$LA_DT" 'ran.layer*'
d="$(new_stubdir)"
run_la_args "$d" "--bogus" DEVLOOP_TEST=1 LAYER_SCRIPT_DIR="$d"
assert_exit   "f4-unknown-exit2"      2 "$LA_RC"
assert_status "f4-unknown-token"      "unknown argument" "$(cat "$LA_ERR")"

# =============================================================================
# (G) CI SHARD — `--layers L,...` (one parallel CI job's slice of the pipeline).
#     Runs exactly its layers from scratch, writes the shard summary the Test Suite
#     aggregate reads, and NEVER writes, removes or overwrites a Gate-2 verdict.
#     Driven through the LAYER_SCRIPT_DIR test seam (GITHUB_ACTIONS is rejected with it).
# =============================================================================
# run_shard <stubdir> <layers-arg> [KEY=VAL ...] — like run_la_args, but seeds a sentinel
# verdict file in the fresh DEVLOOP_TMP first, so a case can prove it is left untouched.
run_shard() {
  local stubdir="$1" layers="$2"; shift 2
  local t; t="$(mktemp -d "$WORK/run.XXXXXX")"
  LA_DT="$t/dt"; LA_OUT="$t/out"; LA_ERR="$t/err"
  mkdir -p "$LA_DT"; printf 'sentinel-verdict\n' > "$LA_DT/gate2-verdict"
  env -i PATH="$PATH" HOME="$HOME" DEVLOOP_TMP="$LA_DT" DEVLOOP_TEST=1 LAYER_SCRIPT_DIR="$stubdir" "$@" \
      bash "$LAYER_ALL" --layers "$layers" >"$LA_OUT" 2>"$LA_ERR"
  LA_RC=$?
}

# (G1) HAPPY PATH: --layers 1,3 runs layers 1 and 3 only, writes exactly their summary
#      lines, and leaves the pre-existing verdict byte-identical.
d="$(new_stubdir)"
run_shard "$d" 1,3 GITHUB_OUTPUT="$WORK/ghout.g1"
assert_exit      "g1-shard-exit0"          0 "$LA_RC"
# CI's upload step reads the summary's path from this output (never restates it).
g1_path="$(sed -n 's/^summary-path=//p' "$WORK/ghout.g1" 2>/dev/null)"
assert_exit      "g1-summary-path-output"  0 "$([[ "$g1_path" == "$LA_DT/layer-summary.txt" && -s "$g1_path" ]] && echo 0 || echo "1 (${g1_path})")"
assert_marker    "g1-l1-ran"               "$LA_DT" 'ran.layer1'
assert_marker    "g1-l3-ran"               "$LA_DT" 'ran.layer3'
assert_no_marker "g1-l2-not-run"           "$LA_DT" 'ran.layer2'
assert_no_marker "g1-l7-not-run"           "$LA_DT" 'ran.layer7'
assert_status    "g1-banner"               "CI_SHARD=1 LAYERS=1,3" "$(cat "$LA_ERR")"
g1_sum="$(sed 's/DURATION=[0-9]*/DURATION=D/' "$LA_DT/layer-summary.txt" 2>/dev/null)"
assert_exit      "g1-summary-exact"        0 "$([[ "$g1_sum" == $'LAYER=1 RESULT=OK DURATION=D RC=0\nLAYER=3 RESULT=OK DURATION=D RC=0' ]] && echo 0 || echo 1)"
assert_exit      "g1-verdict-untouched"    0 "$([[ "$(cat "$LA_DT/gate2-verdict")" == "sentinel-verdict" ]] && echo 0 || echo 1)"

# (G2) The summary carries each layer's REAL exit code (the input to the aggregate's rc
#      FLOOR): a lying `STATUS=OK; exit 1` layer is recorded RESULT=OK RC=1.
d="$(new_stubdir)"; mk_stub "$d" 3 OK lying-ok 1
run_shard "$d" 3 DEVLOOP_FAIL_FAST=0
assert_exit   "g2-shard-exit1"        1 "$LA_RC"
assert_status "g2-summary-real-rc"    "LAYER=3 RESULT=OK DURATION=" "$(cat "$LA_DT/layer-summary.txt" 2>/dev/null)"
assert_status "g2-summary-rc1"        " RC=1" "$(cat "$LA_DT/layer-summary.txt" 2>/dev/null)"

# (G3) REFUSALS (exit 2, nothing runs): outside CI with no test seam; inside a run-story
#      gate / pre-commit (DEVLOOP_FMT_CHECK_ONLY); malformed lists; mixed with --max-layer.
d="$(new_stubdir)"
t="$(mktemp -d "$WORK/run.XXXXXX")"
env -i PATH="$PATH" HOME="$HOME" DEVLOOP_TMP="$t" bash "$LAYER_ALL" --layers 1 >/dev/null 2>"$t/err"
assert_exit      "g3-local-refused-exit2"  2 "$?"
assert_status    "g3-local-refused-token"  "REASON=shard-outside-ci" "$(cat "$t/err")"
run_shard "$d" 1 DEVLOOP_FMT_CHECK_ONLY=1
assert_exit      "g3-attesting-refused"    2 "$LA_RC"
assert_no_marker "g3-attesting-no-stub"    "$LA_DT" 'ran.layer*'
for bad in "2,1" "1,1" "8" "0" "1,x" ","; do
  run_shard "$d" "$bad"
  assert_exit      "g3-list-${bad:-empty}-exit2"   2 "$LA_RC"
  assert_status    "g3-list-${bad:-empty}-token"   "REASON=shard-layer-list-invalid" "$(cat "$LA_ERR")"
  assert_no_marker "g3-list-${bad:-empty}-no-stub" "$LA_DT" 'ran.layer*'
done
run_shard "$d" ""
assert_exit   "g3-list-empty-exit2"  2 "$LA_RC"
assert_status "g3-list-empty-token"  "--layers needs a value" "$(cat "$LA_ERR")"
run_la_args "$d" "--max-layer 3 --layers 1" DEVLOOP_TEST=1 LAYER_SCRIPT_DIR="$d"
assert_exit   "g3-exclusive-exit2"   2 "$LA_RC"
assert_status "g3-exclusive-token"   "mutually exclusive" "$(cat "$LA_ERR")"

# =============================================================================
# (H) CI AGGREGATE — `--aggregate DIR` (the `Test Suite` job). Every layer 1..LAYER_MAX
#     exactly once across the shard summaries, strictly parsed; worst status + rc FLOOR.
# =============================================================================
# mk_summary <aggdir> <shard> <line>... — write <aggdir>/<shard>/layer-summary.txt.
mk_summary() { local a="$1" sh="$2"; shift 2; mkdir -p "$a/$sh"; printf '%s\n' "$@" > "$a/$sh/layer-summary.txt"; }
# mk_partition <aggdir> — the real CI partition, all green (layer 7 = the CI clean skip).
mk_partition() {
  mk_summary "$1" guards "LAYER=1 RESULT=OK DURATION=1 RC=0" "LAYER=2 RESULT=OK DURATION=1 RC=0" \
    "LAYER=3 RESULT=OK DURATION=1 RC=0" "LAYER=6 RESULT=OK DURATION=1 RC=0" \
    "LAYER=7 RESULT=SKIPPED-NO-CLUSTER DURATION=0 RC=0"
  mk_summary "$1" test "LAYER=4 RESULT=OK DURATION=1 RC=0"
  mk_summary "$1" lint "LAYER=5 RESULT=OK DURATION=1 RC=0"
}
run_agg() {
  LA_OUT="$WORK/agg.out"; LA_ERR="$WORK/agg.err"
  env -i PATH="$PATH" HOME="$HOME" DEVLOOP_TMP="$WORK/agg.dt" bash "$LAYER_ALL" --aggregate "$1" >"$LA_OUT" 2>"$LA_ERR"
  LA_RC=$?
}

a="$(mktemp -d "$WORK/agg.XXXXXX")"; mk_partition "$a"; run_agg "$a"
assert_exit   "h1-all-green-exit0"     0 "$LA_RC"
assert_status "h1-total-ok"            "TOTAL_RESULT=OK" "$(cat "$LA_OUT")"
assert_status "h1-l7-skip-cell"        "LAYER=7 RESULT=SKIPPED-NO-CLUSTER" "$(cat "$LA_OUT")"
assert_absent "h1-no-triage"           "FAILURE_TRIAGE" "$(cat "$LA_ERR")"

# The same N/A demotion on the CI aggregate path (one ladder: aggregate_worst_status).
a="$(mktemp -d "$WORK/agg.XXXXXX")"; mk_partition "$a"
mk_summary "$a" guards "LAYER=1 RESULT=OK DURATION=1 RC=0" "LAYER=2 RESULT=OK DURATION=1 RC=0" \
  "LAYER=3 RESULT=OK DURATION=1 RC=0" "LAYER=6 RESULT=N/A DURATION=1 RC=0" \
  "LAYER=7 RESULT=SKIPPED-NO-CLUSTER DURATION=0 RC=0"
mk_summary "$a" test "LAYER=4 RESULT=N/A DURATION=1 RC=0"
run_agg "$a"
assert_exit   "h1b-na-shards-exit0"    0 "$LA_RC"
assert_status "h1b-na-cell-kept"       "LAYER=6 RESULT=N/A" "$(cat "$LA_OUT")"
assert_status "h1b-na-total-ok"        "TOTAL_RESULT=OK" "$(cat "$LA_OUT")"

a="$(mktemp -d "$WORK/agg.XXXXXX")"; mk_partition "$a"; mk_summary "$a" test "LAYER=4 RESULT=FAIL DURATION=9 RC=1"; run_agg "$a"
assert_exit   "h2-worst-status-exit1"  1 "$LA_RC"
assert_status "h2-total-fail"          "TOTAL_RESULT=FAIL" "$(cat "$LA_OUT")"
assert_status "h2-triage-names-shard"  "FAILURE_TRIAGE LAYER=4 SHARD=test" "$(cat "$LA_ERR")"

a="$(mktemp -d "$WORK/agg.XXXXXX")"; mk_partition "$a"; mk_summary "$a" lint "LAYER=5 RESULT=OK DURATION=1 RC=1"; run_agg "$a"
assert_exit   "h3-rc-floor-wins"       1 "$LA_RC"

a="$(mktemp -d "$WORK/agg.XXXXXX")"; mk_partition "$a"; mk_summary "$a" test "LAYER=4 RESULT=PRECONDITION_FAILURE DURATION=1 RC=2"; run_agg "$a"
assert_exit   "h4-operator-lane-exit2" 2 "$LA_RC"

# Every malformed partition is exit 2 with its own REASON (never a pass, never a FAIL).
agg_case() {  # agg_case <label> <reason>  (aggregate the dir in $a)
  run_agg "$a"
  assert_exit   "$1-exit2"  2 "$LA_RC"
  assert_status "$1-reason" "REASON=aggregate-$2" "$(cat "$LA_OUT")"
}
a="$(mktemp -d "$WORK/agg.XXXXXX")"; mk_partition "$a"; rm "$a/lint/layer-summary.txt"; agg_case h5-missing-layer missing-layer
a="$(mktemp -d "$WORK/agg.XXXXXX")"; mk_partition "$a"; mk_summary "$a" extra "LAYER=4 RESULT=OK DURATION=1 RC=0"; agg_case h6-duplicate-layer duplicate-layer
a="$(mktemp -d "$WORK/agg.XXXXXX")"; mk_partition "$a"; mk_summary "$a" lint "LAYER=5 RESULT=OK DURATION=1"; agg_case h7-malformed-line malformed-line
a="$(mktemp -d "$WORK/agg.XXXXXX")"; mk_partition "$a"; mk_summary "$a" lint "LAYER=5 RESULT=PASSED DURATION=1 RC=0"; agg_case h8-unknown-status unknown-status
a="$(mktemp -d "$WORK/agg.XXXXXX")"; mk_partition "$a"; mk_summary "$a" lint "LAYER=5 RESULT=NOT-RUN DURATION=0 RC=0"; agg_case h9-not-run-is-not-a-result unknown-status
a="$(mktemp -d "$WORK/agg.XXXXXX")"; mk_partition "$a"; mk_summary "$a" lint "LAYER=5 RESULT=OK DURATION=1 RC=0" "LAYER=8 RESULT=OK DURATION=1 RC=0"; agg_case h10-out-of-range layer-out-of-range
a="$(mktemp -d "$WORK/agg.XXXXXX")"; mk_partition "$a"; mk_summary "$a" lint 'LAYER=5 RESULT=OK DURATION=1 RC=0; touch /tmp/x'; agg_case h11-trailing-text-rejected malformed-line
a="$(mktemp -d "$WORK/agg.XXXXXX")"; agg_case h12-empty-dir no-summaries
a="$WORK/does-not-exist"; agg_case h13-missing-dir dir-unreadable

# (H14) ROUND TRIP: real shard runs (G-mode, stub layers) feed the aggregate, which then
#       reports the full pipeline — proving the shard writer and aggregate parser agree.
d="$(new_stubdir)"; a="$(mktemp -d "$WORK/agg.XXXXXX")"
for part in guards:1,2,3,6,7 test:4 lint:5; do
  run_shard "$d" "${part#*:}"
  mkdir -p "$a/${part%%:*}"; cp "$LA_DT/layer-summary.txt" "$a/${part%%:*}/"
done
run_agg "$a"
assert_exit   "h14-round-trip-exit0"   0 "$LA_RC"
assert_status "h14-round-trip-l5"      "LAYER=5 RESULT=OK" "$(cat "$LA_OUT")"

# (H15) ONE EMITTER: the aggregate's LAYER_SUMMARY block is the same shape as a full run's
#       over the same stub layers (durations normalised) — the parsed contract cannot drift
#       between the single-invocation path and the CI aggregate.
summary_block() { sed -n '/=== LAYER_SUMMARY_BEGIN ===/,/^TOTAL_DURATION=/p' "$1" | sed 's/DURATION=[0-9]*/DURATION=D/'; }
h15_agg="$(summary_block "$LA_OUT")"
d="$(new_stubdir)"  # same all-OK stubs the H14 shards ran
run_la "$d" DEVLOOP_TEST=1 LAYER_SCRIPT_DIR="$d"
h15_full="$(summary_block "$LA_OUT")"
assert_exit "h15-summary-block-identical" 0 "$([[ -n "$h15_full" && "$h15_agg" == "$h15_full" ]] && echo 0 || echo "1 (agg: ${h15_agg//$'\n'/|} full: ${h15_full//$'\n'/|})")"

report_results "scripts/layer-all.test.sh"
