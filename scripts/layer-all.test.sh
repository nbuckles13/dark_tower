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
# LAYER_MAX (the one home), the verdict reader, and the zombie-aware liveness check the kill
# cases use. All three are source-safe (no shell options).
# shellcheck source=lang/_layers.sh
source "${__here}/lang/_layers.sh"
# shellcheck source=lang/_gate2_binding.sh
source "${__here}/lang/_gate2_binding.sh"
# shellcheck source=lang/_child_group.sh
source "${__here}/lang/_child_group.sh"

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
mk_all_ok() { local n; for (( n = 1; n <= LAYER_MAX; n++ )); do mk_stub "$1" "$n" OK "layer$n-ok" 0; done; }

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
run_la_args "$d" "--max-layer $((LAYER_MAX + 2))" DEVLOOP_TEST=1 LAYER_SCRIPT_DIR="$d"
assert_exit      "f4-range-exit2"     2 "$LA_RC"
assert_status    "f4-range-token"     "--max-layer must be an integer 1..${LAYER_MAX}" "$(cat "$LA_ERR")"
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

# =============================================================================
# (I) LAYER-7 SUITE SPLIT — DEVLOOP_LAYER7_SUITE (scripts/layer7.sh header §SUITE SELECTOR).
#     CI may run Layer 7 as one shard PER SUITE (each on its own cluster). layer-all passes the
#     variable to layer7.sh by inheritance, records it as SUITE= on the shard's layer-7 line,
#     refuses it anywhere that could stand for the whole pipeline, and the aggregate accepts a
#     split layer 7 only when the SUITE= fields cover LAYER7_SUITES exactly.
# =============================================================================
# A layer-7 stub that records the selector it inherited (the pass-through proof).
mk_l7_recorder() {
  cat > "$1/layer7.sh" <<'STUB'
#!/usr/bin/env bash
printf 'STATUS=OK REASON=layer7-ok\n'
printf '%s\n' "${DEVLOOP_LAYER7_SUITE-<unset>}" > "${DEVLOOP_TMP}/ran.layer7"
STUB
  chmod +x "$1/layer7.sh"
}

