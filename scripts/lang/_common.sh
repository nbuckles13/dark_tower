#!/usr/bin/env bash
# _common.sh — sourced by every layer script, dispatcher, and per-language wrapper.
#
# Provides:
#   - DEVLOOP_TMP path + init_devloop_tmp (700-perm cache dir)
#   - emit_status / run_and_emit / parse_status_line   (STATUS= line primitives)
#   - aggregate_worst_status                            (worst-child precedence)
#   - layer_lifecycle_begin / tee_collect_statuses      (layer skeleton)
#   - color helpers
#
# Requires bash >= 4.0 (associative arrays, `mapfile`, lastpipe). macOS ships
# bash 3.2; mac devs install homebrew bash. Tripwire below exits loud if bash 3.x.
#
# Idempotent: safe to source multiple times.
set -euo pipefail
IFS=$'\n\t'

# Bash 4.0+ tripwire (code-reviewer Nit 1).
if [[ ${BASH_VERSINFO[0]:-0} -lt 4 ]]; then
  echo "_common.sh requires bash >= 4.0; got ${BASH_VERSION:-unknown}" >&2
  exit 2
fi

# Idempotent-source guard (code-reviewer addendum #4).
[[ -n "${__DEVLOOP_COMMON_SH:-}" ]] && return 0
readonly __DEVLOOP_COMMON_SH=1

# Enable lastpipe so the rightmost command of a pipeline runs in the current shell,
# not a subshell. Required for `tee_collect_statuses` to mutate __LAYER_STATUSES
# in the parent shell so __layer_lifecycle_end (EXIT trap) sees the collected values.
# Job control must be off (default in non-interactive shells; explicit here for safety).
set +m  # disable job control
shopt -s lastpipe

# -----------------------------------------------------------------------------
# Paths
# -----------------------------------------------------------------------------

# DEVLOOP_TMP — pipeline cache namespace.
# Default /tmp/devloop is distinct from ADR-0030's /tmp/devloop-{slug}/ namespace.
# 700 perms: layer logs may incidentally capture token-bearing env (a misconfigured
# cargo test printing $GITHUB_TOKEN, RUST_LOG=trace surfacing auth headers, etc.).
# Cheap to lock the dir; expensive to retrofit if a leak occurs.
DEVLOOP_TMP="${DEVLOOP_TMP:-/tmp/devloop}"

# Args: (none)
# Outputs: (none)
# Returns: 0 on success
init_devloop_tmp() {
  mkdir -p "$DEVLOOP_TMP"
  chmod 700 "$DEVLOOP_TMP"
}

# -----------------------------------------------------------------------------
# Color helpers
# -----------------------------------------------------------------------------

if [[ -t 1 ]]; then
  readonly DEVLOOP_RED='\033[0;31m'
  readonly DEVLOOP_GREEN='\033[0;32m'
  readonly DEVLOOP_YELLOW='\033[1;33m'
  readonly DEVLOOP_BLUE='\033[0;34m'
  readonly DEVLOOP_NC='\033[0m'
else
  readonly DEVLOOP_RED=''
  readonly DEVLOOP_GREEN=''
  readonly DEVLOOP_YELLOW=''
  readonly DEVLOOP_BLUE=''
  readonly DEVLOOP_NC=''
fi

# -----------------------------------------------------------------------------
# STATUS line primitives (dry-reviewer §3 + (C))
# -----------------------------------------------------------------------------

# Module flag: set to 1 the first time ANY status line is emitted (OK/FAIL/
# SKIPPED-*/N/A). The wrapper EXIT trap (install_wrapper_exit_trap) reads this to
# decide whether a wrapper died BEFORE emitting a STATUS line — the silent-skip-at-
# pipeline-edge case (task #50). Set inside emit_status (the single emit locus) so
# EVERY emission marks it, FAIL included: a normal `emit_status FAIL; exit 1` must
# NOT re-trigger the trap and clobber the real rc.
_status_emitted=0

