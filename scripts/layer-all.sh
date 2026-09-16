#!/usr/bin/env bash
# layer-all.sh — orchestrator for the polyglot validation pipeline (ADR-0033 §4).
#
# Runs scripts/layer1.sh through scripts/layer7.sh sequentially. Captures each
# layer's stdout to ${DEVLOOP_TMP}/layer-N.log and stderr to layer-N.stderr.log.
# Emits per-layer budget warnings, machine-parseable LAYER_SUMMARY block + total,
# and a human-readable summary table.
#
# Budget (ADR-0033 §4 + paired-operations §2):
#   Per-layer warn threshold: ${DEVLOOP_LAYER_BUDGET_SECS:-20}s (warn-only).
#   Guard+audit fast tier (layers 3 + 6): hard 90s p95.
#
# NOTE (2026-08-20): since the language short-circuit was retired, ALL of layers 1-6
# always-run. The 90s p95 budget deliberately covers ONLY the cheap "fast floor" —
# layers 3 (guards) + 6 (audit) — NOT the language layers 1/2/4/5 (compile/fmt/test/lint),
# whose cost is inherently large/variable and was never in a p95 latency budget (the same
# reason layer 7's ~10-15min env-test envelope is excluded). "Always-run" is now the whole
# pipeline; this budget is scoped to the fast-floor subset by design, not by accident.
#
# Greppable warn tokens (paired-operations §2):
#   WARN BUDGET_BREACH LAYER=<n> DURATION=<s> BUDGET=<s>
#   WARN BUDGET_TOTAL_BREACH GUARD_AUDIT_DURATION=<s> BUDGET=90
#
# Failure triage: docs/runbooks/devloop-validation.md (all layers + §4 two-token convention).

set -euo pipefail
IFS=$'\n\t'

__here="$(cd "$(dirname "$0")" && pwd)"
source "${__here}/lang/_common.sh"
# Gate-2 authority gate (task #51): shared producer/hook library. Provides
# emit_gate2_verdict + the binding/exclusion/signature/slug primitives. Self-
# contained (no _common.sh dependency); sourced here for the producer side.
source "${__here}/lang/_gate2_binding.sh"
init_devloop_tmp

# Runtime discoverability for the memory cap set in lang/_common.sh: announce the
# effective cargo job budget and its escape hatch once at pipeline start, so anyone
# watching a run sees why parallelism is limited and how to change it.
printf 'CARGO_BUILD_JOBS=%s (capped for memory; export CARGO_BUILD_JOBS=N or pass --jobs to override)\n' \
  "${CARGO_BUILD_JOBS:-<unset>}" >&2

# Per-layer result/duration accumulators — declared BEFORE the EXIT trap installs
# so emit_gate2_verdict's namerefs always bind to existing (possibly-empty) arrays,
# even on an early exit that fires before the layer loop populates them.
declare -a layer_status layer_dur
final_exit=0
stopped_early=0   # >0 = the layer at which interactive fail-fast stopped the run (0 = ran all)

# NOT_RUN — DISPLAY-ONLY marker for layers that never ran under interactive fail-fast.
# GUARDRAIL (@security): NOT_RUN must NEVER be added to _common.sh __status_rank or given an
# arm in status_to_exit_code. It is deliberately kept OUT of aggregation: the aggregation loop
# `continue`s past it and status_to_exit_code is never called on it, so an un-run layer can
# neither demote nor inflate TOTAL_RESULT / LAYER_ALL_EXIT. The `*)` fail-closed backstops in
# __status_rank (rank 8) and status_to_exit_code (exit 2) make an accidental leak LOUD — but a
# well-meaning `NOT-RUN → 0` arm would silently reopen the fail-fast GATE2=PASS-forgery vector
# (a stopped-early run exiting 0 over layers that never ran). Do not "complete the enum".
readonly NOT_RUN="NOT-RUN"

# Gate-2 producer: emit the tree-bound verdict as the pipeline's FINAL step via an
# EXIT trap (task #51; design main.md §Design point 1). Installing it HERE — before
# the sentinel/precondition checks below — guarantees a verdict file even when those
# early-exit (GATE2=FAIL + LAYER_ALL_EXIT=<rc>), so a MISSING file means exclusively
# "layer-all.sh was never invoked" (the authority skip-vector the hook catches).
#
# Capture the real exit code FIRST so a git/sha256 error inside the emitter cannot
# mutate it; restore the default trap and re-exit with the captured code. Emit
# failures are logged but never change the pipeline's verdict exit.
__gate2_emit_trap() {
  local __rc=$?
  trap - EXIT
  if ! emit_gate2_verdict "$__rc" layer_status layer_dur 2>>"${DEVLOOP_TMP}/gate2-emit.stderr.log"; then
    printf 'WARN gate2: verdict emit failed (see %s/gate2-emit.stderr.log); exit code preserved as %s\n' \
      "${DEVLOOP_TMP}" "$__rc" >&2
  fi
  exit "$__rc"
}
trap '__gate2_emit_trap' EXIT

