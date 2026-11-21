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
# Every layer runs in its own session (bounded teardown, stale-group reap): the shared
# mechanism run-story.sh also uses. Self-contained (no _common.sh dependency).
source "${__here}/lang/_child_group.sh"
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

# derive_layer_all_exit [floor] — THE exit rule, shared by the normal end, the EXIT trap and
# the CI aggregate (one home; dry-reviewer). Sets:
#   layer_all_exit = max(status_to_exit_code(worst status), every recorded layer rc, floor)
#   layer_missing  = comma list of layers in layers_to_run with NO terminal record
# A layer counts as recorded only when it has BOTH a status AND a process rc (security C1:
# the loop assigns rc first, so a signal between the two can never leave a parsed OK with no
# rc). An unrecorded layer is UNKNOWN (exit 2) — never skipped — so a killed run can never
# derive 0. NOT-RUN is display-only (guardrail at its decl): skipped only under a fail-fast
# stop (stopped_early>0, where a failing layer already carries rc!=0); a NOT-RUN with no stop
# is an orchestrator bug and derives 2 without touching status_to_exit_code. TOTAL under
# `set -u`: every read is defaulted, so the EXIT trap can always call it.
layer_all_exit=2
layer_missing=""
# __derive_exit_failclosed [floor] — derive_layer_all_exit, or — if it fails — the fail-closed
# result (exit 2, MISSING=?) with a fixed loud token. The ONE wrapper both the EXIT trap and
# derive_gate2 call, so they can never disagree on a failed derivation.
__derive_exit_failclosed() {
  derive_layer_all_exit "${1:-0}" && return 0
  layer_all_exit=2
  layer_missing="?"
  printf 'WARN GATE2_DERIVE_FAILED — the exit derivation itself failed; verdict forced FAIL (MISSING=?)\n' >&2
}
derive_layer_all_exit() {
  local floor="${1:-0}" n st rc mapped worst_rc=0 missing=""
  for n in "${layers_to_run[@]}"; do
    st="${layer_status[$n]:-}"
    rc="${layer_rc[$n]:-}"
    if [[ "$st" == "$NOT_RUN" ]]; then
      # Display-only under a fail-fast stop; anywhere else an un-run layer is simply missing.
      (( ${stopped_early:-0} > 0 )) && continue
      missing="${missing:+${missing},}${n}"
      continue
    fi
    if [[ -z "$st" || -z "$rc" ]]; then
      missing="${missing:+${missing},}${n}"
      continue
    fi
    (( rc > worst_rc )) && worst_rc=$rc
  done
  # compute_layer_totals reads an unrecorded status as UNKNOWN (top rank → exit 2).
  compute_layer_totals
  mapped=$(status_to_exit_code "$total_result") || mapped=2
  [[ -n "$missing" ]] && (( mapped < 2 )) && mapped=2
  (( mapped > worst_rc )) && worst_rc=$mapped
  (( floor > worst_rc )) && worst_rc=$floor
  layer_all_exit=$worst_rc
  layer_missing="$missing"
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
#   * every layer 1..LAYER_MAX must appear EXACTLY once across all summaries — or, with
#     `--layers L,...` beside `--aggregate`, exactly the listed layers and no other (a
#     check over a subset of the jobs, e.g. ci.yml's non-required Layer 7 jobs; the
#     required `Test Suite` always aggregates the whole pipeline);
#   * any violation, or no summary at all, is PRECONDITION_FAILURE (exit 2) with a REASON.
# Then TOTAL_RESULT = worst status, exit = max(status_to_exit_code(TOTAL), every RC) — the
# same aggregation and observed-rc FLOOR as a single invocation.
aggregate_shard_summaries() {
  local dir="$1" f line n status dur rc shard suite
  local -a files=()
  # Layer LAYER_MAX may be SPLIT across shards by suite (DEVLOOP_LAYER7_SUITE): one line per
  # suite, each carrying SUITE=. suite_shard maps suite -> the shard that ran it; split_seen
  # marks that layer LAYER_MAX arrived split (so a whole-layer line beside it is a duplicate).
  local -A suite_shard=()
  local split_seen=0
  local -A agg_scope=()
  for n in "${layers_to_run[@]}"; do agg_scope[$n]=1; done
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
      if [[ ! "$line" =~ ^LAYER=([1-9][0-9]*)\ RESULT=([A-Z/_-]+)\ DURATION=([0-9]+)\ RC=([0-9]+)(\ SUITE=([a-z0-9-]+))?$ ]]; then
        agg_fail malformed-line "${f}: unparseable line ${line@Q}"; return 2
      fi
      n="${BASH_REMATCH[1]}"; status="${BASH_REMATCH[2]}"; dur="${BASH_REMATCH[3]}"; rc="${BASH_REMATCH[4]}"
      suite="${BASH_REMATCH[6]}"
      if [[ "$status" != "UNKNOWN" && "$(__status_rank "$status")" -ge "$(__status_rank UNKNOWN)" ]]; then
        agg_fail unknown-status "${f}: layer ${n} RESULT=${status} is not a pipeline status"; return 2
      fi
      if (( n > LAYER_MAX )); then
        agg_fail layer-out-of-range "${f}: layer ${n} is outside 1..${LAYER_MAX}"; return 2
      fi
      if [[ -z "${agg_scope[$n]:-}" ]]; then
        agg_fail layer-out-of-scope "${f}: layer ${n} is not one of the aggregated layers (${layers_to_run[*]})"; return 2
      fi
      if [[ -n "$suite" ]]; then
        # A suite-split layer-LAYER_MAX line: merge it (worst status, max rc, max duration —
        # the suites ran in parallel jobs) instead of rejecting the second one as a duplicate.
        if (( n != LAYER_MAX )); then
          agg_fail malformed-line "${f}: SUITE= is only valid on layer ${LAYER_MAX} (got it on layer ${n})"; return 2
        fi
        if ! layer7_suite_known "$suite"; then
          agg_fail unknown-suite "${f}: layer ${n} SUITE=${suite} is not one of: ${LAYER7_SUITES[*]}"; return 2
        fi
        if [[ -n "${suite_shard[$suite]:-}" ]]; then
          agg_fail duplicate-suite "layer ${n} suite ${suite} reported by both ${suite_shard[$suite]} and ${shard}"; return 2
        fi
        if [[ -n "${layer_status[$n]:-}" && "$split_seen" -eq 0 ]]; then
          agg_fail duplicate-layer "layer ${n} reported whole by ${layer_shard[$n]} and as suite ${suite} by ${shard}"; return 2
        fi
        suite_shard[$suite]="$shard"
        if (( split_seen )); then
          layer_status[n]="$(aggregate_worst_status "${layer_status[$n]}" "$status")"
          (( dur > layer_dur[n] )) && layer_dur[n]="$dur"
          (( rc > layer_rc[n] )) && layer_rc[n]="$rc"
          layer_shard[$n]="${layer_shard[$n]}+${shard}"
        else
          layer_status[n]="$status"; layer_dur[n]="$dur"; layer_rc[n]="$rc"; layer_shard[$n]="$shard"
          split_seen=1
        fi
        continue
      fi
      if [[ -n "${layer_status[$n]:-}" ]]; then
        agg_fail duplicate-layer "layer ${n} reported by both ${layer_shard[$n]} and ${shard}"; return 2
      fi
      layer_status[$n]="$status"; layer_dur[$n]="$dur"; layer_rc[$n]="$rc"; layer_shard[$n]="$shard"
    done < "$f"
  done
  # A split layer LAYER_MAX is complete only when EVERY suite was run by some shard: a suite
  # dropped from the CI partition must fail here, never read as a green Layer 7.
  if (( split_seen )); then
    local s missing_suites=""
    for s in "${LAYER7_SUITES[@]}"; do
      [[ -n "${suite_shard[$s]:-}" ]] || missing_suites="${missing_suites:+${missing_suites},}${s}"
    done
    [[ -z "$missing_suites" ]] || { agg_fail missing-suite "layer ${LAYER_MAX} suite(s) ${missing_suites} reported by no shard — a suite-split layer ${LAYER_MAX} must cover: ${LAYER7_SUITES[*]}"; return 2; }
  fi
  local missing=""
  for n in "${layers_to_run[@]}"; do
    [[ -n "${layer_status[$n]:-}" ]] || missing="${missing:+${missing},}${n}"
  done
  [[ -z "$missing" ]] || { agg_fail missing-layer "layer(s) ${missing} reported by no shard — the CI partition must cover ${layers_to_run[*]}"; return 2; }

  # Same totals and the same exit rule as one invocation (derive_layer_all_exit: max(mapped
  # status, every RC)); every layer is recorded by here — the agg_fail refusals above
  # returned first with their own REASON tokens.
  local worst_rc
  derive_layer_all_exit 0
  worst_rc=$layer_all_exit

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
#   2. A shortened run never writes, removes or overwrites a Gate-2 verdict (its EXIT trap
#      tears layers down but skips the verdict — __authority=0). A missing verdict is fail-closed — a commit still needs a full layer-all.sh —
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
#     because a `--layers` run never writes, removes or overwrites a Gate-2 verdict (its
#     EXIT trap skips the verdict — __authority=0); its output is the shard summary file,
#     which only the aggregate reads. Local users keep `--max-layer`.
#   * `--aggregate` runs no layer and writes no verdict; it only reads summary files.
#     `--aggregate DIR --layers L,...` narrows it to exactly those layers (see
#     aggregate_shard_summaries); it is the one pairing of the modes allowed.
layer_lo=1
layer_hi="$LAYER_MAX"
layers_arg=""
aggregate_dir=""
mode="full"
aggregate_subset=0
set_mode() {  # one mode per invocation; --max-layer / --layers / --aggregate are exclusive,
              # except --aggregate with --layers (a subset aggregate), in either order
  if [[ "$mode" == "aggregate" && "$1" == "shard" ]] || [[ "$mode" == "shard" && "$1" == "aggregate" ]]; then
    mode="aggregate"; aggregate_subset=1; return
  fi
  if [[ "$mode" != "full" && "$mode" != "$1" ]]; then
    printf 'PRECONDITION_FAILURE: --max-layer, --layers and --aggregate are mutually exclusive (except --aggregate DIR --layers L,...).\n' >&2
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
    *) printf 'PRECONDITION_FAILURE: unknown argument %q — layer-all.sh accepts only --max-layer N, --layers L,... (CI), --aggregate DIR, or --aggregate DIR --layers L,....\n' "$1" >&2; exit 2 ;;
  esac
  shift
done
is_layer_number() { [[ "$1" =~ ^[1-9][0-9]*$ ]] && (( $1 >= 1 && $1 <= LAYER_MAX )); }
# Parse $layers_arg (ascending, distinct, comma-separated 1..LAYER_MAX) into layers_to_run,
# or refuse. Shared by a shard (--layers) and a subset aggregate (--aggregate --layers).
parse_layers_arg() {
  local n prev=0
  IFS=',' read -r -a __shard_list <<< "$layers_arg"
  for n in "${__shard_list[@]}"; do
    if ! is_layer_number "$n" || (( n <= prev )); then
      printf 'PRECONDITION_FAILURE: --layers must be ascending, distinct integers 1..%s, comma-separated (got %q). REASON=shard-layer-list-invalid\n' "$LAYER_MAX" "$layers_arg" >&2
      exit 2
    fi
    layers_to_run+=("$n"); prev=$n
  done
  (( ${#layers_to_run[@]} )) || { printf 'PRECONDITION_FAILURE: --layers is empty. REASON=shard-layer-list-invalid\n' >&2; exit 2; }
}

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
    parse_layers_arg
    shard_mode=1
    # CI reads the summary's path from here (the upload step's `path:`), so the file name
    # and directory live only in this script: emitted up front, before any layer can fail.
    if [[ -n "${GITHUB_OUTPUT:-}" ]]; then
      printf 'summary-path=%s\n' "${DEVLOOP_TMP}/${SHARD_SUMMARY_FILE}" >> "$GITHUB_OUTPUT"
    fi
    ;;
  aggregate)
    if (( aggregate_subset )); then
      parse_layers_arg
    else
      for (( n = 1; n <= LAYER_MAX; n++ )); do layers_to_run+=("$n"); done
    fi
    ;;
esac
layer_hi="${layers_to_run[-1]}"

# --- Layer-7 suite selector (DEVLOOP_LAYER7_SUITE; scripts/layer7.sh header §SUITE SELECTOR) ---
# Passed to layer7.sh by plain environment inheritance, like every other layer knob
# (DEVLOOP_ENV_TEST_TIMEOUT, ...); this script adds no flag for it. It is a CI-SHARD-ONLY
# narrowing of Layer 7, so it is refused here in every mode that could stand for the whole
# pipeline: a full run (the Gate-2 authority — a verdict over half of Layer 7 would be a
# forged PASS) and a --max-layer fast lane. In shard mode it must name a known suite AND the
# shard must include layer LAYER_MAX — a selector with nothing to select is a CI wiring bug,
# never silently ignored. The shard records it as SUITE= on its layer-LAYER_MAX summary line,
# and --aggregate (which runs no layer, so ignores the variable) requires those fields to
# cover LAYER7_SUITES exactly.
layer7_suite="${DEVLOOP_LAYER7_SUITE:-}"
if [[ -n "$layer7_suite" && "$mode" != "aggregate" ]]; then
  if [[ "$mode" != "shard" ]]; then
    printf 'PRECONDITION_FAILURE: DEVLOOP_LAYER7_SUITE=%q runs only part of Layer %s, which is allowed only in a CI shard (--layers ...,%s); a full or --max-layer run always runs every suite — unset it. REASON=layer7-suite-outside-shard\n' "$layer7_suite" "$LAYER_MAX" "$LAYER_MAX" >&2
    exit 2
  fi
  if ! layer7_suite_known "$layer7_suite"; then
    printf 'PRECONDITION_FAILURE: DEVLOOP_LAYER7_SUITE=%q is not a Layer-%s suite (valid: %s). REASON=layer7-suite-invalid\n' "$layer7_suite" "$LAYER_MAX" "${LAYER7_SUITES[*]}" >&2
    exit 2
  fi
  if [[ "$layer_hi" != "$LAYER_MAX" ]]; then
    printf 'PRECONDITION_FAILURE: DEVLOOP_LAYER7_SUITE=%s is set but this shard (--layers %s) does not run layer %s. REASON=layer7-suite-without-layer%s\n' "$layer7_suite" "$layers_arg" "$LAYER_MAX" "$LAYER_MAX" >&2
    exit 2
  fi
fi

# The per-layer accumulators (layer_status / layer_dur / layer_rc) are declared at the top
# ("One result model"), i.e. BEFORE the EXIT trap installs, so derive_layer_all_exit and
# emit_gate2_verdict's namerefs always see existing (possibly-empty) arrays, even on an early
# exit that fires before the layer loop populates them.
final_exit=0
stopped_early=0   # >0 = the layer at which interactive fail-fast stopped the run (0 = ran all)

# NOT_RUN — DISPLAY-ONLY marker for layers that never ran under interactive fail-fast.
# GUARDRAIL (@security): NOT_RUN must NEVER be added to _common.sh __status_rank or given an
# arm in status_to_exit_code. It is deliberately kept OUT of aggregation: the aggregation loop
# `continue`s past it and status_to_exit_code is never called on it, so an un-run layer can
# neither demote nor inflate TOTAL_RESULT / LAYER_ALL_EXIT. The `*)` fail-closed backstops in
# __status_rank (top rank) and status_to_exit_code (exit 2) make an accidental leak LOUD — but a
# well-meaning `NOT-RUN → 0` arm would silently reopen the fail-fast GATE2=PASS-forgery vector
# (a stopped-early run exiting 0 over layers that never ran). Do not "complete the enum".
readonly NOT_RUN="NOT-RUN"

# --aggregate runs no layer and writes no verdict: dispatch it before the verdict trap.
if [[ "$mode" == "aggregate" ]]; then
  aggregate_shard_summaries "$aggregate_dir"
  exit $?
fi

# --- Run lifecycle: signals, teardown, the Gate-2 verdict ---------------------------------
# (devloop 2026-11-21-killed-run-gate2-pass; runbook §8.5 "Verdict lifecycle".)
#
# ROOT CAUSE this replaces: the EXIT trap used to write GATE2 from `$?`. When bash dies of a
# fatal signal it still runs the EXIT trap, but `$?` there is the last COMPLETED command's
# status (0), not 128+N — a run SIGTERMed mid-Layer-7 wrote GATE2=PASS LAYER_ALL_EXIT=0
# with no LAYER 7 line while the process exited 143; an uncatchable SIGKILL left the
# previous run's PASS on disk. Now:
#   * every layer runs in its OWN session and is waited on interruptibly, so the TERM/INT/
#     HUP trap below fires at once (a trap on a foreground pipeline is deferred until the
#     layer ends — minutes on Layer 7) and exits 128+N;
#   * the EXIT trap tears the in-flight layer's whole session down (bounded TERM→KILL,
#     strictly inside run-story's outer grace) — nothing outlives layer-all;
#   * the verdict is DERIVED from the layer record (derive_gate2 → derive_layer_all_exit:
#     an unrecorded layer is UNKNOWN), never from `$?`; an untrapped fatal signal (`$?`=0)
#     still derives FAIL;
#   * an authority run replaces any previous verdict with REASON=in-progress BEFORE Layer 1,
#     so an uncatchable death leaves "the last run never finished", never an older PASS;
#   * an flock held for the run's lifetime refuses a concurrent run in the same DEVLOOP_TMP,
#     and lets the next run prove an uncatchably-killed owner dead and reap its orphaned
#     layer session (identity-checked).
__signal="none"        # TERM | INT | HUP once the signal trap fired
__loop_started=0       # the layer loop began
__run_completed=0      # the script reached its final `exit` — the ONLY completion evidence
__owns_lock=0          # this run holds the DEVLOOP_TMP lock (else the trap touches nothing)
__gate2_armed=0        # the run-start verdict is on disk; the trap writes the exit verdict
__inflight_pgid=""     # the running layer's session id ("" = none)
__inflight_reaped=0    # 1 once `wait` reaped that session's leader (its bare pid may be recycled)
__inflight_layer=""
__derived=0            # the EXIT trap already derived (derive_gate2 reuses it, never re-derives)
__authority=0
(( ! fast_lane && ! shard_mode )) && __authority=1
__lock_file="${DEVLOOP_TMP}/layer-all.lock"
__inflight_record="${DEVLOOP_TMP}/layer-all.inflight"
__snap_root="${DEVLOOP_TMP}/gate2-snapshot"
gate2_verdict="FAIL"
gate2_reason="pipeline-incomplete"

# derive_gate2 <start|exit> [floor] — THE ONE HOME of the Gate-2 decision. The emitter only
# renders what this sets (gate2_verdict / gate2_reason; layer_all_exit / layer_missing via
# derive_layer_all_exit). Fail-closed: every path that is not provably a complete, green,
# tree-stable run is FAIL. REASON is a closed set; when several causes hold, the FIRST of
# this precedence wins:
#   pipeline-incomplete      — the run did not reach its final exit (incl. after the loop):
#                              a signal (SIGNAL= names a trapped one), an untrapped death, a
#                              set -e abort, a pre-loop refusal (MISSING=<all>), or a layer
#                              with no record (incl. a NOT-RUN layer without a fail-fast stop)
#   layer-failed             — complete record, LAYER_ALL_EXIT != 0 (incl. a fail-fast stop)
#   tree-snapshot-failed     — green run, but the exit snapshot could not be taken
#   tree-changed-during-run  — green run, but the tree at exit != the tree at start
#   none                     — PASS
#   in-progress              — the `start` phase only: the run-start verdict
# The exit snapshot is taken ONLY on an otherwise-green run (it cannot change a FAIL, and
# hashing the tree inside a signal trap would only widen the window before a reaper's KILL).
derive_gate2() {
  local phase="$1" floor="${2:-0}" start_id end_id
  gate2_verdict="FAIL"
  if [[ "$phase" == "start" ]]; then
    gate2_reason="in-progress"
    return 0
  fi
  if (( ! __derived )); then __derive_exit_failclosed "$floor"; fi
  if __run_incomplete; then
    gate2_reason="pipeline-incomplete"
  elif (( layer_all_exit != 0 )); then
    gate2_reason="layer-failed"
  elif ! { mkdir -m 700 "${__snap_root}/end" 2>/dev/null && gate2_snapshot_worktree "${__snap_root}/end" 2>>"${DEVLOOP_TMP}/gate2-emit.stderr.log"; }; then
    gate2_reason="tree-snapshot-failed"
  else
    start_id="$(gate2_snapshot_identity "${__snap_root}/start")"
    end_id="$(gate2_snapshot_identity "${__snap_root}/end")"
    if [[ -z "$start_id" || "$start_id" != "$end_id" ]]; then
      gate2_reason="tree-changed-during-run"
      printf 'WARN GATE2_TREE_CHANGED_DURING_RUN PATHS=%s — verdict will not bind; re-run before commit (usual cause: Layer 2 fmt APPLY — run ./scripts/layer-fast.sh, then ./scripts/layer-all.sh)\n' \
        "$(gate2_snapshot_changed_paths "${__snap_root}/start" "${__snap_root}/end" 5)" >&2
    else
      gate2_verdict="PASS"
      gate2_reason="none"
    fi
  fi
}

# __run_incomplete — THE incompleteness predicate (verdict REASON and the stderr
# PIPELINE_INCOMPLETE line both read it): the script did not reach its final `exit` (a signal,
# an untrapped death or a `set -e` abort ANYWHERE — before, inside or after the layer loop), a
# signal arrived, or a layer has no terminal record. Call after a derive.
__run_incomplete() {
  (( ! __run_completed )) || [[ "$__signal" != "none" || -n "$layer_missing" ]]
}

# __gate2_write — render the current derive_gate2 result through the emitter, from the
# START snapshot. Emit failures are logged, never change the exit code.
__gate2_write() {
  local exit_field="$1"
  if ! emit_gate2_verdict "${__snap_root}/start" "$gate2_verdict" "$gate2_reason" "$exit_field" \
        "$__signal" "$layer_missing" layer_status layer_dur 2>>"${DEVLOOP_TMP}/gate2-emit.stderr.log"; then
    printf 'WARN gate2: verdict emit failed (see %s/gate2-emit.stderr.log)\n' "${DEVLOOP_TMP}" >&2
    return 1
  fi
}

# __layer_all_on_signal <NAME> <N> — the ONE handler for TERM/INT/HUP: record, exit 128+N
# (the EXIT trap then sees `$?`=128+N and tears down). Prompt because layer-all is always in
# an interruptible `wait` while a layer runs.
__layer_all_on_signal() {
  __signal="$1"
  trap - TERM INT HUP
  exit $(( 128 + $2 ))
}

# __layer_all_exit_trap — every exit path of every layer-running mode. Total under
# `set -euo pipefail`: errexit off first; every step's failure is ignored so the next runs.
# ORDER (deliberate): tear the in-flight layer down FIRST, so the exit snapshot and the
# verdict follow its death; that delays the emit by at most the inner grace
# (CHILD_GROUP_INNER_POLLS × CHILD_GROUP_POLL_SECS) — a reaper KILL landing inside that
# window leaves the run-start REASON=in-progress verdict, which is correct.
__layer_all_exit_trap() {
  local rc=$?
  set +e
  trap - EXIT TERM INT HUP
  if (( ! __owns_lock )); then exit "$rc"; fi   # concurrent-run refusal: touch nothing
  # `${__inflight_pgid:-$__CHILD_PID}`: a signal landing between child_group_spawn and the
  # __inflight_pgid assignment must not orphan the just-spawned layer. Default mode (bare-pid
  # fallback) ONLY while the leader is our unreaped child; once `wait` reaped it
  # (__inflight_reaped=1 — the post-wait reap window), group-only: its pid may be recycled.
  local inflight="${__inflight_pgid:-${__CHILD_PID:-}}" mode=""
  (( __inflight_reaped )) && mode="group-only"
  if [[ -n "$inflight" ]]; then
    kill_child_group "$CHILD_GROUP_INNER_POLLS" "$inflight" $mode
    __inflight_pgid=""
  fi
  rm -f "$__inflight_record"
  __derive_exit_failclosed "$rc"
  __derived=1
  if (( __loop_started )) && __run_incomplete; then
    printf 'PIPELINE_INCOMPLETE MISSING=%s IN_FLIGHT=%s SIGNAL=%s LAYER_ALL_EXIT=%s\n' \
      "${layer_missing:-none}" "${__inflight_layer:-none}" "$__signal" "$layer_all_exit" >&2
  fi
  if (( __authority && __gate2_armed )); then
    derive_gate2 exit "$rc"
    __gate2_write "$layer_all_exit"
  fi
  rm -rf "$__snap_root"
  exit "$rc"
}

# __reap_stale_layer_group — run with the lock held, before anything spawns. A record only
# survives an UNCATCHABLE death of its owner (the trap removes it on every other exit), and
# holding the lock proves that owner is gone. Reap the recorded session only if its leader
# is still the SAME process (start time + script path): a pid recycled after a container
# restart or a long gap is never signalled.
__reap_stale_layer_group() {
  local key val owner="" layer="" pgid="" leader_start="" script="" recorded_at="" live cmdline
  [[ -f "$__inflight_record" ]] || return 0
  while IFS='=' read -r key val; do
    case "$key" in
      OWNER_PID) owner="$val" ;; LAYER) layer="$val" ;; PGID) pgid="$val" ;;
      LEADER_START) leader_start="$val" ;; SCRIPT) script="$val" ;; RECORDED_AT) recorded_at="$val" ;;
    esac
  done < "$__inflight_record"
  rm -f "$__inflight_record"
  if ! [[ "$pgid" =~ ^[1-9][0-9]*$ ]]; then
    printf 'STALE_LAYER_RECORD_DROPPED PGID=%s LAYER=%s REASON=identity-mismatch\n' "${pgid:-none}" "${layer:-none}" >&2
    return 0
  fi
  live="$(proc_starttime "$pgid")"
  if [[ -z "$live" ]] || ! child_group_alive "$pgid"; then
    # The leader is gone. If members of its group still live, they ARE that session's (Linux
    # never hands out a pid number still in use as a pgid), so reap them group-only — the
    # leader's bare pid is never signalled. An empty group: nothing to do.
    if kill -0 -- "-${pgid}" 2>/dev/null; then
      kill_child_group "$CHILD_GROUP_INNER_POLLS" "$pgid" group-only
      printf 'REAPED_STALE_LAYER_GROUP PGID=%s LAYER=%s OWNER_PID=%s RECORDED_AT=%s REASON=leader-gone\n' \
        "$pgid" "${layer:-none}" "${owner:-none}" "${recorded_at:-none}" >&2
    else
      printf 'STALE_LAYER_RECORD_DROPPED PGID=%s LAYER=%s REASON=leader-gone\n' "$pgid" "${layer:-none}" >&2
    fi
    return 0
  fi
  cmdline="$(tr '\0' ' ' < "/proc/${pgid}/cmdline" 2>/dev/null || true)"
  if [[ "$live" != "$leader_start" || -z "$script" || "$cmdline" != *"$script"* ]]; then
    printf 'STALE_LAYER_RECORD_DROPPED PGID=%s LAYER=%s REASON=identity-mismatch\n' "$pgid" "${layer:-none}" >&2
    return 0
  fi
  kill_child_group "$CHILD_GROUP_INNER_POLLS" "$pgid" group-only   # not our child
  printf 'REAPED_STALE_LAYER_GROUP PGID=%s LAYER=%s OWNER_PID=%s RECORDED_AT=%s\n' \
    "$pgid" "${layer:-none}" "${owner:-none}" "${recorded_at:-none}" >&2
}

