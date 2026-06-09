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
#   Always-run subset (layers 3 + 6): hard 90s p95.
#
# Greppable warn tokens (paired-operations §2):
#   WARN BUDGET_BREACH LAYER=<n> DURATION=<s> BUDGET=<s>
#   WARN BUDGET_TOTAL_BREACH ALWAYS_RUN_DURATION=<s> BUDGET=90
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

# Per-layer result/duration accumulators — declared BEFORE the EXIT trap installs
# so emit_gate2_verdict's namerefs always bind to existing (possibly-empty) arrays,
# even on an early exit that fires before the layer loop populates them.
declare -a layer_status layer_dur
final_exit=0

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
total_budget_secs=90  # ADR-0033 §4: 90s p95 wall-clock for the always-run set (layers 3 + 6)

for n in 1 2 3 4 5 6 7; do
  start=$(date +%s)
  # observability O3: atomic stderr append redirect (no process-sub race with stdout tee).
  if ! "${__here}/layer${n}.sh" \
        2>>"${DEVLOOP_TMP}/layer-${n}.stderr.log" \
        | tee "${DEVLOOP_TMP}/layer-${n}.log"; then
    final_exit=1
  fi
  end=$(date +%s)
  dur=$((end - start))

  # observability O4 + dry-reviewer (C): single source of truth for STATUS parsing.
  status=$(parse_status_line "${DEVLOOP_TMP}/layer-${n}.log")
  layer_status[$n]="${status:-UNKNOWN}"
  layer_dur[$n]=$dur

  if [[ $dur -gt $budget_secs_per_layer ]]; then
    echo "WARN BUDGET_BREACH LAYER=${n} DURATION=${dur} BUDGET=${budget_secs_per_layer}" >&2
  fi
done

# Always-run subset budget check (layers 3 + 6 per ADR-0033 §4).
always_run_dur=$(( ${layer_dur[3]:-0} + ${layer_dur[6]:-0} ))
if [[ $always_run_dur -gt $total_budget_secs ]]; then
  echo "WARN BUDGET_TOTAL_BREACH ALWAYS_RUN_DURATION=${always_run_dur} BUDGET=${total_budget_secs}" >&2
fi

# Aggregate total + emit machine-parseable summary block (paired-operations §5).
total_dur=0
total_result="OK"
for n in 1 2 3 4 5 6 7; do
  total_dur=$(( total_dur + ${layer_dur[$n]:-0} ))
  total_result=$(aggregate_worst_status "$total_result" "${layer_status[$n]:-UNKNOWN}")
done

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
