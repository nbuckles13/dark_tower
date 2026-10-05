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
# Greppable failure-triage token (ADR-0037 D8, on stderr; one per actionable-failed layer):
#   FAILURE_TRIAGE LAYER=<n> LOG=<path> STDERR_LOG=<path> RESULT=<status>
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

# Runtime discoverability for the memory cap (build.jobs in .cargo/config.toml): announce the
# effective cargo job budget and its escape hatch once at pipeline start, so anyone
# watching a run sees why parallelism is limited and how to change it.
if [[ -n "${CARGO_BUILD_JOBS:-}" ]]; then
  __cap="${CARGO_BUILD_JOBS} (from the CARGO_BUILD_JOBS env override)"
else
  __cap="$(sed -n 's/^jobs[[:space:]]*=[[:space:]]*\([0-9][0-9]*\).*/\1/p' "${__here}/../.cargo/config.toml" | head -n1)"
  __cap="${__cap:-<unset>} (build.jobs in .cargo/config.toml)"
fi
printf 'cargo jobs cap: %s — capped for memory; export CARGO_BUILD_JOBS=N or pass --jobs to override\n' "$__cap" >&2

# --- One result model for every mode -----------------------------------------------------
# A full run, a fast lane, a CI shard and the CI aggregate all report through the same
# per-layer arrays (layer_status / layer_dur / layer_rc, indexed by layer number, over the
# layers in layers_to_run) and the same two functions below, so the parsed LAYER_SUMMARY
# contract has exactly one emitter. layer_shard is filled only by --aggregate.
declare -a layers_to_run=() layer_status=() layer_dur=() layer_rc=()
declare -A layer_shard=()

# Sets total_result (worst status of the layers that ACTUALLY ran) and total_dur. NOT-RUN
# layers are DISPLAY-ONLY: skipped so they never vote (not a new aggregation enum), so under
# fail-fast TOTAL_RESULT is the failing layer's status.
compute_layer_totals() {
  local n
  total_dur=0
  total_result="OK"
  for n in "${layers_to_run[@]}"; do
    total_dur=$(( total_dur + ${layer_dur[$n]:-0} ))
    [[ "${layer_status[$n]:-UNKNOWN}" == "$NOT_RUN" ]] && continue
    total_result=$(aggregate_worst_status "$total_result" "${layer_status[$n]:-UNKNOWN}")
  done
}

