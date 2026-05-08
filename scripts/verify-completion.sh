#!/usr/bin/env bash
# Verify-completion — refactored to call layer scripts (ADR-0033 §4 + paired-operations Q5).
#
# Preserves the CLI surface (--layer, --format, --verbose, <path>) while
# delegating actual work to scripts/layerN.sh + scripts/layer-all.sh.
#
# --layer mapping (per-verb-name, not per-layer-number):
#   quick    → L1 compile, L2 format, L3 guards         (no tests/lint/audit/env-tests)
#   standard → quick + L4 test                          (no lint/audit/env-tests)
#   full     → all layers L1-L7 (canonical pipeline)    (literally invokes layer-all.sh)
#
# `--layer full` IS `scripts/layer-all.sh` (paired-operations Q5): identical layer
# scripts run, identical exit-code semantics. Layer loop is NOT reimplemented here.
#
# <path> arg is informational/cosmetic; layers run repo-wide.

set -euo pipefail
IFS=$'\n\t'

__here="$(cd "$(dirname "$0")" && pwd)"
source "${__here}/lang/_common.sh"

# Default DATABASE_URL for tests (preserved from original).
export DATABASE_URL="${DATABASE_URL:-postgresql://postgres:postgres@localhost:5433/dark_tower_test}"

# CLI defaults.
LAYER="full"
FORMAT="text"
VERBOSE=false
SEARCH_PATH="$__here/.."   # informational only; new pipeline runs repo-wide

usage() {
  cat <<'EOF'
Usage: verify-completion.sh [options] [path]

Options:
  --layer LEVEL    Verification level: quick, standard, full (default: full)
                   quick    → L1 compile, L2 format, L3 guards
                   standard → quick + L4 test
                   full     → all layers L1-L7 (= scripts/layer-all.sh)
  --format FORMAT  Output format: text, json (default: text)
  --verbose        Show detailed output
  --help           Show this help message

  [path]           Informational only — layers run repo-wide.

Examples:
  ./verify-completion.sh                    # Full verification
  ./verify-completion.sh --layer quick      # Fast feedback
  ./verify-completion.sh --format json      # Machine-readable output
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --layer)   LAYER="$2"; shift 2 ;;
    --format)  FORMAT="$2"; shift 2 ;;
    --verbose) VERBOSE=true; shift ;;
    --help)    usage; exit 0 ;;
    -*)        echo "Unknown option: $1" >&2; exit 2 ;;
    *)         SEARCH_PATH="$1"; shift ;;
  esac
done

if [[ ! "$LAYER" =~ ^(quick|standard|full)$ ]]; then
  echo "Invalid layer: $LAYER (must be quick, standard, or full)" >&2
  exit 2
fi
if [[ ! "$FORMAT" =~ ^(text|json)$ ]]; then
  echo "Invalid format: $FORMAT (must be text or json)" >&2
  exit 2
fi

START_TIME=$(date +%s)
init_devloop_tmp

# Determine which layer scripts to run.
declare -a layers
case "$LAYER" in
  quick)    layers=(1 2 3) ;;
  standard) layers=(1 2 3 4) ;;
  full)     layers=(1 2 3 4 5 6 7) ;;
esac

# `--layer full` is structurally identical to layer-all.sh — invoke it directly.
declare -a layer_status layer_dur
final_exit=0

if [[ "$LAYER" == "full" ]]; then
  if ! "$__here/layer-all.sh"; then
    final_exit=1
  fi
  # Read back the layer logs to populate the status table for --format reporting.
  for n in "${layers[@]}"; do
    layer_status[$n]=$(parse_status_line "${DEVLOOP_TMP}/layer-${n}.log" 2>/dev/null || true)
    layer_status[$n]="${layer_status[$n]:-UNKNOWN}"
    layer_dur[$n]=0  # durations live in stderr LAYER= lines; not parsed here for brevity
  done
else
  # quick/standard: iterate the selected layers directly.
  rm -f "${DEVLOOP_TMP}"/layer-*.log "${DEVLOOP_TMP}"/layer-*.stderr.log "${DEVLOOP_TMP}"/changed-files.layer-*
  for n in "${layers[@]}"; do
    start=$(date +%s)
    if ! "${__here}/layer${n}.sh" \
          2>>"${DEVLOOP_TMP}/layer-${n}.stderr.log" \
          | tee "${DEVLOOP_TMP}/layer-${n}.log"; then
      final_exit=1
    fi
    end=$(date +%s)
    layer_dur[$n]=$((end - start))
    layer_status[$n]=$(parse_status_line "${DEVLOOP_TMP}/layer-${n}.log")
    layer_status[$n]="${layer_status[$n]:-UNKNOWN}"
  done
fi

ELAPSED=$(( $(date +%s) - START_TIME ))

# -----------------------------------------------------------------------------
# Output
# -----------------------------------------------------------------------------

output_text() {
  printf '\n==========================================\n'
  printf 'Verification Report\n'
  printf '==========================================\n\n'
  printf 'Layer:   %s\n' "$LAYER"
  printf 'Elapsed: %ss\n\n' "$ELAPSED"
  printf '%-8s %-22s\n' "Layer" "Status"
  printf '%-8s %-22s\n' "-----" "------"
  for n in "${layers[@]}"; do
    printf '%-8s %-22s\n' "$n" "${layer_status[$n]:-UNKNOWN}"
  done
  if [[ $final_exit -eq 0 ]]; then
    printf '\nAll layers passed.\n'
  else
    printf '\nVerification failed — see %s/layer-N.log for details.\n' "$DEVLOOP_TMP"
  fi
}

output_json() {
  local first=true
  printf '{\n'
  printf '  "layer": "%s",\n' "$LAYER"
  printf '  "elapsed_seconds": %s,\n' "$ELAPSED"
  printf '  "passed": %s,\n' "$( [[ $final_exit -eq 0 ]] && echo true || echo false )"
  printf '  "layers": [\n'
  for n in "${layers[@]}"; do
    [[ "$first" != "true" ]] && printf ',\n'
    first=false
    printf '    {"layer": %s, "status": "%s"}' "$n" "${layer_status[$n]:-UNKNOWN}"
  done
  printf '\n  ]\n}\n'
}

if [[ "$FORMAT" == "json" ]]; then
  output_json
else
  output_text
fi

exit "$final_exit"