trap '__layer_all_on_signal TERM 15' TERM
trap '__layer_all_on_signal INT 2' INT
trap '__layer_all_on_signal HUP 1' HUP
trap '__layer_all_exit_trap' EXIT

for __tool in setsid flock; do
  command -v "$__tool" >/dev/null 2>&1 || {
    printf 'PRECONDITION_FAILURE: %s (util-linux) not found — layer-all.sh runs each layer in its own session and holds a run lock. Install util-linux. REASON=util-linux-missing\n' "$__tool" >&2
    exit 2
  }
done

# The run lock (every layer-running mode). Taken FIRST: a refused run touches nothing — no
# snapshot, no verdict, no kill. The holder line is display-only.
exec {__lock_fd}>>"$__lock_file"
if ! flock -n "$__lock_fd"; then
  printf 'PRECONDITION_FAILURE: another layer-all.sh holds %s (%s) — runs sharing a DEVLOOP_TMP would clobber each other'"'"'s logs, snapshot and verdict (any mode, incl. a fast lane beside an authority run). Wait for it or stop it (kill -0 <PID> to check). REASON=concurrent-layer-all\n' \
    "$__lock_file" "$(tr -d '\n' < "$__lock_file" 2>/dev/null || true)" >&2
  exit 2
fi
__owns_lock=1
printf 'PID=%s STARTED_AT=%s MODE=%s\n' "$$" "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$mode" > "$__lock_file"

