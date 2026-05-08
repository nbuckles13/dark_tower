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

# Emit a STATUS= line to stdout in the canonical format.
# Args: $1=status-enum  $2=reason-token-no-spaces
# Outputs: stdout="STATUS=<status> REASON=<reason>"
# Returns: 0
emit_status() {
  printf 'STATUS=%s REASON=%s\n' "$1" "$2"
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

# -----------------------------------------------------------------------------
# STATUS aggregation (test §D + code-reviewer locked)
# -----------------------------------------------------------------------------

# Aggregate multiple STATUS values; print the worst per precedence.
#
# Precedence (code-reviewer locked): FAIL > N/A > SKIPPED-NO-DIFF > SKIPPED-NO-VERB > OK
# Reasoning: SKIPPED-NO-VERB is a §6 success-exit; loud visibility is via the child
# STATUS line surviving the stream-verbatim contract, not via aggregated promotion.
# N/A is a deliberate documented gap (e.g. layer 7 wave2-pending); SKIPPED-NO-DIFF
# is "world state didn't change for this lang"; SKIPPED-NO-VERB is "polyglot reality
# during ramp-up". A Wave 1 run with only lang/rust/ + Rust OK aggregates to OK, not
# SKIPPED-NO-VERB — so the success path stays clean.
# UNKNOWN ranks above FAIL (means dispatcher bug → exit 2).
#
# Args: $@=zero-or-more STATUS enum values
# Outputs: stdout=worst status (OK if no args)
# Returns: 0
aggregate_worst_status() {
  local s worst="OK"
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
    OK)               printf '0\n' ;;
    SKIPPED-NO-VERB)  printf '1\n' ;;
    SKIPPED-NO-DIFF)  printf '2\n' ;;
    N/A)              printf '3\n' ;;
    FAIL)             printf '4\n' ;;
    UNKNOWN)          printf '5\n' ;;
    *)                printf '5\n' ;;  # unknown enum → treat as bug
  esac
}

# Map a STATUS enum value to its ADR-0033 §6 exit code (dry-reviewer F1).
# Single source of truth for status→exit-code mapping; replaces the duplicated
# case-statements that used to live in __layer_lifecycle_end and the dispatcher.
#
# Args: $1=STATUS enum value
# Outputs: stdout=exit code (0/1/2)
# Returns: 0 always (the exit code is on stdout)
status_to_exit_code() {
  case "$1" in
    OK|SKIPPED-NO-DIFF|SKIPPED-NO-VERB|N/A) printf '0\n' ;;
    FAIL)                                    printf '1\n' ;;
    *)                                       printf '2\n' ;;  # UNKNOWN / dispatcher bug
  esac
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
  __LAYER_RESULT="UNKNOWN"
  # Export DEVLOOP_LAYER for child processes (dry-reviewer F3).
  # _get_base_ref.sh reads this to namespace the per-layer changed-files cache,
  # avoiding the duplicated `DEVLOOP_LAYER=N` per-line prefix on every layer-script
  # subprocess invocation.
  export DEVLOOP_LAYER="$1"
  trap '__layer_lifecycle_end' EXIT
}

# Stream stdin to stdout verbatim, side-effect __LAYER_STATUSES with parsed STATUS values.
#
# Args: (none — reads stdin)
# Outputs: stdout=verbatim copy
# Returns: 0
tee_collect_statuses() {
  local line
  while IFS= read -r line; do
    printf '%s\n' "$line"
    if [[ "$line" =~ ^STATUS=([^[:space:]]+) ]]; then
      __LAYER_STATUSES+=("${BASH_REMATCH[1]}")
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
  local end duration result reason rc
  end=$(date +%s)
  duration=$((end - __LAYER_START))
  if [[ ${#__LAYER_STATUSES[@]} -eq 0 ]]; then
    result="$__LAYER_RESULT"
  else
    result=$(aggregate_worst_status "${__LAYER_STATUSES[@]}")
  fi
  reason="layer${__LAYER_NUM}-summary"
  printf 'STATUS=%s REASON=%s\n' "$result" "$reason"
  printf 'LAYER=%s START=%s END=%s DURATION=%s RESULT=%s REASON=%s\n' \
    "$__LAYER_NUM" "$__LAYER_START" "$end" "$duration" "$result" "$reason" >&2

  # Exit code mapping via status_to_exit_code() — single source of truth (dry-reviewer F1).
  # Mapping rationale: OK/SKIPPED-*/N/A → 0 (§6 success-exit class), FAIL → 1,
  # UNKNOWN → 2 (dispatcher bug: child crashed before emitting STATUS, or
  # tee_collect_statuses pipe was broken — surface loud, not silent).
  rc=$(status_to_exit_code "$result")
  trap - EXIT
  exit "$rc"
}