# CI-SENTINEL-LEAK runtime assertion (task #47, §J/C — security trust boundary).
# Shared single-locus check in _common.sh; called here (full pipeline) and from
# layer3.sh (standalone) — two legitimate entry points, one check body. Catches a
# DEVLOOP_TEST sentinel leak at the pipeline boundary, before any layer runs.
assert_no_ci_sentinel_leak

# Precondition: base ref must be pack-resident (ci.yml fetch-depth: 0 — task #42).
# Dispatch per mode mirrors _get_base_ref.sh's resolution branches.
if [[ -n "${GITHUB_ACTIONS:-}" && "${GITHUB_EVENT_NAME:-}" == "pull_request" ]]; then
  # CI-PR: $GITHUB_BASE_REF is the actual base branch (main OR develop).
  __precondition_ref="origin/${GITHUB_BASE_REF}"
elif [[ -z "${GITHUB_ACTIONS:-}" ]]; then
  # Local: resolver uses origin/main.
  __precondition_ref="origin/main"
else
  # CI-push: resolver uses HEAD~1 (local by definition, no remote pack lookup).
  __precondition_ref=""
fi
if [[ -n "$__precondition_ref" ]] && ! git merge-base "${__precondition_ref}" HEAD >/dev/null 2>&1; then
  # merge-base reachability check — catches both ref-missing (empty/wrong clone)
  # AND merge-base-outside-depth-window (production-likely depth-N shallow case)
  # with the same fetch-depth: 0 remediation.
  printf 'PRECONDITION_FAILURE: merge-base(%s, HEAD) unreachable — CI clone too shallow.\n\n' "$__precondition_ref" >&2
  printf 'Fix: set actions/checkout fetch-depth: 0 in .github/workflows/ci.yml\n' >&2
  printf 'See docs/runbooks/devloop-validation.md.\n' >&2
  exit 2
fi

# Cleanup prior run's logs (paired-operations §4).
rm -f "${DEVLOOP_TMP}"/layer-*.log "${DEVLOOP_TMP}"/layer-*.stderr.log "${DEVLOOP_TMP}"/changed-files.layer-*

# layer_status / layer_dur / final_exit are declared at the top (before the EXIT
# trap install) so the verdict emitter's namerefs always bind.
budget_secs_per_layer="${DEVLOOP_LAYER_BUDGET_SECS:-20}"
total_budget_secs=90  # ADR-0033 §4: 90s p95 wall-clock for the guard+audit fast tier (layers 3 + 6)

# Layer-script directory. Production is __here. The LAYER_SCRIPT_DIR override is a hermetic
# TEST SEAM (orchestrator lane-integrity test, task #56): it points the loop at fake layer
# scripts so a simulated layer-7 PRECONDITION_FAILURE can be driven end-to-end without a
# cluster. It is honored ONLY under DEVLOOP_TEST=1 (security): assert_no_ci_sentinel_leak
# above already hard-fails the pipeline if DEVLOOP_TEST leaks into CI, so this seam can
# never redirect the production orchestrator in a real CI/devloop run.
layer_script_dir="${__here}"
if [[ "${DEVLOOP_TEST:-}" == "1" && -n "${LAYER_SCRIPT_DIR:-}" ]]; then
  layer_script_dir="$LAYER_SCRIPT_DIR"
fi

# Fail-fast vs run-all mode (Change 2). Resolved by the pure _common.sh::fail_fast_mode()
# predicate (single source of the DEVLOOP_FAIL_FAST precedence). Placed AFTER
# assert_no_ci_sentinel_leak (above) so no new env is read before the sentinel check.
# fail_fast_mode always returns 0 and signals INVALID via stdout; `|| true` just keeps a
# (hypothetical) non-zero return from aborting under set -e without a dead capture var.
ff_decision="$(fail_fast_mode)" || true
ff_mode="${ff_decision%% *}"      # first word: FAILFAST | RUNALL | INVALID
ff_source="${ff_decision#"$ff_mode"}"; ff_source="${ff_source# }"  # remainder = SOURCE label (empty for INVALID)
case "$ff_mode" in
  FAILFAST) fail_fast=1 ;;
  RUNALL)   fail_fast=0 ;;
  *)  # INVALID (or any unexpected token) — fail-closed, loud, per "fail loudly; never mask".
    printf 'PRECONDITION_FAILURE: DEVLOOP_FAIL_FAST=%q is not a recognized boolean (use 1/0/true/false/yes/no).\n' "${DEVLOOP_FAIL_FAST:-}" >&2
    exit 2 ;;