# Authority runs: the previous verdict goes FIRST — before the stale reap, the snapshot or
# anything else that could be interrupted — so no window exists in which this run can die and
# leave an older PASS readable as "the last run". (Replaced below by the run-start verdict.)
if (( __authority )); then
  rm -f "$GATE2_VERDICT_FILE" 2>/dev/null || true
  if [[ -e "$GATE2_VERDICT_FILE" || -L "$GATE2_VERDICT_FILE" ]]; then
    printf 'PRECONDITION_FAILURE: could not remove the previous verdict %s — it would survive this run. Remove it by hand. REASON=gate2-verdict-invalidate-failed\n' "$GATE2_VERDICT_FILE" >&2
    exit 2
  fi
fi

__reap_stale_layer_group

# Authority runs only: snapshot the tree, then replace any previous verdict with the
# run-start one. Fast lane and shard never write, remove or overwrite a verdict.
if (( __authority )); then
  rm -rf "$__snap_root"
  if ! mkdir -m 700 "$__snap_root" 2>/dev/null || ! mkdir -m 700 "${__snap_root}/start" \
      || ! gate2_snapshot_worktree "${__snap_root}/start" 2>>"${DEVLOOP_TMP}/gate2-emit.stderr.log"; then
    rm -f "$GATE2_VERDICT_FILE"
    printf 'PRECONDITION_FAILURE: could not snapshot the work tree for the Gate-2 verdict (see %s/gate2-emit.stderr.log) — not a git work tree, or git failed. REASON=gate2-snapshot-failed\n' "${DEVLOOP_TMP}" >&2
    exit 2
  fi
  derive_gate2 start
  if ! __gate2_write none \
      || [[ ! -f "$GATE2_VERDICT_FILE" || -L "$GATE2_VERDICT_FILE" ]] \
      || [[ "$(gate2_verdict_get REASON "$GATE2_VERDICT_FILE")" != "in-progress" ]]; then
    rm -f "$GATE2_VERDICT_FILE" 2>/dev/null || true   # judged by the check below, not by errexit
    if [[ -e "$GATE2_VERDICT_FILE" || -L "$GATE2_VERDICT_FILE" ]]; then
      printf 'PRECONDITION_FAILURE: could not replace %s with the run-start verdict — a previous verdict would survive this run. Remove it by hand. REASON=gate2-verdict-invalidate-failed\n' "$GATE2_VERDICT_FILE" >&2
      exit 2
    fi
  fi
  __gate2_armed=1
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
  printf 'CI_SHARD=1 LAYERS=%s%s — runs these layers from scratch; no verdict emitted; the Test Suite job aggregates every shard (--aggregate).\n' "$layers_arg" "${layer7_suite:+ LAYER7_SUITE=${layer7_suite}}" >&2