# Emit a STATUS= line to stdout in the canonical format.
# Args: $1=status-enum  $2=reason-token-no-spaces
# Outputs: stdout="STATUS=<status> REASON=<reason>"
# Returns: 0
emit_status() {
  # Set the sentinel AFTER printf succeeds, not before (Lead + semantic-guard, task #50):
  # the sentinel means "a STATUS line was actually written". If printf is interrupted /
  # the write fails, the flag stays unset so the EXIT trap still fires and recovers with
  # STATUS=FAIL — fail-closed. Setting it before printf would risk a silent/garbled line.
  # This still satisfies the "no double-emit" guard: on the normal `emit_status FAIL;
  # return 1` path, printf completes → flag set → the trap sees it and stays silent.
  printf 'STATUS=%s REASON=%s\n' "$1" "$2"
  _status_emitted=1
}

# Wrapper EXIT-trap installer (task #50, change 1 — silent-skip-at-pipeline-edge).
# Call as a one-liner from each per-language VERB wrapper (rust/ts {compile,fmt,lint,
# test,audit} + proto {compile,fmt,lint,breaking}) right after sourcing _common.sh.
#
# Do NOT call from changed.sh predicates (they legitimately `exit 1` to mean "lang
# untouched" — a trap there would synthesize a false STATUS=FAIL) or from layer
# scripts (they own the layer_lifecycle_begin EXIT trap; a second trap would clobber it).
#
# Args: (none)
# Outputs: (none at install time)
# Returns: 0
install_wrapper_exit_trap() {
  trap '__wrapper_early_exit_trap' EXIT
}

# Internal: EXIT-trap handler for verb wrappers. If the wrapper reached EXIT WITHOUT
# emitting a STATUS line (set -e abort, an exit 1 in a helper, a crashing command
# before run_and_emit), emit STATUS=FAIL with the captured rc so the dispatcher
# parses a loud FAIL instead of an empty pipe (which would aggregate to UNKNOWN). If a
# status WAS already emitted (normal path, or explicit emit_status+exit), stay silent
# and preserve the wrapper's own exit code.
#
# CRITICAL: `rc=$?` MUST be the literal first statement — any prior command would
# overwrite $? (semantic-guard check 1). The handler ends in `exit "${rc:-1}"`, never
# `exit 0`, so a pre-emit crash can never be converted to a silent success (a trailing
# printf would otherwise leave $?=0).
__wrapper_early_exit_trap() {
  local rc=$?
  if [[ "${_status_emitted:-0}" != "1" ]]; then
    printf 'STATUS=FAIL REASON=wrapper-aborted-early-exit-%s\n' "$rc"
  fi
  trap - EXIT
  exit "${rc:-1}"
}

# Run a command, emit STATUS based on its exit code.
# Args: $1=reason-prefix  $@=command-and-args
# Outputs: stdout=command output, then STATUS line; stderr=command stderr
# Returns: 0 on OK, 1 on FAIL
run_and_emit() {
  local prefix="$1"; shift
  if "$@"; then
    emit_status OK "${prefix}-passed"
    return 0
  else
    emit_status FAIL "${prefix}-failed"
    return 1
  fi
}

# Parse the LAST STATUS= line from a log file; print just the enum value.
# Single source of truth for STATUS= line shape — used by layer-all.sh,
# verify-completion.sh, and (in Wave 2) CI YAML's grep.
# Args: $1=log-file
# Outputs: stdout=enum value (empty if no STATUS= line found)
# Returns: 0
parse_status_line() {
  grep '^STATUS=' "$1" 2>/dev/null | tail -n1 | sed -n 's/^STATUS=\([^ ]*\).*/\1/p'
}

# Parse the LAST STATUS= line from a log file; print just the REASON token.
# Sibling to parse_status_line — SPOT for REASON extraction (dry-reviewer item 3),
# so no caller (_dispatch.sh, __layer_lifecycle_end, the tests) re-implements an
# inline sed/grep for the reason. Task #50 needs the reason to drive the exit code,
# not just the enum.
# Args: $1=log-file
# Outputs: stdout=reason token (empty if no STATUS= line found)
# Returns: 0
parse_status_reason() {
  grep '^STATUS=' "$1" 2>/dev/null | tail -n1 | sed -n 's/^STATUS=[^ ]* REASON=\([^ ]*\).*/\1/p'
}