esac
# Always announce the mode in force (R-E/N1) — a fact in the log, incl. someone else's pasted
# log; neither line starts with STATUS=. On an unattended override refusal, also emit the
# WARN token (matches the WARN BUDGET_* convention so `grep 'WARN '` finds it).
printf 'PIPELINE_MODE=%s SOURCE=%s\n' "$([[ $fail_fast -eq 1 ]] && printf 'fail-fast' || printf 'run-all')" "$ff_source" >&2
if [[ "$ff_source" == "unattended-override-refused" ]]; then
  echo "WARN FAIL_FAST_OVERRIDE_IGNORED REQUESTED=1 MODE=run-all" >&2
fi

for n in 1 2 3 4 5 6 7; do
  start=$(date +%s)
  # observability O3: atomic stderr append redirect (no process-sub race with stdout tee).
  # Capture the LAYER's real exit code (PIPESTATUS[0], NOT tee's). set +e/-e around the
  # pipeline so a non-zero layer doesn't abort the loop (replaces the old `if ! …` guard).
  set +e
  "${layer_script_dir}/layer${n}.sh" \
        2>>"${DEVLOOP_TMP}/layer-${n}.stderr.log" \
        | tee "${DEVLOOP_TMP}/layer-${n}.log"
  rc=${PIPESTATUS[0]}
  set -e
  end=$(date +%s)
  dur=$((end - start))

  # Accumulate the worst OBSERVED process exit code (the @operations FLOOR, task #56). The
  # authoritative LAYER_ALL_EXIT below is max(status_to_exit_code(total_result), this floor)
  # — the real rc is the belt to the enum's suspenders: a STATUS/rc MISMATCH (a layer whose
  # last STATUS line says OK but whose process exited non-zero) must NEVER demote
  # LAYER_ALL_EXIT below what the process actually returned. That mismatch is a cousin of
  # the silent-masking bug this task exists to kill.
  if (( rc > final_exit )); then final_exit=$rc; fi

  # observability O4 + dry-reviewer (C): single source of truth for STATUS parsing.
  status=$(parse_status_line "${DEVLOOP_TMP}/layer-${n}.log")
  layer_status[$n]="${status:-UNKNOWN}"
  layer_dur[$n]=$dur

  # Per-layer 20s warn EXCLUDES Layer 7 (task #56 ruling B): env-tests run in a separate
  # ~10–15 min envelope (ADR-0033 §4), so a Layer-7 BUDGET_BREACH would false-fire every
  # run and train operators to ignore the token. Layer 7 is also excluded from the 90s
  # guard+audit fast-tier total below (which sums only layers 3 + 6).
  if [[ $n -ne 7 && $dur -gt $budget_secs_per_layer ]]; then
    echo "WARN BUDGET_BREACH LAYER=${n} DURATION=${dur} BUDGET=${budget_secs_per_layer}" >&2
  fi

  # Change 2 — interactive fail-fast: STOP at the first failing layer. Keyed on the layer's
  # real process exit code `rc != 0` (@security S1 — NEVER status-based; a status-based stop
  # could halt on an exit-0 N/A/SKIPPED layer at final_exit==0 and forge GATE2=PASS over
  # un-run layers). rc catches even a lying `STATUS=OK; exit 1` layer. Placed at the END of
  # the loop body, AFTER final_exit + layer_status/dur are set, so the failing layer's own rc
  # and RESULT cell are recorded. Unattended (fail_fast=0) never breaks → byte-identical run-all.
  if (( fail_fast )) && (( rc != 0 )); then
    stopped_early=$n
    break
  fi
done