# The machine-parseable LAYER_SUMMARY block (paired-operations §5) + the human table. The
# table gains a Shard column only when --aggregate filled layer_shard.
print_layer_summary() {
  local n
  printf '\n=== LAYER_SUMMARY_BEGIN ===\n'
  for n in "${layers_to_run[@]}"; do
    printf 'LAYER=%d RESULT=%s DURATION=%s\n' "$n" "${layer_status[$n]:-UNKNOWN}" "${layer_dur[$n]:-0}"
  done
  printf '=== LAYER_SUMMARY_END ===\n'
  printf 'TOTAL_DURATION=%s TOTAL_RESULT=%s\n\n' "$total_dur" "$total_result"
  if (( ${#layer_shard[@]} )); then
    printf '%-8s %-22s %-12s %s\n' "Layer" "Status" "Duration(s)" "Shard"
    printf '%-8s %-22s %-12s %s\n' "-----" "------" "-----------" "-----"
    for n in "${layers_to_run[@]}"; do
      printf '%-8s %-22s %-12s %s\n' "$n" "${layer_status[$n]:-UNKNOWN}" "${layer_dur[$n]:-0}" "${layer_shard[$n]:-}"
    done
  else
    printf '%-8s %-22s %s\n' "Layer" "Status" "Duration(s)"
    printf '%-8s %-22s %s\n' "-----" "------" "-----------"
    for n in "${layers_to_run[@]}"; do
      printf '%-8s %-22s %s\n' "$n" "${layer_status[$n]:-UNKNOWN}" "${layer_dur[$n]:-0}"
    done
  fi
}

# The shard summary's file name: written by `--layers` into DEVLOOP_TMP, read by
# `--aggregate` from every shard's directory (ci.yml uploads it by this name).
SHARD_SUMMARY_FILE="layer-summary.txt"

# --- CI aggregate (`--aggregate DIR`) ------------------------------------------------------
# Reads every `layer-summary.txt` under DIR (one per CI shard, downloaded from THIS workflow
# run) and decides the pipeline result as if one invocation had run every layer. It is the
# Gate-2 backstop's completeness proof once CI is sharded, so it is strict:
#   * each line must match one anchored shape with a KNOWN status (a status
#     __status_rank ranks below its fail-closed catch-all, or UNKNOWN — the token a layer
#     with no STATUS line records). The file is never sourced/eval'd;
#   * every layer 1..LAYER_MAX must appear EXACTLY once across all summaries;
#   * any violation, or no summary at all, is PRECONDITION_FAILURE (exit 2) with a REASON.
# Then TOTAL_RESULT = worst status, exit = max(status_to_exit_code(TOTAL), every RC) — the
# same aggregation and observed-rc FLOOR as a single invocation.
aggregate_shard_summaries() {
  local dir="$1" f line n status dur rc shard
  local -a files=()
  agg_fail() {
    printf 'PRECONDITION_FAILURE: --aggregate: %s REASON=aggregate-%s\n' "$2" "$1" >&2
    printf 'STATUS=PRECONDITION_FAILURE REASON=aggregate-%s\n' "$1"
    return 2
  }
  [[ -d "$dir" && -r "$dir" ]] || { agg_fail dir-unreadable "summary directory ${dir@Q} is missing or unreadable"; return 2; }
  mapfile -t files < <(find "$dir" -type f -name "$SHARD_SUMMARY_FILE" | sort)
  (( ${#files[@]} )) || { agg_fail no-summaries "no ${SHARD_SUMMARY_FILE} under ${dir@Q} — no shard reported"; return 2; }
  for f in "${files[@]}"; do
    shard="$(basename "$(dirname "$f")")"
    while IFS= read -r line || [[ -n "$line" ]]; do
      if [[ ! "$line" =~ ^LAYER=([1-9][0-9]*)\ RESULT=([A-Z/_-]+)\ DURATION=([0-9]+)\ RC=([0-9]+)$ ]]; then
        agg_fail malformed-line "${f}: unparseable line ${line@Q}"; return 2
      fi
      n="${BASH_REMATCH[1]}"; status="${BASH_REMATCH[2]}"; dur="${BASH_REMATCH[3]}"; rc="${BASH_REMATCH[4]}"
      if [[ "$status" != "UNKNOWN" && "$(__status_rank "$status")" -ge "$(__status_rank UNKNOWN)" ]]; then
        agg_fail unknown-status "${f}: layer ${n} RESULT=${status} is not a pipeline status"; return 2
      fi
      if (( n > LAYER_MAX )); then
        agg_fail layer-out-of-range "${f}: layer ${n} is outside 1..${LAYER_MAX}"; return 2
      fi
      if [[ -n "${layer_status[$n]:-}" ]]; then
        agg_fail duplicate-layer "layer ${n} reported by both ${layer_shard[$n]} and ${shard}"; return 2
      fi
      layer_status[$n]="$status"; layer_dur[$n]="$dur"; layer_rc[$n]="$rc"; layer_shard[$n]="$shard"
    done < "$f"
  done
  local missing=""
  for n in "${layers_to_run[@]}"; do
    [[ -n "${layer_status[$n]:-}" ]] || missing="${missing:+${missing},}${n}"
  done
  [[ -z "$missing" ]] || { agg_fail missing-layer "layer(s) ${missing} reported by no shard — the CI partition must cover 1..${LAYER_MAX}"; return 2; }

  # Same totals and the same exit rule as one invocation: max(mapped status, every RC).
  local worst_rc=0 mapped
  compute_layer_totals
  for n in "${layers_to_run[@]}"; do
    (( layer_rc[n] > worst_rc )) && worst_rc=${layer_rc[n]}
  done
  mapped=$(status_to_exit_code "$total_result")
  (( mapped > worst_rc )) && worst_rc=$mapped

  print_layer_summary
  # Point at the shard job whose log holds the detail (same token family as a local run).
  for n in "${layers_to_run[@]}"; do
    if [[ "$(status_to_exit_code "${layer_status[$n]}")" -ne 0 || "${layer_rc[$n]}" -ne 0 ]]; then
      printf 'FAILURE_TRIAGE LAYER=%d SHARD=%s RC=%s RESULT=%s\n' "$n" "${layer_shard[$n]}" "${layer_rc[$n]}" "${layer_status[$n]}" >&2
      printf '    open the %s shard job log: its layer-%d detail and FAILURE_TRIAGE line are there.\n' "${layer_shard[$n]}" "$n" >&2
    fi
  done
  return "$worst_rc"
}

# --- Layer range (developer fast lane) ---------------------------------------
# Default: the full authority pipeline, layers 1..LAYER_MAX. `--max-layer N` runs layers
# 1..N only; it is the mechanism behind scripts/layer-fast.sh — the non-destructive
# inner-loop check an implementer runs before "Ready for review", which stops
# BEFORE Layer 7's shared-cluster bring-up (the resource a concurrent run thrashes).
#
# Two guardrails keep the fast lane off the authority path:
#   1. A shortened `--max-layer` range is REFUSED in any tree-attesting context (CI, the run-story
#      authority/close gate, pre-commit — the same GITHUB_ACTIONS/DEVLOOP_FMT_CHECK_ONLY
#      sentinels CARGO_LOCKED gates on). The authority pipeline can never be shortened.
#   2. A shortened run emits NO Gate-2 verdict (the EXIT trap below is not installed for
#      it). A missing verdict is fail-closed — a commit still needs a full layer-all.sh —
#      so a fast run can never be mistaken for an authority PASS.
# No local caller passes positional args (run-story, verify-completion, pre-commit invoke
# bare), so the default path is byte-identical to before.
#
# --- CI shards (`--layers`) and their aggregate (`--aggregate`) ---------------------------
# CI splits the pipeline across parallel jobs (.github/workflows/ci.yml): each shard runs
# `layer-all.sh --layers <list>` FROM SCRATCH, and a final `Test Suite` job runs
# `layer-all.sh --aggregate <dir>` over the shards' summaries. The aggregate requires
# every layer 1..LAYER_MAX exactly once, so the union of the shards provably equals this
# pipeline: a layer dropped from (or duplicated in) the CI partition fails loudly.
#   * `--layers` is accepted only under GITHUB_ACTIONS (or the hermetic test seam). That is
#     CONVENIENCE gating, NOT a
#     security control — the variable is trivially set locally. It is acceptable only
#     because a `--layers` run never writes, removes or overwrites a Gate-2 verdict (the
#     EXIT trap below is not installed for it); its output is the shard summary file,
#     which only the aggregate reads. Local users keep `--max-layer`.
#   * `--aggregate` runs no layer and writes no verdict; it only reads summary files.
layer_lo=1
layer_hi="$LAYER_MAX"
layers_arg=""
aggregate_dir=""
mode="full"
set_mode() {  # one mode per invocation; --max-layer / --layers / --aggregate are exclusive
  if [[ "$mode" != "full" && "$mode" != "$1" ]]; then
    printf 'PRECONDITION_FAILURE: --max-layer, --layers and --aggregate are mutually exclusive.\n' >&2
    exit 2
  fi
  mode="$1"
}
need_value() {
  [[ $# -gt 1 && -n "$2" ]] || { printf 'PRECONDITION_FAILURE: %s needs a value.\n' "$1" >&2; exit 2; }
}
while (( $# )); do
  case "$1" in
    --max-layer)   need_value "$@"; set_mode max; shift; layer_hi="$1" ;;
    --max-layer=*) set_mode max; layer_hi="${1#--max-layer=}" ;;
    --layers)      need_value "$@"; set_mode shard; shift; layers_arg="$1" ;;
    --layers=*)    set_mode shard; layers_arg="${1#--layers=}" ;;
    --aggregate)   need_value "$@"; set_mode aggregate; shift; aggregate_dir="$1" ;;
    --aggregate=*) set_mode aggregate; aggregate_dir="${1#--aggregate=}" ;;
    *) printf 'PRECONDITION_FAILURE: unknown argument %q — layer-all.sh accepts only --max-layer N, --layers L,... (CI) or --aggregate DIR.\n' "$1" >&2; exit 2 ;;
  esac
  shift
done
is_layer_number() { [[ "$1" =~ ^[1-9][0-9]*$ ]] && (( $1 >= 1 && $1 <= LAYER_MAX )); }

fast_lane=0
shard_mode=0
case "$mode" in
  full|max)
    is_layer_number "$layer_hi" || { printf 'PRECONDITION_FAILURE: --max-layer must be an integer 1..%s (got %q).\n' "$LAYER_MAX" "$layer_hi" >&2; exit 2; }
    for (( n = layer_lo; n <= layer_hi; n++ )); do layers_to_run+=("$n"); done
    if (( layer_hi < LAYER_MAX )); then
      if [[ -n "${GITHUB_ACTIONS:-}" || -n "${DEVLOOP_FMT_CHECK_ONLY:-}" ]]; then
        printf 'PRECONDITION_FAILURE: --max-layer=%s refused in an attesting context (CI / run-story gate / pre-commit) — the authority pipeline runs all %s layers. REASON=fast-lane-in-attesting-context\n' "$layer_hi" "$LAYER_MAX" >&2
        exit 2
      fi
      fast_lane=1
    fi
    ;;
  shard)
    # GITHUB_ACTIONS, or the hermetic LAYER_SCRIPT_DIR test seam (DEVLOOP_TEST-gated, and
    # DEVLOOP_TEST can never be set in CI — assert_no_ci_sentinel_leak), so
    # scripts/layer-all.test.sh can drive shard mode against stub layers.
    if [[ ( -z "${GITHUB_ACTIONS:-}" && ! ( "${DEVLOOP_TEST:-}" == "1" && -n "${LAYER_SCRIPT_DIR:-}" ) ) || -n "${DEVLOOP_FMT_CHECK_ONLY:-}" ]]; then
      printf 'PRECONDITION_FAILURE: --layers is the CI shard mode (GITHUB_ACTIONS only, never a run-story gate / pre-commit); locally use --max-layer N or the full pipeline. REASON=shard-outside-ci\n' >&2
      exit 2
    fi
    prev=0
    IFS=',' read -r -a __shard_list <<< "$layers_arg"
    for n in "${__shard_list[@]}"; do
      if ! is_layer_number "$n" || (( n <= prev )); then
        printf 'PRECONDITION_FAILURE: --layers must be ascending, distinct integers 1..%s, comma-separated (got %q). REASON=shard-layer-list-invalid\n' "$LAYER_MAX" "$layers_arg" >&2
        exit 2
      fi
      layers_to_run+=("$n"); prev=$n
    done
    (( ${#layers_to_run[@]} )) || { printf 'PRECONDITION_FAILURE: --layers is empty. REASON=shard-layer-list-invalid\n' >&2; exit 2; }
    shard_mode=1
    # CI reads the summary's path from here (the upload step's `path:`), so the file name
    # and directory live only in this script: emitted up front, before any layer can fail.
    if [[ -n "${GITHUB_OUTPUT:-}" ]]; then
      printf 'summary-path=%s\n' "${DEVLOOP_TMP}/${SHARD_SUMMARY_FILE}" >> "$GITHUB_OUTPUT"
    fi
    ;;
  aggregate)
    for (( n = 1; n <= LAYER_MAX; n++ )); do layers_to_run+=("$n"); done
    ;;
esac
layer_hi="${layers_to_run[-1]}"

# The per-layer accumulators (layer_status / layer_dur / layer_rc) are declared at the top
# ("One result model"), i.e. BEFORE the EXIT trap installs, so emit_gate2_verdict's namerefs
# always bind to existing (possibly-empty) arrays, even on an early exit that fires before
# the layer loop populates them.
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

# --aggregate runs no layer and writes no verdict: dispatch it before the verdict trap.
if [[ "$mode" == "aggregate" ]]; then
  aggregate_shard_summaries "$aggregate_dir"
  exit $?
fi

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
# Authority runs only: a fast-lane run (--max-layer < LAYER_MAX) writes no Gate-2 verdict
# (guardrail 2 above), and neither does a CI shard (--layers), so neither EXIT may fire
# the emitter — a shard run must never write, remove or overwrite a verdict file.
if (( ! fast_lane && ! shard_mode )); then
  trap '__gate2_emit_trap' EXIT
fi

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
rm -f "${DEVLOOP_TMP}"/layer-*.log "${DEVLOOP_TMP}"/layer-*.stderr.log "${DEVLOOP_TMP}"/changed-files.layer-* "${DEVLOOP_TMP}/${SHARD_SUMMARY_FILE}"

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
if (( fast_lane )); then
  printf 'FAST_LANE=1 LAYERS=%d-%d — developer inner-loop check; NOT an authority Gate-2 run; no verdict emitted. Layer %d (shared cluster) is the Lead Gate 2.\n' "$layer_lo" "$layer_hi" "$LAYER_MAX" >&2
fi
if (( shard_mode )); then
  printf 'CI_SHARD=1 LAYERS=%s — runs these layers from scratch; no verdict emitted; the Test Suite job aggregates every shard (--aggregate).\n' "$layers_arg" >&2
fi

for n in "${layers_to_run[@]}"; do
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
  # Record the layer's real process exit code for the ADR-0037 D8 directive (ADD-ONLY — a value
  # already computed above; not consumed by aggregation or the Gate-2 trap). Lets the directive
  # fire on a lying `STATUS=OK; exit 1` layer (the :170 FLOOR case), not just on the parsed enum.
  layer_rc[$n]=$rc

  # Per-layer 20s warn EXCLUDES Layer 7 (task #56 ruling B): env-tests run in a separate
  # ~10–15 min envelope (ADR-0033 §4), so a Layer-7 BUDGET_BREACH would false-fire every
  # run and train operators to ignore the token. Layer 7 is also excluded from the 90s
  # guard+audit fast-tier total below (which sums only layers 3 + 6).
  if [[ $n -ne $LAYER_MAX && $dur -gt $budget_secs_per_layer ]]; then
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
  for n in "${layers_to_run[@]}"; do
    (( n > stopped_early )) || continue
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
# Only measurable when this invocation includes both layers: a fast lane that stops before
# 6, or a CI shard without 3 and 6, has nothing to sum (and nothing to warn about).
__runs_layer() { local x; for x in "${layers_to_run[@]}"; do [[ "$x" == "$1" ]] && return 0; done; return 1; }
if ! __runs_layer 3 || ! __runs_layer 6; then
  : # this invocation does not include both fast-tier layers; the budget is not measurable here.
elif [[ "${layer_status[3]:-}" != "$NOT_RUN" && "${layer_status[6]:-}" != "$NOT_RUN" ]]; then
  guard_audit_dur=$(( ${layer_dur[3]:-0} + ${layer_dur[6]:-0} ))
  if [[ $guard_audit_dur -gt $total_budget_secs ]]; then
    echo "WARN BUDGET_TOTAL_BREACH GUARD_AUDIT_DURATION=${guard_audit_dur} BUDGET=${total_budget_secs}" >&2
  fi
else
  echo "WARN BUDGET_TOTAL_SKIPPED REASON=layers-not-run LAST_RAN=${stopped_early}" >&2
fi

compute_layer_totals

# Authoritative process exit = max(status_to_exit_code(total_result), worst-observed-rc).
# The enum mapping (dry-reviewer F1 single source) carries the SEMANTICS — so the operator
# lane (Layer-7 PRECONDITION_FAILURE) reaches LAYER_ALL_EXIT=2 and emit_gate2_verdict
# derives GATE2 correctly, and the latent FAIL-MISSING-VERB(exit 2)→exit-1 collapse is
# repaired. The worst-observed-rc FLOOR (@operations) is the belt: a STATUS/rc mismatch
# (last STATUS line OK but the process exited non-zero) can never demote the pipeline below
# the real return code. final_exit already holds the worst observed rc from the loop.
mapped_exit=$(status_to_exit_code "$total_result")
if (( mapped_exit > final_exit )); then final_exit=$mapped_exit; fi

print_layer_summary

# CI shard: hand this invocation's per-layer results to the Test Suite aggregate
# (--aggregate). One line per layer this shard ran, in the shape the aggregate parses.
if (( shard_mode )); then
  for n in "${layers_to_run[@]}"; do
    printf 'LAYER=%d RESULT=%s DURATION=%s RC=%s\n' "$n" "${layer_status[$n]:-UNKNOWN}" "${layer_dur[$n]:-0}" "${layer_rc[$n]:-0}"
  done > "${DEVLOOP_TMP}/${SHARD_SUMMARY_FILE}"
fi

# ADR-0037 D8 — point-of-failure log directive (ADD-ONLY, DISPLAY-ONLY). Each failing layer's
# detail is ALREADY captured (run-guards.sh names the guard + first error lines; each layer's
# stdout is tee'd to ${DEVLOOP_TMP}/layer-N.log and stderr to layer-N.stderr.log). A reader seeing
# only the aggregate "Layer N FAILED" re-runs the whole layer to get detail it already has — pure
# token/time waste. So point at the logs, in the output the reader is already looking at, and name
# the anti-pattern. This block reads layer_status / layer_rc / DEVLOOP_TMP but NEVER final_exit /
# TOTAL_RESULT / LAYER_SUMMARY / the NOT-RUN handling / aggregation / STOPPED_EARLY / the
# __gate2_emit_trap — it changes what is DISPLAYED on a failure, never what a failure MEANS.
#
# Channel = stderr, joining the WARN BUDGET_* / PIPELINE_MODE= / STOPPED_EARLY / PRECONDITION_FAILURE:
# family; stdout stays the machine block + table (a stdout FAILURE_TRIAGE line would also collide
# with the summary-cell substring assertions in layer-all.test.sh). Placed AFTER the human table so
# it is the LAST thing on screen — visible when a fail-fast run ends here, and surviving
# run-story.sh's `tail -n 50 "$gatelog"` on a run-all authority gate.
#
# WHICH layers get a directive: those with ACTIONABLE detail — status_to_exit_code(status) != 0
# (the FAIL / PRECONDITION_FAILURE / FAIL-MISSING-VERB / UNKNOWN set) OR real process rc != 0 (the
# lying `STATUS=OK; exit 1` masking case the :170 FLOOR names). NOT-RUN is filtered FIRST and
# EXPLICITLY, before the exit-code map: status_to_exit_code(NOT-RUN) hits its fail-closed `*)`->2
# backstop and would otherwise hand a directive to a layer that never ran (the mirror of the "do
# NOT complete the enum" guardrail on the NOT_RUN decl above). OK / N/A / SKIPPED-* map to 0 with
# rc 0, so they are silently skipped. Works under BOTH modes: fail-fast (un-run layers are NOT-RUN
# -> skipped; the one red layer gets it) and run-all (every red layer gets one).
for n in "${layers_to_run[@]}"; do
  st="${layer_status[$n]:-UNKNOWN}"
  [[ "$st" == "$NOT_RUN" ]] && continue
  # ${layer_rc[$n]:-0} defaulted for `set -u`: fail-fast sets layer_status/layer_dur for un-run
  # layers (above) but never layer_rc.
  if [[ "$(status_to_exit_code "$st")" -ne 0 || "${layer_rc[$n]:-0}" -ne 0 ]]; then
    triage_log="${DEVLOOP_TMP}/layer-${n}.log"
    triage_errlog="${DEVLOOP_TMP}/layer-${n}.stderr.log"
    # RESULT= placed NON-adjacent to LAYER= so this line never contains the `LAYER=<n> RESULT=<st>`
    # substring the summary-cell assertions pin (belt-and-braces; it is on stderr, they read stdout).
    printf 'FAILURE_TRIAGE LAYER=%d LOG=%s STDERR_LOG=%s RESULT=%s\n' \
      "$n" "$triage_log" "$triage_errlog" "$st" >&2
    printf '    read the log(s) to triage; do NOT re-run the layer to capture detail — it is already there.\n' >&2
    if [[ $n -eq $LAYER_MAX ]]; then
      printf '    (layer 7: env-test / browser-e2e sublogs + failure map in docs/runbooks/devloop-validation.md §6.7)\n' >&2
    fi
    # Layer 3: name the exact failed guard(s) from the structured, closed-vocabulary token (a list
    # of guard NAMES, which cannot carry a secret), in preference to grepping raw log text.
    if [[ $n -eq 3 ]]; then
      triage_guards="$(parse_failed_guard_names "$triage_log")"
      [[ -n "$triage_guards" ]] && printf '    FAILED_GUARD_NAMES=%s\n' "$triage_guards" >&2
    fi
    # Optional teaser: a short preview of the failing lines. Pipeline order is load-bearing:
    # SANITIZE (strip ANSI CSI + control chars) -> MATCH (anchored) -> TRUNCATE per line -> head.
    # Sanitize FIRST and UNCONDITIONALLY: the colorized `FAILED:` line begins with the escape byte so
    # `^FAILED` matches nothing until stripped, and every layer log can carry third-party color
    # (cargo/nx/prettier/Playwright force color on CI) the run-guards/common.sh tty-gate does not
    # touch — the sanitizer is a property of the teaser, not a workaround for one emitter. `grep -a`
    # (@observability F5): treat the log as text so an odd byte can't make grep suppress the line.
    # BUDGET IS PER-LOG (@observability F3): take up to 2 lines from EACH of layer-N.log AND
    # layer-N.stderr.log — a single `head` over the concatenation lets a busy stdout log starve the
    # stderr-only detail (a guard-timeout `PRECONDITION_FAILURE:` line, MIXED_LANE), the exact
    # mixed-lane case the two-log pointer exists for. Each line is PREFIXED `    | ` so no echoed line
    # can begin STATUS=/REASON= and be re-collected by run-story.sh's escalation grep. NO
    # tail/head-of-log fallback on no-match: ${DEVLOOP_TMP} is 700-perm precisely because layer logs
    # may incidentally capture token-bearing env (_common.sh:40-43); a `|| tail -20` would exfiltrate
    # arbitrary log text and is a REGRESSION, not a usability fix. The `|| true` on EACH pipeline is
    # load-bearing under `set -euo pipefail`: a grep no-match (exit 1) or `head` SIGPIPE (141) would
    # otherwise abort layer-all BEFORE `exit "$final_exit"`, firing __gate2_emit_trap with the WRONG
    # rc (a true operator-lane exit 2 written as exit 1 in the Gate-2 verdict — misrouted triage) and
    # truncating this loop so later red layers get no directive. Same mechanism run-guards.sh
    # documents ("turning a single failure into a CI lie").
    triage_teaser="$(
      for __tlog in "$triage_log" "$triage_errlog"; do
        { LC_ALL=C sed 's/\x1b\[[0-9;?]*[A-Za-z]//g; s/[[:cntrl:]]//g' "$__tlog" 2>/dev/null \
            | grep -aE '^(FAILED:|VIOLATION|STATUS=FAIL|PRECONDITION_FAILURE:)' \
            | cut -c1-200 \
            | head -2; } || true
      done
    )"
    if [[ -n "$triage_teaser" ]]; then
      printf '%s\n' "$triage_teaser" | sed 's/^/    | /' >&2
    else
      printf '    (no teaser — read the log(s) above)\n' >&2
    fi
  fi
done

exit "$final_exit"
