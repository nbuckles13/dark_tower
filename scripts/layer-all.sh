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

set -euo pipefail
IFS=$'\n\t'

__here="$(cd "$(dirname "$0")" && pwd)"
source "${__here}/lang/_common.sh"
init_devloop_tmp

# Cleanup prior run's logs (paired-operations §4).
rm -f "${DEVLOOP_TMP}"/layer-*.log "${DEVLOOP_TMP}"/layer-*.stderr.log "${DEVLOOP_TMP}"/changed-files.layer-*

declare -a layer_status layer_dur
budget_secs_per_layer="${DEVLOOP_LAYER_BUDGET_SECS:-20}"
total_budget_secs=90  # ADR-0033 §4: 90s p95 wall-clock for the always-run set (layers 3 + 6)
final_exit=0

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
