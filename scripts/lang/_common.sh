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

# Single source of truth for the UNEXPECTED-verb-missing REASON suffix (task #50,
# test-reviewer item 4). The two-layer classifier has two independent touch points for
# this literal:
#   - `_dispatch.sh` DERIVES the reason: "${name}-${verb}${DEVLOOP_UNEXPECTED_VERB_SUFFIX}"
#   - `_common.sh::__is_intentional_gap_reason` MATCHES on it (layer edge, no lang:verb context)
# Routing both through this one constant means a rename can't silently desync the
# producer and the matcher. The `_wrapper_trap.test.sh` shared-contract test also pins
# both sides to this same value end-to-end.
readonly DEVLOOP_UNEXPECTED_VERB_SUFFIX='-UNEXPECTED-verb-missing-or-not-executable'

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

# Is this REASON token an INTENTIONAL (exit-0) SKIPPED-NO-VERB gap, vs an UNEXPECTED
# (exit-2 wiring-fault) verb-missing? Single shared predicate (dry-reviewer item 3),
# consumed by status_to_exit_code and the tests.
#
# Matching key (test-reviewer): the intentional token (`-sh-missing-or-not-executable`)
# and the unexpected token (`-UNEXPECTED-verb-missing-or-not-executable`) share the
# suffix `missing-or-not-executable`, so we anchor on the marker UNIQUE to the
# unexpected path — the literal `UNEXPECTED` segment — NEVER the shared suffix. A reason
# WITHOUT the UNEXPECTED marker is intentional (or any other non-bug reason).
# Args: $1=reason token
# Returns: 0 (true) if intentional/non-bug; 1 (false) if it carries the UNEXPECTED marker
# The matcher anchors on the shared DEVLOOP_UNEXPECTED_VERB_SUFFIX constant (single
# source of truth with the dispatcher's reason-derivation — test-reviewer item 4).
__is_intentional_gap_reason() {
  [[ "$1" != *"${DEVLOOP_UNEXPECTED_VERB_SUFFIX}" ]]
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
  return 0
}

# -----------------------------------------------------------------------------
# STATUS aggregation (test §D + code-reviewer locked)
# -----------------------------------------------------------------------------

# Aggregate multiple STATUS values; print the worst per precedence.
#
# Precedence (code-reviewer locked, re-confirmed Wave 2 #4 per ADR-0033):
#   FAIL > N/A > OK > SKIPPED-NO-DIFF > SKIPPED-NO-VERB
# Reasoning: if any child did real work and passed, the layer passed; otherwise
# the SKIPPED-* state is informative. The Wave 1 ladder (OK at the bottom) was
# correct only for the single-lang case — once a 2nd lang registered with a
# verb wrapper, a clean rust-only edit would aggregate to SKIPPED-NO-DIFF (or
# SKIPPED-NO-VERB for missing verbs), corrupting the "loud success" signal.
# Wave 2 #4 re-ranks OK above the SKIPPED-* class to honor the documented
# invariant in all multi-lang cases. N/A remains a deliberate documented gap
# (e.g. layer 7 wave2-pending) that propagates above OK because it signals
# "this verb is not yet wired" — distinct from "ran cleanly". UNKNOWN ranks
# above FAIL (means dispatcher bug → exit 2).
#
# Exit-code mapping (status_to_exit_code): unaffected by the re-rank — OK and
# both SKIPPED-* all still map to 0 (§6 success-exit class).
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
    SKIPPED-NO-VERB)  printf '0\n' ;;
    SKIPPED-NO-DIFF)  printf '1\n' ;;
    OK)               printf '2\n' ;;
    N/A)              printf '3\n' ;;
    FAIL)             printf '4\n' ;;
    UNKNOWN)          printf '5\n' ;;
    *)                printf '5\n' ;;  # unknown enum → treat as bug
  esac
}