# CI-SENTINEL-LEAK runtime assertion (task #47, §J/C — security trust boundary).
# Single source of truth for the check, called from BOTH entry points that can run the
# always-run path: layer-all.sh (full pipeline) and layer3.sh (standalone). Two call
# sites are legitimate; the BODY must not be duplicated (a security control that could
# silently diverge). If the condition ever broadens (another sentinel var, a different
# CI detector), it changes here once.
#
# The audit-suppressions test seam reads override envs ONLY under DEVLOOP_TEST=1; the
# production path trusts that CI never sets that sentinel. ENFORCE it: if DEVLOOP_TEST
# leaks into the CI job env, the always-run check would honor ambient overrides
# repo-wide and silently. Catch it at the pipeline boundary, before any layer runs.
#
# Args: (none — reads GITHUB_ACTIONS / DEVLOOP_TEST from env)
# Outputs: on leak — STATUS line on stdout, CI-SENTINEL-LEAK explanation on stderr
# Returns: does NOT return on leak (exits 1); returns 0 when no leak.
assert_no_ci_sentinel_leak() {
  if [[ -n "${GITHUB_ACTIONS:-}" && -n "${DEVLOOP_TEST:-}" ]]; then
    printf 'STATUS=FAIL REASON=test-sentinel-set-in-ci\n'
    printf 'CI-SENTINEL-LEAK: DEVLOOP_TEST is set (=%q) in a CI job. The test sentinel must NEVER be set in CI — it would let the always-run audit-suppressions check honor ambient override envs (manifest/date/derived-path) repo-wide. Find and remove whatever exported DEVLOOP_TEST (workflow step, reusable action); do NOT unset-and-rerun blindly. See docs/runbooks/devloop-validation.md §6.3.\n' "${DEVLOOP_TEST}" >&2
    exit 1
  fi

  # Second sentinel (task #56): LAYER_SCRIPT_DIR is a LOCAL-ONLY test seam — layer-all.sh
  # substitutes stub layer scripts under it (gated on DEVLOOP_TEST=1). Reject its mere
  # PRESENCE in CI, INDEPENDENT of DEVLOOP_TEST: in CI a forged stub dir would turn the
  # whole pipeline green and forge the Gate-2 verdict (the authority skip-vector ci.yml
  # guards). Defense-in-depth — even if a future edit drops the DEVLOOP_TEST gate on the
  # seam, CI safety no longer depends on that gate staying intact.
  #
  # ASYMMETRY (deliberate — do NOT add a third clause "for symmetry", and do NOT delete
  # this one): LAYER_SCRIPT_DIR gets a CI presence-rejection because its threat is
  # CI-forgery (a forged stub dir turns the whole pipeline green). DEVLOOP_HELPER_SOCKET,
  # the other task-#56 test seam, gets NO CI clause and needs none — for two independent
  # reasons. (1) CI-INERT BY CONSTRUCTION: the DEVLOOP_HELPER_SOCKET override is DEVLOOP_TEST-
  # gated (layer7.sh seam block), and DEVLOOP_TEST can NEVER be set in CI (assert_no_ci_sentinel_leak
  # above hard-fails the pipeline on a leak) — so in CI the socket var always resolves to the
  # fixed canonical path; the override has zero CI effect, regardless of branch order. (NB:
  # the gate is socket-present-FIRST — layer7.sh:256 reads the socket before the GITHUB_ACTIONS
  # elif — so CI-inertness rests on the gate+sentinel, NOT on any ordering.) (2) Its only real
  # threat is a LOCAL silent-skip, which a CI-only clause cannot guard anyway; that is closed by
  # the layer7 GITHUB_ACTIONS DISCRIMINATOR (local + socket-absent → PRECONDITION_FAILURE exit 2,
  # loud — never a clean skip), NOT by the gate. That gate only prevents a test-seam redirect of
  # the socket PATH — it does not (and need not) close the genuine missing-helper case; the
  # discriminator does. So a CI clause here would guard nothing.
  if [[ -n "${GITHUB_ACTIONS:-}" && -n "${LAYER_SCRIPT_DIR:-}" ]]; then
    printf 'STATUS=FAIL REASON=layer-script-dir-set-in-ci\n'
    printf 'CI-SENTINEL-LEAK: LAYER_SCRIPT_DIR is set (=%q) in a CI job. It is a LOCAL-ONLY test seam that substitutes stub layer scripts; in CI it would let a forged stub dir turn the whole pipeline green and forge the Gate-2 verdict. Nothing legitimate sets it in CI — find and remove whatever exported it. See docs/runbooks/devloop-validation.md §6.3.\n' "${LAYER_SCRIPT_DIR}" >&2
    exit 1
  fi
  return 0
}

