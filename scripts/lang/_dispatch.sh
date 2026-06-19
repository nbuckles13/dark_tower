#!/usr/bin/env bash
# _dispatch.sh — per-verb dispatcher (ADR-0033 §6).
#
# Provides for_each_lang_with_verb: iterates scripts/lang/<X>/, runs each language's
# changed.sh, then invokes the requested verb script (or emits FAIL-MISSING-VERB if a
# verb wrapper that should exist is missing/non-executable).
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

# Intentional verb gaps are registered as PLACEHOLDER verb scripts (task #52,
# filesystem-as-source-of-truth): a lang that deliberately has no real <verb>.sh ships a
# one-line wrapper emitting `STATUS=N/A REASON=not-applicable-to-this-lang` (see
# scripts/lang/proto/{test,audit}.sh). With placeholders present, a verb script that is
# genuinely MISSING/non-executable is ALWAYS a wiring fault — the dispatcher emits
# FAIL-MISSING-VERB unconditionally (no allowlist, no allowlist↔filesystem drift). This
# replaces #50's lang:verb intentional-gap allowlist (and its test-only env seam);
# readers of scripts/lang/<X>/ now see an intentional gap at the filesystem level.

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
#   - DEVLOOP_DISPATCH_INCLUDE_LANGS=<lang> → keep only the named lang.
#   - DEVLOOP_DISPATCH_EXCLUDE_LANGS=<lang> → drop the named lang.
#     (Wave 2 #4: single-lang exact match; multi-lang/comma-split deferred
#     per YAGNI. layer1.sh uses INCLUDE for stage 1 and EXCLUDE for stage 2.)
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
  local include="${DEVLOOP_DISPATCH_INCLUDE_LANGS:-}"
  local exclude="${DEVLOOP_DISPATCH_EXCLUDE_LANGS:-}"

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

  # Apply INCLUDE/EXCLUDE filter BEFORE the lint-at-startup pass: a
  # filtered-out lang is invisible to the dispatcher (no changed.sh
  # requirement, no execution, no cache-write side-effects). This means the
  # filter is a true "skip this lang entirely" — useful for layer scripts
  # that need to invoke different lang subsets in different stages (e.g.,
  # layer1.sh stages proto separately from rust+ts via INCLUDE then EXCLUDE).
  #
  # Single-lang exact match for Wave 2 — multi-lang/comma-split deliberately
  # deferred per YAGNI (CLAUDE.md "don't design for hypothetical future
  # requirements"); if a future layer needs multi-lang filter, format
  # extension is trivial.
  if [[ -n "$include" ]]; then
    local -a kept=()
    for name in "${langs[@]}"; do
      [[ "$name" == "$include" ]] && kept+=("$name")
    done
    langs=("${kept[@]}")
  elif [[ -n "$exclude" ]]; then
    local -a kept=()
    for name in "${langs[@]}"; do
      [[ "$name" == "$exclude" ]] || kept+=("$name")
    done
    langs=("${kept[@]}")
  fi

  if [[ ${#langs[@]} -eq 0 ]]; then
    # Operator-error case (e.g. INCLUDE=nonexistent or EXCLUDE matched all).
    # Loud signal — langs *exist*, they were explicitly muted (closer to
    # SKIPPED-NO-VERB than N/A's "verb doesn't apply to anything").
    echo "# dispatcher: INCLUDE='${include}' EXCLUDE='${exclude}' cleared lang set" >&2
    emit_status SKIPPED-NO-VERB "all-langs-filtered"
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
  # child_statuses (enums) + child_reasons (reasons) are pushed 1:1 at EVERY site
  # (semantic-guard check 3 — index alignment) so the exit decision can consult the
  # worst child's REASON, not just the enum (task #50).
  local -a child_statuses=()
  local -a child_reasons=()
  local s line
  local lang_count=${#langs[@]}
  for name in "${langs[@]}"; do
    local verb_sh="${lang_root}/${name}/${verb}.sh"
    local lang_status lang_reason

    if [[ "$always_run" != "1" ]]; then
      # Skip-if-untouched short-circuit via changed.sh.
      if ! "${lang_root}/${name}/changed.sh"; then
        lang_status="SKIPPED-NO-DIFF"
        lang_reason="${name}-no-diff"
        emit_status "$lang_status" "$lang_reason"
        child_statuses+=("$lang_status")
        child_reasons+=("$lang_reason")
        continue
      fi
    fi

    if [[ -x "$verb_sh" ]]; then
      # Run verb wrapper, stream stdout verbatim, capture last STATUS line + reason.
      local tmp_out
      tmp_out=$(mktemp)
      local rc=0
      if ! "$verb_sh" "$@" >"$tmp_out" 2>&1; then
        rc=$?
      fi
      cat "$tmp_out"
      lang_status=$(parse_status_line "$tmp_out")
      lang_reason=$(parse_status_reason "$tmp_out")
      rm -f "$tmp_out"
      if [[ -z "$lang_status" ]]; then
        # Empty pipe: the wrapper emitted no STATUS line (and its EXIT trap didn't
        # fire — e.g. killed -9, or a non-trap-installed script). Genuinely
        # exceptional now that the 14 wrappers self-emit via the trap (task #50,
        # observability P4). Surface as UNKNOWN, not silent.
        lang_status="UNKNOWN"
        lang_reason="${name}-${verb}-no-status-emitted"
      fi
      child_statuses+=("$lang_status")
      child_reasons+=("$lang_reason")
      # Continue iterating other langs even if this one failed —
      # we want the full picture per ADR philosophy.
      _ignored_rc=$rc  # avoid unused-var lint
    else
      # Verb script genuinely missing-or-not-executable → always a WIRING fault
      # (task #52). Intentional gaps register as placeholder <verb>.sh scripts that
      # emit N/A (taking the `-x` branch above), so reaching HERE means a wrapper that
      # should exist is missing/chmod-stripped — FAIL-MISSING-VERB (rank 5), which
      # status_to_exit_code maps to exit 2 (§6 wiring-fault class). No allowlist: the
      # enum carries the semantics, so a sibling lang's OK can never mask this.
      lang_status="FAIL-MISSING-VERB"
      lang_reason="${name}-${verb}-verb-missing-or-not-executable"
      emit_status "$lang_status" "$lang_reason"
      child_statuses+=("$lang_status")
      child_reasons+=("$lang_reason")
    fi
  done

  # Emission rule: only emit aggregated STATUS when 2+ langs participated.
  # 1-lang case: child's STATUS is already the final line on stdout.
  if [[ $lang_count -gt 1 ]]; then
    local agg worst_reason i pairs
    agg=$(aggregate_worst_status "${child_statuses[@]}")
    # Pick a representative reason among children sharing the winning enum, for the
    # emitted line (so a FAIL-MISSING-VERB aggregate names the offending lang/verb, not
    # a flattened token). With FAIL-MISSING-VERB ranked above OK (task #52), a missing
    # verb wins the aggregate over a sibling's clean run — masking closed at the ladder,
    # no reason→exit coupling needed.
    pairs=()
    for ((i = 0; i < ${#child_statuses[@]}; i++)); do
      pairs+=("${child_statuses[i]}" "${child_reasons[i]:-}")
    done
    worst_reason=$(worst_reason_for_status "$agg" "${pairs[@]}")
    case "$agg" in
      OK)                 emit_status OK                "${verb}-all-langs-ok" ;;
      SKIPPED-NO-DIFF)    emit_status SKIPPED-NO-DIFF   "all-langs-untouched" ;;
      SKIPPED-NO-VERB)    emit_status SKIPPED-NO-VERB   "$worst_reason" ;;
      N/A)                emit_status N/A               "${verb}-aggregate-na" ;;
      FAIL)               emit_status FAIL              "${verb}-some-lang-failed" ;;
      # Emit the offending lang/verb REASON so the loud exit names why.
      FAIL-MISSING-VERB)  emit_status FAIL-MISSING-VERB "$worst_reason" ;;
      *)                  emit_status FAIL              "${verb}-aggregate-unknown" ;;
    esac
    return "$(status_to_exit_code "$agg")"
  fi

  # Single-lang path: derive return code from the one child's enum, so a single touched
  # lang with a missing verb returns exit 2 (FAIL-MISSING-VERB → §6 wiring-fault class).
  return "$(status_to_exit_code "${child_statuses[0]:-UNKNOWN}")"
}