# Mark the layers that never ran under a fail-fast stop as NOT-RUN (DISPLAY-ONLY — see the
# NOT_RUN decl). Setting them explicitly means the `${…:-UNKNOWN}` reads below never fire for
# an un-run layer (UNKNOWN would wrongly inflate TOTAL_RESULT → exit 2). Loud greppable stop
# line so "layers N+1..7 missing" is never ambiguous with a truncated log.
if (( stopped_early > 0 )); then
  not_run_list=""
  for (( n = stopped_early + 1; n <= 7; n++ )); do
    layer_status[$n]="$NOT_RUN"
    layer_dur[$n]=0
    not_run_list="${not_run_list:+${not_run_list},}${n}"
  done
  echo "STOPPED_EARLY LAYER=${stopped_early} RESULT=${layer_status[$stopped_early]} NOT_RUN=${not_run_list}" >&2
  # Defensive (N3): fail-fast only ever breaks on rc!=0, so final_exit>0 by construction here.
  # A zero here is an orchestrator bug that would let a stopped-early run report success — a
  # fixed greppable token so it's identifiable in a log and rides the §4 one-pass grep.
  if (( final_exit == 0 )); then
    echo "PRECONDITION_FAILURE: stopped early at layer ${stopped_early} but final_exit==0 — orchestrator invariant violated REASON=fail-fast-exit-invariant" >&2
    exit 2
  fi
fi

# Guard+audit fast-tier budget check (layers 3 + 6 per ADR-0033 §4). Deliberately NOT a
# sum over all always-run layers: since 2026-08-20 layers 1/2/4/5 also always-run, but
# their compile/fmt/test/lint cost is inherently large/variable and out of this p95 budget
# (like layer 7's env-test envelope). This is the cheap fast-floor latency guard only.
# Under fail-fast, if layer 3 OR 6 did not run, the sum would be half-measured — a check that
# silently stops checking (CLAUDE.md "fail loudly; never mask"). Skip it with a loud greppable
# token (joins the WARN BUDGET_* family) rather than compare against absent data. Keyed off the
# NOT-RUN status, not layer_dur==0 (a genuinely fast layer 6 can legitimately measure 0s).
if [[ "${layer_status[3]:-}" != "$NOT_RUN" && "${layer_status[6]:-}" != "$NOT_RUN" ]]; then
  guard_audit_dur=$(( ${layer_dur[3]:-0} + ${layer_dur[6]:-0} ))
  if [[ $guard_audit_dur -gt $total_budget_secs ]]; then
    echo "WARN BUDGET_TOTAL_BREACH GUARD_AUDIT_DURATION=${guard_audit_dur} BUDGET=${total_budget_secs}" >&2
  fi
else
  echo "WARN BUDGET_TOTAL_SKIPPED REASON=layers-not-run LAST_RAN=${stopped_early}" >&2
fi

# Aggregate total + emit machine-parseable summary block (paired-operations §5).
# NOT-RUN layers are DISPLAY-ONLY: skipped here so they never vote (not a new aggregation
# enum). TOTAL_RESULT is therefore the worst of the layers that ACTUALLY ran (through the
# failing one) = the failing layer's status; final_exit already holds its rc.
total_dur=0
total_result="OK"
for n in 1 2 3 4 5 6 7; do
  total_dur=$(( total_dur + ${layer_dur[$n]:-0} ))
  [[ "${layer_status[$n]:-UNKNOWN}" == "$NOT_RUN" ]] && continue
  total_result=$(aggregate_worst_status "$total_result" "${layer_status[$n]:-UNKNOWN}")
done

# Authoritative process exit = max(status_to_exit_code(total_result), worst-observed-rc).
# The enum mapping (dry-reviewer F1 single source) carries the SEMANTICS — so the operator
# lane (Layer-7 PRECONDITION_FAILURE) reaches LAYER_ALL_EXIT=2 and emit_gate2_verdict
# derives GATE2 correctly, and the latent FAIL-MISSING-VERB(exit 2)→exit-1 collapse is
# repaired. The worst-observed-rc FLOOR (@operations) is the belt: a STATUS/rc mismatch
# (last STATUS line OK but the process exited non-zero) can never demote the pipeline below
# the real return code. final_exit already holds the worst observed rc from the loop.
mapped_exit=$(status_to_exit_code "$total_result")
if (( mapped_exit > final_exit )); then final_exit=$mapped_exit; fi

printf '\n=== LAYER_SUMMARY_BEGIN ===\n'
for n in 1 2 3 4 5 6 7; do
  printf 'LAYER=%d RESULT=%s DURATION=%s\n' "$n" "${layer_status[$n]:-UNKNOWN}" "${layer_dur[$n]:-0}"
done
printf '=== LAYER_SUMMARY_END ===\n'
printf 'TOTAL_DURATION=%s TOTAL_RESULT=%s\n\n' "$total_dur" "$total_result"

# Human-readable table.
printf '%-8s %-22s %s\n' "Layer" "Status" "Duration(s)"
printf '%-8s %-22s %s\n' "-----" "------" "-----------"
for n in 1 2 3 4 5 6 7; do
  printf '%-8s %-22s %s\n' "$n" "${layer_status[$n]:-UNKNOWN}" "${layer_dur[$n]:-0}"
done

exit "$final_exit"
