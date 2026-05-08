#!/usr/bin/env bash
# _dispatch.sh — per-verb dispatcher (ADR-0033 §6).
#
# Provides for_each_lang_with_verb: iterates scripts/lang/<X>/, runs each language's
# changed.sh, then invokes the requested verb script (or emits SKIPPED-NO-VERB).
#
# STATUS line emission rule (paired-operations Q3):
#   - 1 lang touched, 1 STATUS streamed → final stdout = that single STATUS;
#     dispatcher does NOT re-emit.
#   - 2+ langs (each emitting STATUS) → dispatcher streams each, then emits ONE
#     aggregated STATUS (worst-child wins) as final line.
#   - 0 langs touched → each lang's SKIPPED-NO-DIFF streamed; dispatcher emits
#     aggregated STATUS=SKIPPED-NO-DIFF REASON=all-langs-untouched.

set -euo pipefail
IFS=$'\n\t'

[[ -n "${__DEVLOOP_DISPATCH_SH:-}" ]] && return 0
readonly __DEVLOOP_DISPATCH_SH=1

__here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=_common.sh
source "${__here}/_common.sh"

# Iterate language directories under DEVLOOP_LANG_ROOT (default: scripts/lang/),
# run each language's changed.sh, and dispatch the requested verb.
#
# Args:
#   $1 = verb name (test|lint|fmt|compile|audit)
#   $@ = pass-through args forwarded to each lang's <verb>.sh
#
# Special-cases:
#   - DEVLOOP_DISPATCH_ALWAYS_RUN=1 in env → skip changed.sh short-circuit
#     (used by audit dispatcher; ADR-0033 §3 always-run).
#   - DEVLOOP_LANG_ROOT env override → for hermetic _dispatch.test.sh (test §E).
#
# Outputs:
#   stdout = streamed child STATUS lines + (when 2+ langs) one aggregated STATUS
#   stderr = errors naming the offending lang/<X>/changed.sh on missing-changed.sh
#
# Returns: 0 on OK/SKIPPED/N/A; 1 on FAIL; 2 on dispatcher bug.
for_each_lang_with_verb() {
  local verb="$1"; shift
  local lang_root="${DEVLOOP_LANG_ROOT:-${__here}}"
  local always_run="${DEVLOOP_DISPATCH_ALWAYS_RUN:-0}"

  local -a langs=()
  local d name
  for d in "${lang_root}"/*/; do
    [[ -d "$d" ]] || continue
    name="$(basename "$d")"
    # Skip underscore-prefixed (helpers + fixtures dirs).
    [[ "$name" =~ ^_ ]] && continue
    [[ "$name" == "fixtures" ]] && continue
    langs+=("$name")
  done

  if [[ ${#langs[@]} -eq 0 ]]; then
    emit_status N/A "no-languages-registered"
    return 0
  fi

  # Lint-at-startup: every lang must have executable changed.sh (ADR-0033 §2).
  for name in "${langs[@]}"; do
    local changed_sh="${lang_root}/${name}/changed.sh"
    if [[ ! -x "$changed_sh" ]]; then
      echo "ERROR: ${name}/changed.sh: missing or not executable at ${changed_sh}" >&2
      emit_status FAIL "dispatcher-missing-changed-sh-${name}"
      return 2
    fi
  done

  # Dispatch each language.
  local -a child_statuses=()
  local s line
  local lang_count=${#langs[@]}
  for name in "${langs[@]}"; do
    local verb_sh="${lang_root}/${name}/${verb}.sh"
    local lang_status

    if [[ "$always_run" != "1" ]]; then
      # Skip-if-untouched short-circuit via changed.sh.
      if ! "${lang_root}/${name}/changed.sh"; then
        lang_status="SKIPPED-NO-DIFF"
        emit_status "$lang_status" "${name}-no-diff"
        child_statuses+=("$lang_status")
        continue
      fi
    fi

    if [[ -x "$verb_sh" ]]; then
      # Run verb wrapper, stream stdout verbatim, capture last STATUS line.
      local tmp_out
      tmp_out=$(mktemp)
      local rc=0
      if ! "$verb_sh" "$@" >"$tmp_out" 2>&1; then
        rc=$?
      fi
      cat "$tmp_out"
      lang_status=$(parse_status_line "$tmp_out")
      rm -f "$tmp_out"
      [[ -z "$lang_status" ]] && lang_status="UNKNOWN"
      child_statuses+=("$lang_status")
      # Continue iterating other langs even if this one failed —
      # we want the full picture per ADR philosophy.
      _ignored_rc=$rc  # avoid unused-var lint
    else
      lang_status="SKIPPED-NO-VERB"
      emit_status "$lang_status" "${name}-${verb}-sh-missing-or-not-executable"
      child_statuses+=("$lang_status")
    fi
  done

  # Emission rule: only emit aggregated STATUS when 2+ langs participated.
  # 1-lang case: child's STATUS is already the final line on stdout.
  if [[ $lang_count -gt 1 ]]; then
    local agg
    agg=$(aggregate_worst_status "${child_statuses[@]}")
    case "$agg" in
      OK)               emit_status OK              "${verb}-all-langs-ok" ;;
      SKIPPED-NO-DIFF)  emit_status SKIPPED-NO-DIFF "all-langs-untouched" ;;
      SKIPPED-NO-VERB)  emit_status SKIPPED-NO-VERB "${verb}-some-langs-missing-verb" ;;
      N/A)              emit_status N/A             "${verb}-aggregate-na" ;;
      FAIL)             emit_status FAIL            "${verb}-some-lang-failed" ;;
      *)                emit_status FAIL            "${verb}-aggregate-unknown" ;;
    esac
    return "$(status_to_exit_code "$agg")"
  fi

  # Single-lang path: derive return code from the one child status.
  return "$(status_to_exit_code "${child_statuses[0]:-UNKNOWN}")"
}