# Map a (STATUS enum, REASON) pair to its ADR-0033 §6 exit code (dry-reviewer F1).
# Single source of truth for status→exit-code mapping; replaces the duplicated
# case-statements that used to live in __layer_lifecycle_end and the dispatcher.
#
# Task #50: the exit code for SKIPPED-NO-VERB is now REASON-dependent. An UNEXPECTED
# verb-missing (a wrapper that should exist but is missing/non-executable) is a
# WIRING fault and maps to exit 2 — the ADR-0033 §6 "wrapper/dispatcher bug —
# investigate the script itself" class, alongside UNKNOWN. SKIPPED-NO-VERB for an
# intentional gap (proto test/audit) or all-langs-filtered stays in the exit-0
# success class. The UNEXPECTED arm is EXPLICIT (Lead ruling), not the `*)→2`
# catch-all, so the intent is legible and a future reason-token rename can't silently
# fall through to 2.
#
# Args: $1=STATUS enum value  $2=REASON token (optional; only consulted for SKIPPED-NO-VERB)
# Outputs: stdout=exit code (0/1/2)
# Returns: 0 always (the exit code is on stdout)
status_to_exit_code() {
  local status="$1" reason="${2:-}"
  case "$status" in
    SKIPPED-NO-VERB)
      if __is_intentional_gap_reason "$reason"; then
        printf '0\n'                       # intentional gap / all-langs-filtered → success class
      else
        printf '2\n'                       # UNEXPECTED verb-missing → wiring-fault class (task #50)
      fi
      ;;
    OK|SKIPPED-NO-DIFF|N/A) printf '0\n' ;;
    FAIL)                    printf '1\n' ;;
    *)                       printf '2\n' ;;  # UNKNOWN / dispatcher bug
  esac
}

# Among children sharing the WINNING enum, pick the reason whose status_to_exit_code
# is HIGHEST — so when a layer/dispatch aggregates to SKIPPED-NO-VERB but one
# contributing child carried an UNEXPECTED-verb-missing reason (exit 2) while another
# carried an intentional-gap reason (exit 0), the UNEXPECTED reason wins and the layer
# reds (task #50 tie-break; observability/operations). Single SPOT helper consumed by
# __layer_lifecycle_end and the dispatcher (dry-reviewer item 3).
#
# Non-empty fallback (semantic-guard check 2): if NO child reason matches the winning
# enum (shouldn't happen with 1:1 arrays, but guard the edge), emit a generic
# "<status>-aggregate" reason so the caller never feeds an empty 2nd arg to
# status_to_exit_code.
#
# Args: $1=winning enum; then alternating enum/reason pairs ($2=enum $3=reason ...)
# Outputs: stdout=worst-exit-driving reason among children whose enum==$1
# Returns: 0
worst_reason_for_status() {
  local winner="$1"; shift
  local best_reason="" best_code=-1 e r code
  while [[ $# -ge 2 ]]; do
    e="$1"; r="$2"; shift 2
    [[ "$e" == "$winner" ]] || continue
    code=$(status_to_exit_code "$e" "$r")
    if [[ "$code" -gt "$best_code" ]]; then
      best_code="$code"
      best_reason="$r"
    fi
  done
  if [[ -z "$best_reason" ]]; then
    printf '%s-aggregate\n' "$(printf '%s' "$winner" | tr '[:upper:]' '[:lower:]')"
  else
    printf '%s\n' "$best_reason"
  fi
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
  # Parallel to __LAYER_STATUSES, index-aligned (task #50): __LAYER_REASONS[i] is the
  # REASON of the child whose enum is __LAYER_STATUSES[i]. Lets __layer_lifecycle_end
  # drive the exit code off the worst child's REASON (UNEXPECTED verb-missing → exit 2)
  # without overloading __LAYER_STATUSES (which stays bare enums — locked tests +
  # lastpipe contract preserved; observability P1).
  __LAYER_REASONS=()
  __LAYER_RESULT="UNKNOWN"
  # Export DEVLOOP_LAYER for child processes (dry-reviewer F3).
  # _get_base_ref.sh reads this to namespace the per-layer changed-files cache,
  # avoiding the duplicated `DEVLOOP_LAYER=N` per-line prefix on every layer-script
  # subprocess invocation.
  export DEVLOOP_LAYER="$1"
  trap '__layer_lifecycle_end' EXIT
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
    # then pick the worst-exit-driving reason among children whose enum == result
    # (task #50). This drives BOTH the exit code (so an UNEXPECTED verb-missing reds
    # the layer even when it shares the SKIPPED-NO-VERB enum with an intentional gap)
    # and the stderr LAYER= REASON (so the loud exit points at the real cause —
    # observability P2 — distinguishing the two exit-2 classes: UNKNOWN vs verb-missing).
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

  # Exit code via status_to_exit_code(result, worst_reason) — single source of truth
  # (dry-reviewer F1). OK/SKIPPED-NO-DIFF/N/A → 0; FAIL → 1; UNKNOWN → 2;
  # SKIPPED-NO-VERB → 0 for intentional/all-langs-filtered, 2 for UNEXPECTED verb-missing
  # (task #50 — surface a wiring fault loud, not silent).
  rc=$(status_to_exit_code "$result" "$worst_reason")
  trap - EXIT
  exit "$rc"
}