fi

proc_starttime_into __owner_start "$$"
for n in "${layers_to_run[@]}"; do
  printf -v start '%(%s)T' -1   # builtin, no fork (see the spawn→wait note); bash >= 4.2
  # observability O3: atomic stderr append redirect (no process-sub race with stdout tee).
  # Capture the LAYER's real exit code (PIPESTATUS[0], NOT tee's). set +e/-e around the
  # pipeline so a non-zero layer doesn't abort the loop (replaces the old `if ! …` guard).
  # Each layer runs in its OWN session (child_group_spawn → setsid), waited on
  # INTERRUPTIBLY, so the TERM/INT/HUP trap fires at once and the EXIT trap can tear the
  # layer's whole tree down. stdout is tee'd to layer-N.log, stderr appended to
  # layer-N.stderr.log (observability O3), and the exit status is the LAYER's
  # (PIPESTATUS[0]), not tee's. stdin is /dev/null (the layer no longer owns the tty) and the
  # run-lock fd is closed in the child (an orphan must never hold the lock).
  __loop_started=1
  __inflight_layer=$n
  __inflight_reaped=0   # reset BEFORE spawning: the new leader is an unreaped child from birth
  set +e
  # Known residual: if the layer process dies on its own while a child of it still holds its
  # stdout, tee (and so this wait) stays open until that child exits — as before this design;
  # a signal to layer-all still tears the whole session down (runbook §8.5).
  # tee IGNORES SIGTERM (SIG_IGN survives exec): a teardown TERMs the whole session, and tee
  # must keep draining so the killed layer's own last words — its lifecycle's
  # `STATUS=UNKNOWN REASON=layer-incomplete` — reach layer-N.log instead of the log ending on
  # a finished child's STATUS=OK. tee still ends at EOF, or at the teardown's bounded KILL.
  # The teardown (kill_child_group) polls the whole SESSION, not this wrapper, so the layer's
  # own TERM/EXIT cleanup gets its full grace even though the wrapper dies on TERM at once.
  child_group_spawn bash -c '"$1" 2>>"$2" | (trap "" TERM; exec tee "$3"); exit "${PIPESTATUS[0]}"' _ \
        "${layer_script_dir}/layer${n}.sh" \
        "${DEVLOOP_TMP}/layer-${n}.stderr.log" "${DEVLOOP_TMP}/layer-${n}.log" \
        </dev/null {__lock_fd}>&-
  __inflight_pgid="$__CHILD_PID"
  # From spawn to `wait`: builtins only, no `$(…)` — bash 5.2.x can lose a trapped signal
  # that lands while a command substitution runs (see _child_group.sh proc_starttime_into).
  proc_starttime_into __leader_start "$__inflight_pgid"
  TZ=UTC0 printf -v __recorded_at '%(%Y-%m-%dT%H:%M:%SZ)T' -1
  printf 'OWNER_PID=%s\nOWNER_START=%s\nLAYER=%s\nPGID=%s\nLEADER_START=%s\nSCRIPT=%s\nRECORDED_AT=%s\n' \
    "$$" "$__owner_start" "$n" "$__inflight_pgid" "$__leader_start" \
    "${layer_script_dir}/layer${n}.sh" "$__recorded_at" > "$__inflight_record"
  printf 'LAYER_START LAYER=%d PGID=%s\n' "$n" "$__inflight_pgid" >&2
  child_group_wait "$__inflight_pgid"
  rc=$?
  __inflight_reaped=1   # the leader is reaped: from here only group signals (trap included)
  # Reap whatever the finished layer left in its session (normally nothing): a layer killed
  # on its own while the pipeline carries on must not leave a cargo/kubectl child running.
  # group-only: the leader was just reaped by `wait`, so its bare pid may be recycled.
  kill_child_group "$CHILD_GROUP_INNER_POLLS" "$__inflight_pgid" group-only
  __inflight_pgid=""
  __CHILD_PID=""
  __inflight_layer=""
  rm -f "$__inflight_record"
  set -e
  printf -v end '%(%s)T' -1
  dur=$((end - start))

  # Record the layer: rc FIRST, then status (security C1 — derive_layer_all_exit counts a
  # layer only when it has both, so a signal between the two can never leave a parsed OK
  # with no rc). The rc is the @operations FLOOR (task #56): a STATUS/rc mismatch (last
  # STATUS line OK, process non-zero) can never demote LAYER_ALL_EXIT below the real rc;
  # it also drives the ADR-0037 D8 directive on a lying `STATUS=OK; exit 1` layer.
  layer_rc[$n]=$rc
  # observability O4 + dry-reviewer (C): single source of truth for STATUS parsing.
  status=$(parse_status_line "${DEVLOOP_TMP}/layer-${n}.log")
  layer_status[$n]="${status:-UNKNOWN}"
  layer_dur[$n]=$dur

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
# The loop reached its end (a fail-fast stop included). Completion is NOT declared here:
# only reaching the final `exit` sets __run_completed, so a signal OR a `set -e` abort in the
# post-loop code is still pipeline-incomplete (never mislabeled layer-failed).