# -----------------------------------------------------------------------------
# STATUS aggregation (test §D + code-reviewer locked)
# -----------------------------------------------------------------------------

# Aggregate multiple STATUS values; print the worst per precedence.
#
# Precedence (task #52 inserted FAIL-MISSING-VERB; task #56 inserted SKIPPED-NO-CLUSTER
# + PRECONDITION_FAILURE):
#   UNKNOWN > FAIL-MISSING-VERB > PRECONDITION_FAILURE > FAIL > N/A > OK
#           > SKIPPED-NO-CLUSTER > SKIPPED-NO-DIFF > SKIPPED-NO-VERB
# Reasoning: if any child did real work and passed, the layer passed; otherwise
# the SKIPPED-* state is informative. SKIPPED-NO-CLUSTER (task #56) = "the verb APPLIES
# but a condition blocked it" (Layer 7 is always-run, so env-tests apply; a missing CI
# cluster blocked them — identical tier to SKIPPED-NO-DIFF) — exit 0, ranks BELOW OK so a
# sibling's real OK dominates and a green CI run reports TOTAL_RESULT=OK. N/A is a
# different tier: "the verb does not APPLY to this lang" (proto placeholder wrappers); it
# ranks above OK. FAIL means "this lang did real work and found a problem".
# PRECONDITION_FAILURE (task #56) means "the ENVIRONMENT the gate needs was unavailable"
# (Layer 7: cluster bring-up/health failed) — the OPERATOR lane; it ranks ABOVE FAIL but
# BELOW FAIL-MISSING-VERB: the Kind cluster is needed ONLY by Layer 7, so a cluster-down
# PRECONDITION does NOT invalidate layers 1-6, whereas a missing verb wrapper is a
# persistent pipeline-machinery defect — "broken machinery" outranks "transient env not
# ready" (@test's ladder; ordering = most-fundamental-machinery-problem first). UNKNOWN
# (dispatcher bug — no STATUS emitted) stays on top.
# Ranking FAIL-MISSING-VERB above OK is what closes the cross-lang-masking residual: a
# missing verb can no longer be hidden by a sibling's clean run (task #52, replaces the
# #50 reason-tiebreak + the audit-slice post-processor).
#
# Exit-code mapping (status_to_exit_code): OK / SKIPPED-* / N/A → 0; FAIL → 1;
# FAIL-MISSING-VERB / PRECONDITION_FAILURE / UNKNOWN → 2 (the §6 "investigate the
# script/environment" class — wiring faults + infra preconditions, the operator lane).
#
# Args: $@=zero-or-more STATUS enum values
# Outputs: stdout=worst status (OK if no args)
# Returns: 0
aggregate_worst_status() {
  if [[ $# -eq 0 ]]; then
    printf 'OK\n'
    return 0
  fi
  local s worst="$1"; shift
  for s in "$@"; do
    if [[ "$(__status_rank "$s")" -gt "$(__status_rank "$worst")" ]]; then
      worst="$s"
    fi
  done
  printf '%s\n' "$worst"
}

# Internal: numeric rank for precedence comparison.
__status_rank() {
  case "$1" in
    SKIPPED-NO-VERB)      printf '0\n' ;;
    SKIPPED-NO-DIFF)      printf '1\n' ;;
    SKIPPED-NO-CLUSTER)   printf '2\n' ;;  # exit-0 clean skip: CI has no devloop cluster — below OK (task #56)
    OK)                   printf '3\n' ;;
    N/A)                  printf '4\n' ;;
    FAIL)                 printf '5\n' ;;
    PRECONDITION_FAILURE) printf '6\n' ;;  # operator/infra lane — outranks FAIL, below the wiring fault (task #56)
    FAIL-MISSING-VERB)    printf '7\n' ;;  # wiring fault — outranks FAIL + PRECONDITION_FAILURE (task #52)
    UNKNOWN)              printf '8\n' ;;
    *)                    printf '8\n' ;;  # FAIL-CLOSED backstop: an enum NOT given its own arm
                                           # above (e.g. a future status, or a typo'd emit_status)
                                           # ranks at the TOP and so dominates every aggregation —
                                           # it can never be silently out-ranked by OK. The two
                                           # task-#56 enums (SKIPPED-NO-CLUSTER, PRECONDITION_FAILURE)
                                           # are EXPLICIT arms above precisely so they rank where
                                           # intended; the catch-all only ever fires on a wiring bug.
  esac
}