# (I1) shard --layers 6,7 + env-tests: the stub inherits the value, the banner names it, and
#      the summary carries SUITE= on the layer-7 line only.
d="$(new_stubdir)"; mk_l7_recorder "$d"
run_shard "$d" 6,7 DEVLOOP_LAYER7_SUITE=env-tests
assert_exit   "i1-split-shard-exit0"  0 "$LA_RC"
assert_status "i1-split-passthrough"  "env-tests" "$(cat "$LA_DT/ran.layer7" 2>/dev/null)"
assert_status "i1-split-banner"       "CI_SHARD=1 LAYERS=6,7 LAYER7_SUITE=env-tests" "$(cat "$LA_ERR")"
i1_sum="$(sed 's/DURATION=[0-9]*/DURATION=D/' "$LA_DT/layer-summary.txt" 2>/dev/null)"
assert_exit   "i1-split-summary-exact" 0 "$([[ "$i1_sum" == $'LAYER=6 RESULT=OK DURATION=D RC=0\nLAYER=7 RESULT=OK DURATION=D RC=0 SUITE=env-tests' ]] && echo 0 || echo "1 (${i1_sum//$'\n'/|})")"

# (I2) DEFAULT UNCHANGED: no selector -> layer 7 inherits nothing and its line has no SUITE=.
d="$(new_stubdir)"; mk_l7_recorder "$d"
run_shard "$d" 7
assert_status "i2-default-no-selector" "<unset>" "$(cat "$LA_DT/ran.layer7" 2>/dev/null)"
assert_absent "i2-default-no-suite"    "SUITE=" "$(cat "$LA_DT/layer-summary.txt" 2>/dev/null)"
assert_absent "i2-default-banner"      "LAYER7_SUITE" "$(cat "$LA_ERR")"

# (I3) REFUSALS (exit 2, no layer runs): a FULL run (the Gate-2 authority), a --max-layer
#      fast lane, an unknown suite, and a shard that does not include layer 7.
d="$(new_stubdir)"
run_la "$d" DEVLOOP_TEST=1 LAYER_SCRIPT_DIR="$d" DEVLOOP_LAYER7_SUITE=env-tests
assert_exit      "i3-full-refused-exit2"  2 "$LA_RC"
assert_status    "i3-full-refused-token"  "REASON=layer7-suite-outside-shard" "$(cat "$LA_ERR")"
assert_no_marker "i3-full-no-stub"        "$LA_DT" 'ran.layer*'
run_la_args "$d" "--max-layer 6" DEVLOOP_TEST=1 LAYER_SCRIPT_DIR="$d" DEVLOOP_LAYER7_SUITE=browser-e2e
assert_exit      "i3-fast-refused-exit2"  2 "$LA_RC"
assert_status    "i3-fast-refused-token"  "REASON=layer7-suite-outside-shard" "$(cat "$LA_ERR")"
run_shard "$d" 7 DEVLOOP_LAYER7_SUITE=all
assert_exit      "i3-invalid-exit2"       2 "$LA_RC"
assert_status    "i3-invalid-token"       "REASON=layer7-suite-invalid" "$(cat "$LA_ERR")"
assert_no_marker "i3-invalid-no-stub"     "$LA_DT" 'ran.layer*'
run_shard "$d" 1,3 DEVLOOP_LAYER7_SUITE=env-tests
assert_exit      "i3-no-layer7-exit2"     2 "$LA_RC"
assert_status    "i3-no-layer7-token"     "REASON=layer7-suite-without-layer7" "$(cat "$LA_ERR")"
assert_no_marker "i3-no-layer7-no-stub"   "$LA_DT" 'ran.layer*'

# (I4) AGGREGATE over a split layer 7: worst status, max duration, both shards named.
# mk_split_partition <aggdir> <env-tests status> <env rc> <browser-e2e status> <browser rc>
mk_split_partition() {
  mk_summary "$1" guards "LAYER=1 RESULT=OK DURATION=1 RC=0" "LAYER=2 RESULT=OK DURATION=1 RC=0" \
    "LAYER=3 RESULT=OK DURATION=1 RC=0" "LAYER=6 RESULT=OK DURATION=1 RC=0"
  mk_summary "$1" test "LAYER=4 RESULT=OK DURATION=1 RC=0"
  mk_summary "$1" lint "LAYER=5 RESULT=OK DURATION=1 RC=0"
  mk_summary "$1" l7-browser "LAYER=7 RESULT=$4 DURATION=250 RC=$5 SUITE=browser-e2e"
  mk_summary "$1" l7-env "LAYER=7 RESULT=$2 DURATION=400 RC=$3 SUITE=env-tests"
}
a="$(mktemp -d "$WORK/agg.XXXXXX")"; mk_split_partition "$a" OK 0 OK 0; run_agg "$a"
assert_exit   "i4-split-green-exit0"    0 "$LA_RC"
assert_status "i4-split-cell"           "LAYER=7 RESULT=OK DURATION=400" "$(cat "$LA_OUT")"
assert_status "i4-split-shards-named"   "l7-browser+l7-env" "$(cat "$LA_OUT")"
a="$(mktemp -d "$WORK/agg.XXXXXX")"; mk_split_partition "$a" OK 0 FAIL 1; run_agg "$a"
assert_exit   "i4-split-one-red-exit1"  1 "$LA_RC"
assert_status "i4-split-one-red-cell"   "LAYER=7 RESULT=FAIL" "$(cat "$LA_OUT")"
assert_status "i4-split-one-red-triage" "FAILURE_TRIAGE LAYER=7 SHARD=l7-browser+l7-env" "$(cat "$LA_ERR")"
a="$(mktemp -d "$WORK/agg.XXXXXX")"; mk_split_partition "$a" PRECONDITION_FAILURE 2 OK 0; run_agg "$a"
assert_exit   "i4-split-operator-exit2" 2 "$LA_RC"
assert_status "i4-split-operator-cell"  "LAYER=7 RESULT=PRECONDITION_FAILURE" "$(cat "$LA_OUT")"

# (I5) Every malformed split is exit 2 with its own REASON — above all a DROPPED suite, the one
#      way a split could otherwise read as a green Layer 7.
a="$(mktemp -d "$WORK/agg.XXXXXX")"; mk_split_partition "$a" OK 0 OK 0; rm "$a/l7-browser/layer-summary.txt"; agg_case i5-missing-suite missing-suite
a="$(mktemp -d "$WORK/agg.XXXXXX")"; mk_split_partition "$a" OK 0 OK 0; mk_summary "$a" l7-env2 "LAYER=7 RESULT=OK DURATION=1 RC=0 SUITE=env-tests"; agg_case i5-duplicate-suite duplicate-suite
a="$(mktemp -d "$WORK/agg.XXXXXX")"; mk_split_partition "$a" OK 0 OK 0; mk_summary "$a" zzz-whole "LAYER=7 RESULT=OK DURATION=1 RC=0"; agg_case i5-whole-after-suites duplicate-layer
a="$(mktemp -d "$WORK/agg.XXXXXX")"; mk_split_partition "$a" OK 0 OK 0; mk_summary "$a" aaa-whole "LAYER=7 RESULT=OK DURATION=1 RC=0"; agg_case i5-suite-after-whole duplicate-layer
a="$(mktemp -d "$WORK/agg.XXXXXX")"; mk_split_partition "$a" OK 0 OK 0; mk_summary "$a" lint "LAYER=5 RESULT=OK DURATION=1 RC=0 SUITE=env-tests"; agg_case i5-suite-on-other-layer malformed-line
a="$(mktemp -d "$WORK/agg.XXXXXX")"; mk_split_partition "$a" OK 0 OK 0; mk_summary "$a" l7-browser "LAYER=7 RESULT=OK DURATION=1 RC=0 SUITE=browser"; agg_case i5-unknown-suite unknown-suite

# (I6) ROUND TRIP: real per-suite shards (stub layers) feed the aggregate, one per LAYER7_SUITES
#      entry — the shard writer and the split-aware parser agree.
d="$(new_stubdir)"; a="$(mktemp -d "$WORK/agg.XXXXXX")"
for part in guards:1,2,3,6 test:4 lint:5; do
  run_shard "$d" "${part#*:}"
  mkdir -p "$a/${part%%:*}"; cp "$LA_DT/layer-summary.txt" "$a/${part%%:*}/"
done
# The suite list is READ from its one home (lang/_common.sh) in a subshell — this suite does
# not source _common.sh (see the header) — so a new suite is round-tripped with no edit here.
mapfile -t i6_suites < <(bash -c 'source "$1/lang/_common.sh" && printf "%s\n" "${LAYER7_SUITES[@]}"' _ "$__here")
assert_exit "i6-suite-list-read" 0 "$(( ${#i6_suites[@]} >= 2 ? 0 : 1 ))"
for suite in "${i6_suites[@]}"; do
  run_shard "$d" 7 DEVLOOP_LAYER7_SUITE="$suite"
  mkdir -p "$a/l7-$suite"; cp "$LA_DT/layer-summary.txt" "$a/l7-$suite/"
done
run_agg "$a"
assert_exit   "i6-split-round-trip-exit0" 0 "$LA_RC"
assert_status "i6-split-round-trip-l7"    "LAYER=7 RESULT=OK" "$(cat "$LA_OUT")"

# (I7) SUBSET AGGREGATE — `--aggregate DIR --layers 7` (ci.yml's non-required Layer 7 check,
#      over the per-suite jobs only). Exactly the listed layers, every suite once; the full
#      aggregate (H, I4-I6) is unchanged.
run_agg_args() {  # run_agg_args <arg>...
  LA_OUT="$WORK/agg.out"; LA_ERR="$WORK/agg.err"
  env -i PATH="$PATH" HOME="$HOME" DEVLOOP_TMP="$WORK/agg.dt" bash "$LAYER_ALL" "$@" >"$LA_OUT" 2>"$LA_ERR"
  LA_RC=$?
}
mk_l7_only() {  # mk_l7_only <aggdir> <env-tests status> <env rc> <browser-e2e status> <browser rc>
  mk_summary "$1" l7-browser "LAYER=7 RESULT=$4 DURATION=250 RC=$5 SUITE=browser-e2e"
  mk_summary "$1" l7-env "LAYER=7 RESULT=$2 DURATION=400 RC=$3 SUITE=env-tests"
}
a="$(mktemp -d "$WORK/agg.XXXXXX")"; mk_l7_only "$a" OK 0 OK 0; run_agg_args --aggregate "$a" --layers 7
assert_exit   "i7-subset-green-exit0"   0 "$LA_RC"
assert_status "i7-subset-cell"          "LAYER=7 RESULT=OK DURATION=400" "$(cat "$LA_OUT")"
assert_absent "i7-subset-only-scope"    "LAYER=1 " "$(cat "$LA_OUT")"
run_agg_args --layers 7 --aggregate "$a"
assert_exit   "i7-subset-either-order"  0 "$LA_RC"
a="$(mktemp -d "$WORK/agg.XXXXXX")"; mk_l7_only "$a" OK 0 FAIL 1; run_agg_args --aggregate "$a" --layers 7
assert_exit   "i7-subset-red-exit1"     1 "$LA_RC"
assert_status "i7-subset-red-triage"    "FAILURE_TRIAGE LAYER=7 SHARD=l7-browser+l7-env" "$(cat "$LA_ERR")"
# A dropped suite, a layer outside the scope, and an empty scope dir are all exit 2.
subset_case() {  # subset_case <label> <reason>  (aggregate the dir in $a over --layers 7)
  run_agg_args --aggregate "$a" --layers 7
  assert_exit   "$1-exit2"  2 "$LA_RC"
  assert_status "$1-reason" "REASON=aggregate-$2" "$(cat "$LA_OUT")"
}
a="$(mktemp -d "$WORK/agg.XXXXXX")"; mk_l7_only "$a" OK 0 OK 0; rm "$a/l7-env/layer-summary.txt"; subset_case i7-subset-missing-suite missing-suite
a="$(mktemp -d "$WORK/agg.XXXXXX")"; mk_l7_only "$a" OK 0 OK 0; mk_summary "$a" test "LAYER=4 RESULT=OK DURATION=1 RC=0"; subset_case i7-subset-out-of-scope layer-out-of-scope
a="$(mktemp -d "$WORK/agg.XXXXXX")"; mk_summary "$a" test "LAYER=4 RESULT=OK DURATION=1 RC=0"; subset_case i7-subset-other-layer-only layer-out-of-scope
a="$(mktemp -d "$WORK/agg.XXXXXX")"; subset_case i7-subset-empty no-summaries
# The full aggregate still requires every layer: the Layer 7 jobs alone are not a pipeline.
a="$(mktemp -d "$WORK/agg.XXXXXX")"; mk_l7_only "$a" OK 0 OK 0; agg_case i7-full-needs-all-layers missing-layer
# Invalid lists, and --max-layer beside --aggregate, are still refused before reading anything.
run_agg_args --aggregate "$a" --layers 7,3
assert_exit   "i7-subset-bad-list-exit2"  2 "$LA_RC"
assert_status "i7-subset-bad-list-token"  "REASON=shard-layer-list-invalid" "$(cat "$LA_ERR")"
run_agg_args --aggregate "$a" --max-layer 3
assert_exit   "i7-max-with-aggregate-exit2" 2 "$LA_RC"
assert_status "i7-max-with-aggregate-says"  "mutually exclusive" "$(cat "$LA_ERR")"

# =============================================================================
# (K) KILLED / INCOMPLETE RUNS — a killed or incomplete layer-all must NEVER yield GATE2=PASS
#     (devloop 2026-11-21-killed-run-gate2-pass). REAL signals against the REAL orchestrator:
#     every run under test is launched in its own session with SIGINT at its default
#     (`setsid env --default-signal=INT`), so a group signal reaches only it and an INT is
#     deliverable; synchronisation is on a marker the blocking stub writes (never a sleep);
#     every wait is capped and fails BY NAME, then SIGKILLs the group so the suite never hangs.
# =============================================================================
env --default-signal=INT true 2>/dev/null || {
  printf 'layer-all.test.sh: PRECONDITION — env --default-signal (coreutils >= 9.1) is required to deliver SIGINT to a background run; the kill cases MUST NOT be skipped.\n' >&2
  exit 2
}
# Backstop: whatever an aborted case leaves behind (every blocking stub records "<layer-pid>
# <grandchild-pid>"; every launched run records its session id) is killed before $WORK goes.
__k_backstop() {
  local f p
  for f in "$WORK"/*/dt/l*.started "$WORK"/*/l*.started "$WORK"/k.pids; do
    [[ -f "$f" ]] || continue
    for p in $(tr ' \n' '\n\n' < "$f"); do
      [[ "$p" =~ ^[0-9]+$ ]] && { kill -KILL -- "-$p" 2>/dev/null; kill -KILL "$p" 2>/dev/null; }
    done
  done
  rm -rf "$WORK"
}
trap '__k_backstop' EXIT

# mk_block_stub <dir> <n> [ignore-term] — a layer that is a TREE (bash → sleep 120), prints
# its STATUS line, then records "<layer-pid> <grandchild-pid>" in $DEVLOOP_TMP/l<n>.started
# and blocks. ignore-term: the layer (and its sleeper) ignore SIGTERM — only KILL ends it.
mk_block_stub() {
  local ign=""; [[ "${3:-}" == "ignore-term" ]] && ign="trap '' TERM"
  cat > "$1/layer$2.sh" <<EOF2
#!/usr/bin/env bash
${ign}
printf 'STATUS=OK REASON=blocking-stub\n'
: > "\${DEVLOOP_TMP}/ran.layer$2"
sleep 120 & printf '%s %s\n' "\$\$" "\$!" > "\${DEVLOOP_TMP}/l$2.started"
wait
EOF2
  chmod +x "$1/layer$2.sh"
}

# k_start <stubdir> <dt> [layer-all args...] — launch layer-all in its own session (the
# session id IS LA_PID), env -i + the seam, into <dt>. Sets LA_PID/LA_DT/LA_OUT/LA_ERR.
# KV env for the run comes from the K_ENV array (default empty).
K_ENV=()
k_start() {
  local stubdir="$1"; LA_DT="$2"; shift 2
  local t; t="$(mktemp -d "$WORK/run.XXXXXX")"
  LA_OUT="$t/out"; LA_ERR="$t/err"
  mkdir -p "$LA_DT"
  setsid env --default-signal=INT env -i PATH="$PATH" HOME="$HOME" DEVLOOP_TMP="$LA_DT" \
      DEVLOOP_TEST=1 LAYER_SCRIPT_DIR="$stubdir" "${K_ENV[@]}" \
      bash "$LAYER_ALL" "$@" >"$LA_OUT" 2>"$LA_ERR" &
  LA_PID=$!
  printf '%s\n' "$LA_PID" >> "$WORK/k.pids"
}
# k_wait_marker <label> <file> — poll (30 s cap, 0.05 s steps) for the stub's marker.
k_wait_marker() {
  local i
  for (( i = 0; i < 600; i++ )); do [[ -s "$2" ]] && return 0; sleep 0.05; done
  FAIL=$((FAIL+1)); FAILURES+=("[$1] marker $2 never appeared (stub did not start)")
  kill -KILL -- "-$LA_PID" 2>/dev/null; { wait "$LA_PID"; } 2>/dev/null
  return 1
}
# k_wait_exit <label> [cap-secs] — the run must EXIT within the cap (default 20 s; a blocking
# stub sleeps 120 s, so a deferred trap or an unbounded teardown fails here BY NAME). Sets LA_RC.
k_wait_exit() {
  local i cap=$(( ${2:-20} * 20 ))
  for (( i = 0; i < cap; i++ )); do child_group_alive "$LA_PID" || break; sleep 0.05; done
  if child_group_alive "$LA_PID"; then
    FAIL=$((FAIL+1)); FAILURES+=("[$1-exit-prompt] layer-all still running ${2:-20}s after the signal")
    kill -KILL -- "-$LA_PID" 2>/dev/null
  else
    PASS=$((PASS+1))
  fi
  { wait "$LA_PID"; } 2>/dev/null; LA_RC=$?   # brace: also swallows bash's job-death notice
} 2>/dev/null   # bash prints a "Killed"/"User defined signal" job notice when it reaps
                # a signal-killed run; the deaths are the point of these cases, not noise.

# k_no_survivors <label> <pid...> — BEFORE any reaping by this harness: every recorded
# layer / grandchild pid must be gone within 10 s.
k_no_survivors() {
  local label="$1" i p alive; shift
  for (( i = 0; i < 200; i++ )); do
    alive=""; for p in "$@"; do child_group_alive "$p" && alive+=" $p"; done
    [[ -z "$alive" ]] && break; sleep 0.05
  done
  if [[ -z "$alive" ]]; then PASS=$((PASS+1)); else
    FAIL=$((FAIL+1)); FAILURES+=("[${label}] process(es)${alive} outlived layer-all")
    for p in $alive; do kill -KILL "$p" 2>/dev/null; done
  fi
}
kv() { gate2_verdict_get "$1" "$LA_DT/gate2-verdict"; }
# k_assert_killed <label> <rc> <signal> <missing> <in-flight> — the exact killed-run shape.
k_assert_killed() {
  local l="$1" rc="$2" sig="$3" missing="$4" inflight="$5" lae
  lae="$rc"; [[ "$sig" == none ]] && lae=2
  assert_exit   "$l-rc"            "$rc" "$LA_RC"
  assert_exit   "$l-verdict-exists" 0 "$([[ -f "$LA_DT/gate2-verdict" ]] && echo 0 || echo 1)"
  assert_exit   "$l-gate2-fail"    0 "$([[ "$(kv GATE2)" == FAIL ]] && echo 0 || echo "1 ($(kv GATE2))")"
  assert_exit   "$l-reason"        0 "$([[ "$(kv REASON)" == pipeline-incomplete ]] && echo 0 || echo "1 ($(kv REASON))")"
  assert_exit   "$l-signal"        0 "$([[ "$(kv SIGNAL)" == "$sig" ]] && echo 0 || echo "1 ($(kv SIGNAL))")"
  assert_exit   "$l-missing"       0 "$([[ "$(kv MISSING)" == "$missing" ]] && echo 0 || echo "1 ($(kv MISSING))")"
  assert_exit   "$l-layer-all-exit" 0 "$([[ "$(kv LAYER_ALL_EXIT)" == "$lae" ]] && echo 0 || echo "1 ($(kv LAYER_ALL_EXIT))")"
  assert_absent "$l-no-pass"       "GATE2=PASS" "$(cat "$LA_DT/gate2-verdict" 2>/dev/null)"
  assert_status "$l-incomplete-line" \
    "PIPELINE_INCOMPLETE MISSING=${missing} IN_FLIGHT=${inflight} SIGNAL=${sig} LAYER_ALL_EXIT=${lae}" "$(cat "$LA_ERR")"
}
# k_case <label> <block-layer> <signal> <pid|group> <rc> — one full killed-run case.
k_case() {
  local l="$1" n="$2" sig="$3" target="$4" rc="$5" d dt missing="" m lp gp
  d="$(new_stubdir)"; mk_block_stub "$d" "$n"
  dt="$(mktemp -d "$WORK/run.XXXXXX")/dt"
  K_ENV=(DEVLOOP_FAIL_FAST=0); k_start "$d" "$dt"; K_ENV=()
  k_wait_marker "$l" "$dt/l${n}.started" || return
  IFS=' ' read -r lp gp < "$dt/l${n}.started"
  if [[ "$target" == group ]]; then kill "-$sig" -- "-$LA_PID"; else kill "-$sig" "$LA_PID"; fi
  k_wait_exit "$l"
  k_no_survivors "$l-no-surviving-layer" "$lp" "$gp"
  for (( m = n; m <= LAYER_MAX; m++ )); do missing="${missing:+${missing},}${m}"; done
  local s="$sig"; [[ "$sig" == USR1 ]] && s=none
  k_assert_killed "$l" "$rc" "$s" "$missing" "$n"
  assert_no_marker "$l-later-layer-never-ran" "$dt" "ran.layer$((n+1))"
  kill -KILL -- "-$LA_PID" 2>/dev/null
}

# K1-K6 — the incident (killed mid-Layer-7), every trapped signal, to the pid and to the group.
k_case k1-term-pid   "$LAYER_MAX" TERM pid   143
k_case k2-term-group "$LAYER_MAX" TERM group 143
k_case k3-int-pid    "$LAYER_MAX" INT  pid   130
k_case k4-int-group  "$LAYER_MAX" INT  group 130
k_case k5-hup-pid    "$LAYER_MAX" HUP  pid   129
k_case k6-hup-group  "$LAYER_MAX" HUP  group 129
# K7 — not Layer-7-specific: killed mid-Layer-3 → MISSING=3..LAYER_MAX, layer 4 never ran.
k_case k7-term-mid-layer3 3 TERM pid 143
# K8 — an UNTRAPPED default-fatal signal (SIGUSR1): bash runs the EXIT trap with `$?`=0, so
#      only "an unrecorded layer is UNKNOWN" (derive_layer_all_exit) keeps the verdict FAIL.
#      The isolating test for the completeness rule: revert that rule alone and this reds.
k_case k8-usr1-untrapped 5 USR1 pid 138

# K9 — an UNCATCHABLE SIGKILL of layer-all alone: the run-start verdict (REASON=in-progress)
#      replaced the earlier PASS, the orphaned layer survives it, and the NEXT run takes the
#      lock (the lock fd never reached the layer), reaps the orphan by identity, and passes.
d_ok="$(new_stubdir)"; d_blk="$(new_stubdir)"; mk_block_stub "$d_blk" 4
k9dt="$(mktemp -d "$WORK/run.XXXXXX")/dt"
k_start "$d_ok" "$k9dt"; k_wait_exit k9-control
assert_exit "k9-control-pass" 0 "$([[ "$(kv GATE2)" == PASS && "$(kv REASON)" == none ]] && echo 0 || echo "1 ($(kv GATE2) $(kv REASON))")"
cp "$k9dt/gate2-verdict" "$WORK/k9.pass-copy"
k_start "$d_blk" "$k9dt"
if k_wait_marker k9 "$k9dt/l4.started"; then
  IFS=' ' read -r k9lp k9gp < "$k9dt/l4.started"
  kill -KILL "$LA_PID"; k_wait_exit k9-kill
  assert_exit   "k9-rc137"            137 "$LA_RC"
  assert_exit   "k9-gate2-fail"       0 "$([[ "$(kv GATE2)" == FAIL ]] && echo 0 || echo "1 ($(kv GATE2))")"
  assert_exit   "k9-reason-in-progress" 0 "$([[ "$(kv REASON)" == in-progress ]] && echo 0 || echo "1 ($(kv REASON))")"
  assert_exit   "k9-not-the-old-pass" 0 "$(cmp -s "$k9dt/gate2-verdict" "$WORK/k9.pass-copy" && echo 1 || echo 0)"
  assert_absent "k9-no-recorded-layer" "LAYER 1 OK" "$(cat "$k9dt/gate2-verdict")"
  assert_exit   "k9-orphan-survived-kill" 0 "$(child_group_alive "$k9lp" && echo 0 || echo 1)"
  k_start "$d_ok" "$k9dt"; k_wait_exit k9-next
  assert_exit   "k9-next-exit0"        0 "$LA_RC"
  assert_absent "k9-next-not-refused"  "concurrent-layer-all" "$(cat "$LA_ERR")"
  assert_status "k9-next-reaped"       "REAPED_STALE_LAYER_GROUP PGID=" "$(cat "$LA_ERR")"
  assert_status "k9-next-reaped-layer" " LAYER=4 " "$(grep REAPED_STALE_LAYER_GROUP "$LA_ERR")"
  k_no_survivors "k9-orphan-reaped" "$k9lp" "$k9gp"
  assert_exit   "k9-next-pass"         0 "$([[ "$(kv GATE2)" == PASS ]] && echo 0 || echo "1 ($(kv GATE2))")"
fi

# K10 — a stale record whose leader identity does NOT match (a live, unrelated process now
#       holds that pid with another start time) is dropped and NEVER signalled.
d="$(new_stubdir)"; k10dt="$(mktemp -d "$WORK/run.XXXXXX")/dt"; mkdir -p "$k10dt"
setsid sleep 120 & k10sl=$!; printf '%s\n' "$k10sl" >> "$WORK/k.pids"
printf 'OWNER_PID=1\nOWNER_START=1\nLAYER=6\nPGID=%s\nLEADER_START=1\nSCRIPT=/nonexistent/layer6.sh\nRECORDED_AT=2026-01-01T00:00:00Z\n' "$k10sl" > "$k10dt/layer-all.inflight"
k_start "$d" "$k10dt"; k_wait_exit k10
assert_status "k10-dropped"        "STALE_LAYER_RECORD_DROPPED PGID=${k10sl} LAYER=6 REASON=identity-mismatch" "$(cat "$LA_ERR")"
assert_absent "k10-not-reaped"     "REAPED_STALE_LAYER_GROUP" "$(cat "$LA_ERR")"
assert_exit   "k10-not-signalled"  0 "$(child_group_alive "$k10sl" && echo 0 || echo 1)"
assert_exit   "k10-record-removed" 0 "$([[ ! -e "$k10dt/layer-all.inflight" ]] && echo 0 || echo 1)"
kill -KILL "$k10sl" 2>/dev/null; wait "$k10sl" 2>/dev/null

# K11 — a CONCURRENT run in the same DEVLOOP_TMP is refused before touching anything: run A
#       blocks in layer 3; run B exits 2 concurrent-layer-all; A's snapshot, verdict and
#       in-flight layer are untouched (security C4 — no cross-read of A's start snapshot).
d_blk="$(new_stubdir)"; mk_block_stub "$d_blk" 3; k11dt="$(mktemp -d "$WORK/run.XXXXXX")/dt"
k_start "$d_blk" "$k11dt"; k11a=$LA_PID; k11a_err="$LA_ERR"
if k_wait_marker k11 "$k11dt/l3.started"; then
  IFS=' ' read -r k11lp _ < "$k11dt/l3.started"
  k11sig="$(cat "$k11dt/gate2-snapshot/start/signature")"; k11ver="$(cat "$k11dt/gate2-verdict")"
  k_start "$(new_stubdir)" "$k11dt"; k_wait_exit k11-b
  assert_exit   "k11-b-refused-exit2" 2 "$LA_RC"
  assert_status "k11-b-token"         "REASON=concurrent-layer-all" "$(cat "$LA_ERR")"
  assert_status "k11-b-names-lock"    "layer-all.lock" "$(cat "$LA_ERR")"
  assert_exit   "k11-a-snapshot-untouched" 0 "$([[ "$(cat "$k11dt/gate2-snapshot/start/signature" 2>/dev/null)" == "$k11sig" ]] && echo 0 || echo 1)"
  assert_exit   "k11-a-verdict-untouched"  0 "$([[ "$(cat "$k11dt/gate2-verdict")" == "$k11ver" ]] && echo 0 || echo 1)"
  assert_exit   "k11-a-layer-alive"   0 "$(child_group_alive "$k11lp" && echo 0 || echo 1)"
  LA_PID=$k11a; LA_ERR="$k11a_err"; kill -TERM "$LA_PID"; k_wait_exit k11-a
fi

# K12 — a layer that IGNORES SIGTERM is still torn down (bounded TERM→KILL) and the run
#       exits promptly with nothing left behind.
d="$(new_stubdir)"; mk_block_stub "$d" 6 ignore-term; k12dt="$(mktemp -d "$WORK/run.XXXXXX")/dt"
k_start "$d" "$k12dt"
if k_wait_marker k12 "$k12dt/l6.started"; then
  IFS=' ' read -r k12lp k12gp < "$k12dt/l6.started"
  kill -TERM "$LA_PID"; k_wait_exit k12
  assert_exit "k12-rc143" 143 "$LA_RC"
  k_no_survivors "k12-no-surviving-layer" "$k12lp" "$k12gp"
fi

# K13 — NESTED grace race (ops B3(b)): an outer run-story-style teardown (kill_child_group,
#       the FULL grace) hits layer-all's session while its layer ignores TERM. layer-all's
#       inner teardown (CHILD_GROUP_INNER_POLLS, derived from the same constant) must KILL the
#       layer before the outer KILL lands on layer-all, so nothing survives.
assert_exit "k13-inner-below-outer" 0 "$(( CHILD_GROUP_INNER_POLLS < CHILD_GROUP_GRACE_POLLS ? 0 : 1 ))"
d="$(new_stubdir)"; mk_block_stub "$d" 6 ignore-term; k13dt="$(mktemp -d "$WORK/run.XXXXXX")/dt"
k_start "$d" "$k13dt"
if k_wait_marker k13 "$k13dt/l6.started"; then
  IFS=' ' read -r k13lp k13gp < "$k13dt/l6.started"
  kill_child_group "$CHILD_GROUP_GRACE_POLLS" "$LA_PID"
  k_no_survivors "k13-no-surviving-layer" "$k13lp" "$k13gp"
fi

# K14 — a layer KILLED ON ITS OWN (the reaper picks one process) while layer-all carries on:
#       a real _common.sh lifecycle layer that recorded one passing child renders UNKNOWN
#       layer-incomplete in its own log, the summary row and the verdict's LAYER line (never
#       OK), the run is FAIL, and the per-layer session reap takes its grandchild.
d="$(new_stubdir)"
cat > "$d/layer3.sh" <<EOF2
#!/usr/bin/env bash
set -euo pipefail
source "${__here}/lang/_common.sh"
layer_lifecycle_begin 3
printf 'STATUS=OK REASON=first-child\n' | tee_collect_statuses
# The straggler does NOT hold the layer's stdout (like a port-forward logging to a file): a
# child that still holds the pipe keeps tee — and so the layer's wait — open until it exits.
sleep 120 >/dev/null 2>&1 & printf '%s %s\n' "\$\$" "\$!" > "\${DEVLOOP_TMP}/l3.started"
wait
layer_lifecycle_done
EOF2
chmod +x "$d/layer3.sh"
k14dt="$(mktemp -d "$WORK/run.XXXXXX")/dt"
K_ENV=(DEVLOOP_FAIL_FAST=0); k_start "$d" "$k14dt"; K_ENV=()
if k_wait_marker k14 "$k14dt/l3.started"; then
  IFS=' ' read -r k14lp k14gp < "$k14dt/l3.started"
  kill -TERM "$k14lp"; k_wait_exit k14
  assert_exit   "k14-run-red"        143 "$LA_RC"   # rc floor: the layer died of SIGTERM
  assert_status "k14-layer-log"      "STATUS=UNKNOWN REASON=layer-incomplete" "$(tail -n1 "$k14dt/layer-3.log")"
  assert_status "k14-summary-row"    "LAYER=3 RESULT=UNKNOWN" "$(cat "$LA_OUT")"
  assert_exit   "k14-verdict-line"   0 "$(grep -qx 'LAYER 3 UNKNOWN [0-9]*' "$k14dt/gate2-verdict" && echo 0 || echo 1)"
  assert_exit   "k14-gate2-fail"     0 "$([[ "$(kv GATE2)" == FAIL && "$(kv REASON)" == layer-failed ]] && echo 0 || echo "1 ($(kv GATE2) $(kv REASON))")"
  assert_marker "k14-pipeline-continued" "$k14dt" 'ran.layer4'
  k_no_survivors "k14-grandchild-reaped" "$k14gp"
fi

# K21 — layer-all KILLED while a real _common.sh lifecycle layer is in flight (one passing
#       child recorded): the stderr PIPELINE_INCOMPLETE line AND the layer's own log agree —
#       the log's LAST STATUS line is the lifecycle's `UNKNOWN layer-incomplete`, never the
#       finished child's OK (tee ignores the teardown TERM so the layer's last words land).
d="$(new_stubdir)"
cat > "$d/layer3.sh" <<EOF2
#!/usr/bin/env bash
set -euo pipefail
source "${__here}/lang/_common.sh"
layer_lifecycle_begin 3
printf 'STATUS=OK REASON=first-child\n' | tee_collect_statuses
sleep 120 >/dev/null 2>&1 & printf '%s %s\n' "\$\$" "\$!" > "\${DEVLOOP_TMP}/l3.started"
wait
layer_lifecycle_done
EOF2
chmod +x "$d/layer3.sh"
k21dt="$(mktemp -d "$WORK/run.XXXXXX")/dt"
K_ENV=(DEVLOOP_FAIL_FAST=0); k_start "$d" "$k21dt"; K_ENV=()
if k_wait_marker k21 "$k21dt/l3.started"; then
  IFS=' ' read -r k21lp k21gp < "$k21dt/l3.started"
  kill -TERM "$LA_PID"; k_wait_exit k21
  k_no_survivors "k21-no-surviving-layer" "$k21lp" "$k21gp"
  assert_status "k21-incomplete-line" "PIPELINE_INCOMPLETE MISSING=3,4,5,6,7 IN_FLIGHT=3 SIGNAL=TERM LAYER_ALL_EXIT=143" "$(cat "$LA_ERR")"
  assert_exit   "k21-layer-log-last-status" 0 "$([[ "$(grep '^STATUS=' "$k21dt/layer-3.log" | tail -n1)" == "STATUS=UNKNOWN REASON=layer-incomplete" ]] && echo 0 || echo "1 ($(tr '\n' '|' < "$k21dt/layer-3.log"))")"
fi

# K22 — a `set -e` abort AFTER the loop (every layer OK; stdout is /dev/full, so the summary
#       printf fails) is pipeline-incomplete — completion is only the final `exit` — never
#       layer-failed, and the stderr line agrees with the verdict.
d="$(new_stubdir)"; LA_DT="$(mktemp -d "$WORK/run.XXXXXX")/dt"; LA_ERR="$LA_DT.err"
env -i PATH="$PATH" HOME="$HOME" DEVLOOP_TMP="$LA_DT" DEVLOOP_TEST=1 LAYER_SCRIPT_DIR="$d" \
    bash "$LAYER_ALL" >/dev/full 2>"$LA_ERR"; LA_RC=$?
assert_exit   "k22-post-loop-abort-nonzero" 0 "$(( LA_RC != 0 ? 0 : 1 ))"
assert_exit   "k22-reason" 0 "$([[ "$(kv GATE2)" == FAIL && "$(kv REASON)" == pipeline-incomplete && -z "$(kv MISSING)" ]] && echo 0 || echo "1 ($(kv GATE2) $(kv REASON) $(kv MISSING))")"
assert_status "k22-line" "PIPELINE_INCOMPLETE MISSING=none IN_FLIGHT=none SIGNAL=none LAYER_ALL_EXIT=${LA_RC}" "$(cat "$LA_ERR")"

# K23 — the recorded LEADER died too (a tree-walking reaper got it and layer-all), but its
#       session's members live on: the next run reaps them GROUP-ONLY (a pgid number stays
#       allocated while its group lives) and says so; the leader's bare pid is never used.
d_blk="$(new_stubdir)"; mk_block_stub "$d_blk" 4; k23dt="$(mktemp -d "$WORK/run.XXXXXX")/dt"
k_start "$d_blk" "$k23dt"
if k_wait_marker k23 "$k23dt/l4.started"; then
  IFS=' ' read -r k23lp k23gp < "$k23dt/l4.started"
  k23pg="$(sed -n 's/^PGID=//p' "$k23dt/layer-all.inflight")"
  kill -KILL "$LA_PID" "$k23pg"; k_wait_exit k23-kill
  assert_exit "k23-members-survived" 0 "$(child_group_alive "$k23gp" && echo 0 || echo 1)"
  k_start "$(new_stubdir)" "$k23dt"; k_wait_exit k23-next
  assert_status "k23-reaped-leader-gone" "REAPED_STALE_LAYER_GROUP PGID=${k23pg} LAYER=4 " "$(cat "$LA_ERR")"
  assert_status "k23-reason" "REASON=leader-gone" "$(grep REAPED_STALE_LAYER_GROUP "$LA_ERR")"
  k_no_survivors "k23-members-reaped" "$k23lp" "$k23gp"
fi

# K24 — kill_child_group group-only NEVER signals the bare pid (the post-wait reap and the
#       stale reap use it: their leader may be reaped and its pid recycled). A live process
#       that is NOT a group leader stands in for the recycled pid: it must survive.
sleep 120 & k24=$!; printf '%s\n' "$k24" >> "$WORK/k.pids"
kill_child_group 1 "$k24" group-only
assert_exit "k24-group-only-spares-bare-pid" 0 "$(child_group_alive "$k24" && echo 0 || echo 1)"
kill -KILL "$k24" 2>/dev/null; wait "$k24" 2>/dev/null

# K25 — a signal DURING the post-wait reap (the layer exited, a TERM-ignoring straggler is
#       still in its session): the trap tears the group down GROUP-ONLY (the leader was just
#       reaped — its bare pid may be recycled) and the straggler is KILLed within the bound.
d="$(new_stubdir)"
cat > "$d/layer5.sh" <<'EOF2'
#!/usr/bin/env bash
printf 'STATUS=OK REASON=leaves-straggler\n'
( trap '' TERM; exec sleep 120 ) >/dev/null 2>&1 &
printf '%s %s\n' "$$" "$!" > "${DEVLOOP_TMP}/l5.started"
exit 0
EOF2
chmod +x "$d/layer5.sh"
k25dt="$(mktemp -d "$WORK/run.XXXXXX")/dt"
k_start "$d" "$k25dt"
if k_wait_marker k25 "$k25dt/l5.started"; then
  IFS=' ' read -r _ k25gp < "$k25dt/l5.started"
  sleep 0.5   # the layer has exited; layer-all is inside the post-wait reap's grace polls
  kill -TERM "$LA_PID"; k_wait_exit k25
  assert_exit "k25-rc143" 143 "$LA_RC"
  k_no_survivors "k25-straggler-killed" "$k25gp"
fi
# Structural pin: the trap's teardown is group-only once `wait` reaped the leader.
assert_status "k25-trap-group-only-after-reap" '(( __inflight_reaped )) && mode="group-only"' "$(cat "$LAYER_ALL")"

# K26 — the teardown GRANTS the grace: a real _common.sh lifecycle layer whose registered
#       cleanup takes ~1 s (a port-forward stop, a helper shutdown) finishes it — and its
#       `STATUS=UNKNOWN REASON=layer-incomplete` line lands — before any KILL, because the
#       teardown waits on the layer's whole SESSION, not on the wrapper that leads it.
d="$(new_stubdir)"
cat > "$d/layer3.sh" <<EOF2
#!/usr/bin/env bash
set -euo pipefail
source "${__here}/lang/_common.sh"
layer_lifecycle_begin 3
slow_cleanup() { sleep 1; : > "\${DEVLOOP_TMP}/l3.cleanup-done"; }
layer_register_cleanup slow_cleanup
printf 'STATUS=OK REASON=first-child\n' | tee_collect_statuses
sleep 120 >/dev/null 2>&1 & printf '%s %s\n' "\$\$" "\$!" > "\${DEVLOOP_TMP}/l3.started"
wait
layer_lifecycle_done
EOF2
chmod +x "$d/layer3.sh"
k26dt="$(mktemp -d "$WORK/run.XXXXXX")/dt"
k_start "$d" "$k26dt"
if k_wait_marker k26 "$k26dt/l3.started"; then
  IFS=' ' read -r k26lp k26gp < "$k26dt/l3.started"
  kill -TERM "$LA_PID"; k_wait_exit k26
  assert_exit   "k26-rc143" 143 "$LA_RC"
  assert_marker "k26-cleanup-ran-before-kill" "$k26dt" 'l3.cleanup-done'
  assert_exit   "k26-incomplete-line-landed" 0 "$([[ "$(grep '^STATUS=' "$k26dt/layer-3.log" | tail -n1)" == "STATUS=UNKNOWN REASON=layer-incomplete" ]] && echo 0 || echo "1 ($(tr '\n' '|' < "$k26dt/layer-3.log"))")"
  k_no_survivors "k26-no-surviving-layer" "$k26lp" "$k26gp"
fi

# K15 — a killed SHARD and a killed FAST LANE never write, remove or overwrite a verdict
#       (and never take a snapshot), and still leave nothing behind.
for k15 in "shard --layers 3" "fast --max-layer 6"; do
  k15l="${k15%% *}"; k15args="${k15#* }"
  d="$(new_stubdir)"; mk_block_stub "$d" 3; k15dt="$(mktemp -d "$WORK/run.XXXXXX")/dt"
  mkdir -p "$k15dt"; printf 'sentinel-verdict\n' > "$k15dt/gate2-verdict"
  IFS=' ' read -r -a k15a <<< "$k15args"
  k_start "$d" "$k15dt" "${k15a[@]}"
  if k_wait_marker "k15-$k15l" "$k15dt/l3.started"; then
    IFS=' ' read -r k15lp k15gp < "$k15dt/l3.started"
    kill -TERM "$LA_PID"; k_wait_exit "k15-$k15l"
    assert_exit "k15-$k15l-rc143"            143 "$LA_RC"
    assert_exit "k15-$k15l-verdict-untouched" 0 "$([[ "$(cat "$k15dt/gate2-verdict")" == sentinel-verdict ]] && echo 0 || echo 1)"
    assert_no_marker "k15-$k15l-no-snapshot" "$k15dt" 'gate2-snapshot'
    k_no_survivors "k15-$k15l-no-surviving-layer" "$k15lp" "$k15gp"
  fi
done

# K16 — the run-start snapshot cannot be taken (cwd is not a git work tree): refused exit 2
#       BEFORE Layer 1, and the PREVIOUS verdict (a pre-seeded PASS) is gone, not left behind.
d="$(new_stubdir)"; k16cwd="$(mktemp -d "$WORK/nogit.XXXXXX")"; k16dt="$(mktemp -d "$WORK/run.XXXXXX")/dt"
mkdir -p "$k16dt"; printf 'SCHEMA=gate2-verdict/v2\nGATE2=PASS\n' > "$k16dt/gate2-verdict"
( cd "$k16cwd" && env -i PATH="$PATH" HOME="$HOME" GIT_CEILING_DIRECTORIES="$WORK" DEVLOOP_TMP="$k16dt" \
    DEVLOOP_TEST=1 LAYER_SCRIPT_DIR="$d" bash "$LAYER_ALL" >"$k16cwd.out" 2>"$k16cwd.err" ); k16rc=$?
assert_exit      "k16-snapshot-failed-exit2" 2 "$k16rc"
assert_status    "k16-token"                 "REASON=gate2-snapshot-failed" "$(cat "$k16cwd.err")"
assert_exit      "k16-old-pass-removed"      0 "$([[ ! -e "$k16dt/gate2-verdict" ]] && echo 0 || echo 1)"
assert_no_marker "k16-no-stub-ran"           "$k16dt" 'ran.layer*'

# K17 — the previous verdict cannot be replaced (a non-empty DIRECTORY squats on the path;
#       rm -f fails on it even as root): refused exit 2, nothing runs.
d="$(new_stubdir)"; k17dt="$(mktemp -d "$WORK/run.XXXXXX")/dt"
mkdir -p "$k17dt/gate2-verdict/sub"
K_ENV=(); k_start "$d" "$k17dt"; k_wait_exit k17
assert_exit      "k17-invalidate-failed-exit2" 2 "$LA_RC"
assert_status    "k17-token"   "REASON=gate2-verdict-invalidate-failed" "$(cat "$LA_ERR")"
assert_no_marker "k17-no-stub-ran" "$k17dt" 'ran.layer*'

# K18 — the TREE must be the same at exit as at start. A hermetic git fixture (git init, a
#       synthetic origin/main) so stubs can edit it; the real layer-all runs with cwd there.
k18_fixture() {
  local r; r="$(mktemp -d "$WORK/repo.XXXXXX")"
  git -C "$r" -c init.defaultBranch=main init -q
  printf 'tracked\n' > "$r/a.txt"
  git -C "$r" add a.txt
  git -C "$r" -c user.name=t -c user.email=t@example.com -c commit.gpgsign=false commit -qm seed
  git -C "$r" update-ref refs/remotes/origin/main HEAD
  printf '%s\n' "$r"
}
# k18_run <label> <stub-layer-3-body> — run every layer OK except a custom layer 3.
k18_run() {
  local r d; r="$(k18_fixture)"; d="$(new_stubdir)"
  printf '#!/usr/bin/env bash\n%s\n: > "${DEVLOOP_TMP}/ran.layer3"\n' "$2" > "$d/layer3.sh"; chmod +x "$d/layer3.sh"
  LA_DT="$(mktemp -d "$WORK/run.XXXXXX")/dt"; LA_OUT="$LA_DT.out"; LA_ERR="$LA_DT.err"
  ( cd "$r" && env -i PATH="$PATH" HOME="$HOME" GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 \
      DEVLOOP_TMP="$LA_DT" DEVLOOP_TEST=1 DEVLOOP_FAIL_FAST=0 LAYER_SCRIPT_DIR="$d" \
      bash "$LAYER_ALL" >"$LA_OUT" 2>"$LA_ERR" ); LA_RC=$?
}
k18_run t2-control "printf 'STATUS=OK REASON=ok\n'"
assert_exit "k18-t2-control-pass" 0 "$([[ "$(kv GATE2)" == PASS && "$(kv REASON)" == none ]] && echo 0 || echo "1 ($(kv GATE2) $(kv REASON) rc=$LA_RC)")"
k18_run t1-edit "printf 'STATUS=OK REASON=ok\n'; printf 'edited\n' >> a.txt"
assert_exit   "k18-t1-exit0"  0 "$LA_RC"
assert_exit   "k18-t1-fail"   0 "$([[ "$(kv GATE2)" == FAIL && "$(kv REASON)" == tree-changed-during-run ]] && echo 0 || echo "1 ($(kv GATE2) $(kv REASON))")"
assert_status "k18-t1-warn"   "WARN GATE2_TREE_CHANGED_DURING_RUN PATHS=a.txt" "$(cat "$LA_ERR")"
k18_run t3-untracked "printf 'STATUS=OK REASON=ok\n'; printf 'new\n' > b.txt"
assert_exit   "k18-t3-fail"   0 "$([[ "$(kv REASON)" == tree-changed-during-run ]] && echo 0 || echo "1 ($(kv REASON))")"
assert_status "k18-t3-warn"   "PATHS=b.txt" "$(cat "$LA_ERR")"
# T4 — two causes at once (a failed layer that also edits): the precedence puts layer-failed first.
k18_run t4-two-causes "printf 'STATUS=FAIL REASON=bad\n'; printf 'edited\n' >> a.txt; exit 1"
assert_exit "k18-t4-precedence" 0 "$([[ "$(kv REASON)" == layer-failed ]] && echo 0 || echo "1 ($(kv REASON))")"

# K19 — CONTROLS: a fail-fast stop is a COMPLETE run (layer-failed, no PIPELINE_INCOMPLETE,
#       the un-run layers render NOT-RUN); a green run is PASS with REASON=none.
d="$(new_stubdir)"; mk_stub "$d" 3 FAIL boom 1
run_la "$d" DEVLOOP_TEST=1 LAYER_SCRIPT_DIR="$d" DEVLOOP_FAIL_FAST=1
assert_exit   "k19-failfast-reason"  0 "$([[ "$(kv GATE2)" == FAIL && "$(kv REASON)" == layer-failed ]] && echo 0 || echo "1 ($(kv GATE2) $(kv REASON))")"
assert_absent "k19-failfast-no-incomplete" "PIPELINE_INCOMPLETE" "$(cat "$LA_ERR")"
assert_exit   "k19-failfast-not-run-line" 0 "$(grep -qx 'LAYER 5 NOT-RUN 0' "$LA_DT/gate2-verdict" && echo 0 || echo 1)"
d="$(new_stubdir)"; run_la "$d" DEVLOOP_TEST=1 LAYER_SCRIPT_DIR="$d"
assert_exit   "k19-green-pass" 0 "$([[ "$(kv GATE2)" == PASS && "$(kv REASON)" == none && "$(kv SIGNAL)" == none && -z "$(kv MISSING)" ]] && echo 0 || echo 1)"
assert_absent "k19-green-no-incomplete" "PIPELINE_INCOMPLETE" "$(cat "$LA_ERR")"

# K20 — LAYER_MAX has exactly ONE home (scripts/lang/_layers.sh). Production files only
#       (*.test.sh excluded: a test may legitimately set `LAYER_MAX=3 cmd` inline); the anchor
#       is an assignment at line start, so a `${LAYER_MAX}` read or a comment never counts.
k20="$(grep -rlE '^[[:space:]]*(readonly[[:space:]]+|export[[:space:]]+)?LAYER_MAX=' "${__here}" --include='*.sh' \
        | grep -v '\.test\.sh$' | sort)"
assert_exit "k20-layer-max-single-home" 0 "$([[ "$k20" == "${__here}/lang/_layers.sh" ]] && echo 0 || echo "1 (${k20//$'\n'/,})")"

report_results "scripts/layer-all.test.sh"