# Mark the layers that never ran under a fail-fast stop as NOT-RUN (DISPLAY-ONLY — see the
# NOT_RUN decl). Setting them explicitly means the `${…:-UNKNOWN}` reads below never fire for
# an un-run layer (UNKNOWN would wrongly inflate TOTAL_RESULT → exit 2). Loud greppable stop
# line so "layers N+1..LAYER_MAX missing" is never ambiguous with a truncated log.
if (( stopped_early > 0 )); then
  not_run_list=""
  for n in "${layers_to_run[@]}"; do
    (( n > stopped_early )) || continue
    layer_status[$n]="$NOT_RUN"
    layer_dur[$n]=0
    not_run_list="${not_run_list:+${not_run_list},}${n}"
  done
  echo "STOPPED_EARLY LAYER=${stopped_early} RESULT=${layer_status[$stopped_early]} NOT_RUN=${not_run_list}" >&2
  # Defensive (N3): fail-fast only ever breaks on rc!=0, so the derived exit is >0 by
  # construction here. A zero is an orchestrator bug that would let a stopped-early run report
  # success — a fixed greppable token so it's identifiable in a log and rides the §4 grep.
  derive_layer_all_exit 0
  if (( layer_all_exit == 0 )); then
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

# Authoritative process exit: derive_layer_all_exit (the ONE exit rule, shared with the EXIT
# trap and --aggregate) = max(status_to_exit_code(worst), every layer rc). The enum mapping
# (dry-reviewer F1) carries the SEMANTICS — the operator lane (Layer-7 PRECONDITION_FAILURE)
# reaches LAYER_ALL_EXIT=2 — and the rc FLOOR (@operations) means a STATUS/rc mismatch can
# never demote it below the real return code. Also sets total_result for the summary.
derive_layer_all_exit 0
final_exit=$layer_all_exit

print_layer_summary

# CI shard: hand this invocation's per-layer results to the Test Suite aggregate
# (--aggregate). One line per layer this shard ran, in the shape the aggregate parses.
if (( shard_mode )); then
  for n in "${layers_to_run[@]}"; do
    # SUITE= only on the layer-LAYER_MAX line of a suite-selected shard (validated above);
    # its absence means "every Layer-7 suite", which is what --aggregate assumes.
    suite_field=""
    if [[ -n "$layer7_suite" && "$n" -eq "$LAYER_MAX" ]]; then suite_field=" SUITE=${layer7_suite}"; fi
    printf 'LAYER=%d RESULT=%s DURATION=%s RC=%s%s\n' "$n" "${layer_status[$n]:-UNKNOWN}" "${layer_dur[$n]:-0}" "${layer_rc[$n]:-0}" "$suite_field"
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
    # otherwise abort layer-all BEFORE `exit "$final_exit"`, firing __layer_all_exit_trap with the WRONG
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

__run_completed=1
exit "$final_exit"