# Map a STATUS enum to its ADR-0033 §6 exit code (dry-reviewer F1).
# Single source of truth for status→exit-code mapping; replaces the duplicated
# case-statements that used to live in __layer_lifecycle_end and the dispatcher.
#
# Pure f(enum) (task #52, Lead ruling): the exit code is a function of the aggregate
# enum ALONE — no REASON is consulted. The #50 reason→exit coupling existed only to
# recover an exit code the rank-collided SKIPPED-NO-VERB enum could not express; with
# FAIL-MISSING-VERB as its own ranked enum, the enum carries the semantics directly and
# the 2nd `reason` arg is retired. FAIL-MISSING-VERB, PRECONDITION_FAILURE (task #56),
# and UNKNOWN are an EXPLICIT exit-2 arm (not the `*)` catch-all) so the
# investigate-script/environment intent is legible.
#
# Args: $1=STATUS enum value
# Outputs: stdout=exit code (0/1/2)
# Returns: 0 always (the exit code is on stdout)
status_to_exit_code() {
  local status="$1"
  case "$status" in
    OK|SKIPPED-NO-DIFF|SKIPPED-NO-VERB|SKIPPED-NO-CLUSTER|N/A) printf '0\n' ;;
    FAIL)                                                     printf '1\n' ;;
    PRECONDITION_FAILURE|FAIL-MISSING-VERB|UNKNOWN)           printf '2\n' ;;  # §6 investigate-script/env class
    *)                                                        printf '2\n' ;;  # FAIL-CLOSED backstop: an enum NOT
                                                                               # mapped above exits 2 (loud/investigate),
                                                                               # never 0 — so a forgotten or typo'd status
                                                                               # can never silently pass the pipeline. The
                                                                               # exit-0 (SKIPPED-NO-CLUSTER) and exit-2
                                                                               # (PRECONDITION_FAILURE) task-#56 enums are
                                                                               # EXPLICIT arms; this only fires on a bug.
  esac
}

# Among children sharing the WINNING enum, pick a representative real reason — so the
# stderr LAYER= / dispatcher-emit REASON field names an actual lang/verb cause rather
# than a generic "<status>-aggregate" token (observability's 3am anchor, P2). This is
# the FIRST child reason whose enum == the winner.
#
# Task #52: this no longer drives the exit code — status_to_exit_code is now pure
# f(enum), and all children sharing the winning enum map to the same exit code, so
# there is nothing to "tie-break" on. The helper survives purely for the human-readable
# stderr cause; it does NOT call status_to_exit_code (zero reason→exit coupling remains).
#
# Non-empty fallback (semantic-guard): if NO child reason matches the winning enum
# (shouldn't happen with 1:1 arrays, but guard the edge), emit a generic
# "<status>-aggregate" reason so the LAYER= line is never blank.
#
# Args: $1=winning enum; then alternating enum/reason pairs ($2=enum $3=reason ...)
# Outputs: stdout=a representative reason among children whose enum==$1
# Returns: 0
worst_reason_for_status() {
  local winner="$1"; shift
  local e r
  while [[ $# -ge 2 ]]; do
    e="$1"; r="$2"; shift 2
    if [[ "$e" == "$winner" && -n "$r" ]]; then
      printf '%s\n' "$r"
      return 0
    fi
  done
  printf '%s-aggregate\n' "$(printf '%s' "$winner" | tr '[:upper:]' '[:lower:]')"
}

# -----------------------------------------------------------------------------
# Layer lifecycle (dry-reviewer §1 + observability O2)
# -----------------------------------------------------------------------------

# Begin layer lifecycle: capture start time, init STATUS collector, install EXIT trap.
#
# CRITICAL (observability O2): the EXIT trap fires on `set -e` abort or signal,
# so the LAYER=... stderr line is GUARANTEED to emit — even when a child wrapper
# kills the layer mid-flight. 3am debug case: runbook reader always sees which
# layer failed and how long it ran.
#
# Args: $1=layer-num
# Outputs: deferred to __layer_lifecycle_end (stdout STATUS, stderr LAYER line)
# Returns: 0
layer_lifecycle_begin() {
  __LAYER_NUM="$1"
  __LAYER_START=$(date +%s)
  __LAYER_STATUSES=()
  # Parallel to __LAYER_STATUSES, index-aligned: __LAYER_REASONS[i] is the REASON of the
  # child whose enum is __LAYER_STATUSES[i]. Lets __layer_lifecycle_end surface the real
  # worst-child REASON on the stderr LAYER= line (observability P2) without overloading
  # __LAYER_STATUSES (which stays bare enums — locked tests + lastpipe contract preserved,
  # observability P1). Task #52: the exit code is pure f(enum), so this REASON drives only
  # the human-readable stderr cause, never the exit code.
  __LAYER_REASONS=()
  __LAYER_RESULT="UNKNOWN"
  # Export DEVLOOP_LAYER for child processes (dry-reviewer F3).
  # _get_base_ref.sh reads this to namespace the per-layer changed-files cache,
  # avoiding the duplicated `DEVLOOP_LAYER=N` per-line prefix on every layer-script
  # subprocess invocation.
  export DEVLOOP_LAYER="$1"
  trap '__layer_lifecycle_end' EXIT
}

# Per-step wall-clock for multi-phase layers (Layer 7's cluster bring-up phases). The layer
# captures a start with `$(layer_now)` and calls emit_step_duration when the step finishes.
# BOTH live here (the lifecycle helper) so layer bodies never call `date` directly — that is
# the _layer_skeleton.test.sh invariant ("lifecycle owns timestamps"); a layer doing its own
# per-step timing would otherwise trip the skeleton linter's `date +%s` rule. The emitted
# `LAYER=<n> STEP=<name> DURATION=<s>` line mirrors the lifecycle's LAYER= anchor shape with a
# STEP= discriminator, so an operator greps `DURATION=` once to see layer + per-step timing
# (observability). stderr, like the LAYER= summary line.
# Args: (none)  Outputs: stdout=unix seconds
layer_now() { date +%s; }
# Args: $1=step-name  $2=start-ts (from layer_now)
# Outputs: stderr=LAYER=<n> STEP=<name> DURATION=<secs>
emit_step_duration() {
  printf 'LAYER=%s STEP=%s DURATION=%s\n' "${__LAYER_NUM:-?}" "$1" "$(( $(date +%s) - $2 ))" >&2
}

# Stream stdin to stdout verbatim, side-effect __LAYER_STATUSES (enums) AND the parallel
# __LAYER_REASONS (reasons) with parsed STATUS values.
#
# The two arrays are pushed 1:1 in the SAME branch (semantic-guard check 3 — index
# alignment): every STATUS line appends its enum to __LAYER_STATUSES and its reason
# (or empty if the line carried none) to __LAYER_REASONS at the same index.
# __LAYER_STATUSES stays bare enums so the locked _common.test.sh:112-114 assertions +
# the lastpipe parent-shell mutation invariant are unchanged (observability P1).
#
# Args: (none — reads stdin)
# Outputs: stdout=verbatim copy
# Returns: 0
tee_collect_statuses() {
  local line
  while IFS= read -r line; do
    printf '%s\n' "$line"
    if [[ "$line" =~ ^STATUS=([^[:space:]]+)([[:space:]]+REASON=([^[:space:]]+))? ]]; then
      __LAYER_STATUSES+=("${BASH_REMATCH[1]}")
      __LAYER_REASONS+=("${BASH_REMATCH[3]}")
    fi
  done
}

# Internal: called from EXIT trap installed by layer_lifecycle_begin.
# Emits final STATUS= line on stdout, LAYER= line on stderr, exits with mapped code.
#
# REASON convention (code-reviewer Nit 2): "layer<n>-summary" — drops the
# redundant result-name echo since RESULT= and STATUS= already carry it.
# REASON should explain *why* this status; at the layer aggregation level
# the answer is just "summary of children". Single hyphen, no §6 collision.
__layer_lifecycle_end() {
  local end duration result reason worst_reason rc i pairs
  end=$(date +%s)
  duration=$((end - __LAYER_START))
  if [[ ${#__LAYER_STATUSES[@]} -eq 0 ]]; then
    result="$__LAYER_RESULT"
    worst_reason="no-child-statuses"
  else
    result=$(aggregate_worst_status "${__LAYER_STATUSES[@]}")
    # Build the (enum reason enum reason …) pair list from the index-aligned arrays,
    # then pick a representative reason among children whose enum == result. This drives
    # the stderr LAYER= REASON only (so the loud exit points at the real cause —
    # observability P2 — distinguishing FAIL-MISSING-VERB from UNKNOWN, both exit 2). The
    # exit code itself is pure f(result) below (task #52 — the enum carries the semantics).
    pairs=()
    for ((i = 0; i < ${#__LAYER_STATUSES[@]}; i++)); do
      pairs+=("${__LAYER_STATUSES[i]}" "${__LAYER_REASONS[i]:-}")
    done
    worst_reason=$(worst_reason_for_status "$result" "${pairs[@]}")
  fi
  # stdout STATUS line keeps the human-summary REASON (backward-compat: layer-all.sh +
  # verify-completion.sh parse this for the ENUM only). The attributable cause rides
  # the stderr LAYER= line below.
  reason="layer${__LAYER_NUM}-summary"
  printf 'STATUS=%s REASON=%s\n' "$result" "$reason"
  # The §CRITICAL 3am anchor (O2): LAYER= line ALWAYS emits before exit on EVERY branch
  # incl. the new verb-missing-non-zero one. Its REASON field carries worst_reason (the
  # real worst-child cause), NOT the generic summary — so a non-zero exit names why.
  printf 'LAYER=%s START=%s END=%s DURATION=%s RESULT=%s REASON=%s\n' \
    "$__LAYER_NUM" "$__LAYER_START" "$end" "$duration" "$result" "$worst_reason" >&2

  # Exit code via status_to_exit_code(result) — single source of truth (dry-reviewer F1),
  # pure f(enum) (task #52). OK/SKIPPED-*/N/A → 0; FAIL → 1; FAIL-MISSING-VERB/UNKNOWN → 2.
  # worst_reason drives only the human-readable stderr REASON above, never the exit code.
  rc=$(status_to_exit_code "$result")
  trap - EXIT
  exit "$rc"
}
