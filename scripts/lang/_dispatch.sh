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

# Intentional verb-missing allowlist (task #50). proto deliberately has no test.sh
# (Layer 4) nor audit.sh (Layer 6 — breaking.sh is its audit gate), per ADR-0033 §1.
# Those gaps are EXPECTED and must keep exiting 0; every OTHER missing/non-executable
# verb wrapper is a WIRING fault and must exit 2 (UNEXPECTED).
#
# Production reads a HARDCODED constant. The DEVLOOP_INTENTIONAL_MISSING_VERBS override
# is honored ONLY under the test sentinel DEVLOOP_TEST=1 (security BLOCKING item 1) —
# mirroring _audit_gate.sh:__audit_pnpm_ignore_path. An unconditional env surface would
# let any caller downgrade a genuinely-missing security gate from exit-2 back to exit-0,
# re-opening the exact silent-skip class this task closes. Reusing DEVLOOP_TEST keeps
# the seam under the existing assert_no_ci_sentinel_leak trust boundary (_common.sh) —
# no parallel sentinel, no new uncovered CI hole (security item 2).
#
# Format: space-separated `lang:verb` tokens.
# Args: (none — reads env)
# Outputs: stdout=space-separated lang:verb allowlist
# Returns: 0
__intentional_missing_verbs() {
  if [[ "${DEVLOOP_TEST:-}" == "1" && -n "${DEVLOOP_INTENTIONAL_MISSING_VERBS:-}" ]]; then
    printf '%s\n' "$DEVLOOP_INTENTIONAL_MISSING_VERBS"
  else
    printf '%s\n' "proto:test proto:audit"   # production constant — literal, not env-derived
  fi
}

# Is this lang:verb an INTENTIONAL (documented, exit-0) verb gap?
# Authoritative source-side check (test-reviewer): keys off ALLOWLIST MEMBERSHIP, not
# reason-string shape — so the dispatcher's exit decision can never be fooled by a
# reason token's spelling. (The layer edge, which lacks lang:verb context, keys off the
# UNEXPECTED reason marker via __is_intentional_gap_reason instead.)
# Args: $1=lang  $2=verb
# Returns: 0 (true) if lang:verb is in the allowlist; 1 (false) otherwise
__is_intentional_gap() {
  local want="$1:$2" tok
  # Split the allowlist on whitespace explicitly: _common.sh sets IFS=$'\n\t' (no
  # space), so a bare `for tok in $(…)` or `read -ra` under the inherited IFS would
  # NOT split the space-separated list. Set a space IFS for the read only.
  local -a allow
  IFS=$' \t\n' read -r -a allow <<<"$(__intentional_missing_verbs)"
  for tok in "${allow[@]}"; do
    [[ "$tok" == "$want" ]] && return 0
  done
  return 1
}

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
      # Verb script missing-or-not-executable. Reason is keyed off ALLOWLIST
      # MEMBERSHIP (task #50): an intentional gap (proto:test/proto:audit) keeps the
      # historical `-sh-missing-or-not-executable` token and exits 0; everything else
      # is a WIRING fault → the loud UNEXPECTED token, which status_to_exit_code maps
      # to exit 2. The UNEXPECTED suffix is the shared DEVLOOP_UNEXPECTED_VERB_SUFFIX
      # constant in _common.sh — SAME literal the layer's __is_intentional_gap_reason
      # matches on (single source of truth; test-reviewer item 4).
      lang_status="SKIPPED-NO-VERB"
      if __is_intentional_gap "$name" "$verb"; then
        lang_reason="${name}-${verb}-sh-missing-or-not-executable"
      else
        lang_reason="${name}-${verb}${DEVLOOP_UNEXPECTED_VERB_SUFFIX}"
      fi
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
    # Worst-exit-driving reason among children sharing the winning enum (task #50):
    # so a SKIPPED-NO-VERB aggregate whose winning reason is UNEXPECTED returns exit 2
    # instead of flattening to status_to_exit_code SKIPPED-NO-VERB=0 (the :162 hole).
    pairs=()
    for ((i = 0; i < ${#child_statuses[@]}; i++)); do
      pairs+=("${child_statuses[i]}" "${child_reasons[i]:-}")
    done
    worst_reason=$(worst_reason_for_status "$agg" "${pairs[@]}")
    case "$agg" in
      OK)               emit_status OK              "${verb}-all-langs-ok" ;;
      SKIPPED-NO-DIFF)  emit_status SKIPPED-NO-DIFF "all-langs-untouched" ;;
      # Emit the worst child's REASON (which carries the UNEXPECTED marker when a
      # verb-missing wiring fault drove the aggregate) rather than flattening it away —
      # so the exit code AND the emitted line agree on why.
      SKIPPED-NO-VERB)  emit_status SKIPPED-NO-VERB "$worst_reason" ;;
      N/A)              emit_status N/A             "${verb}-aggregate-na" ;;
      FAIL)             emit_status FAIL            "${verb}-some-lang-failed" ;;
      *)                emit_status FAIL            "${verb}-aggregate-unknown" ;;
    esac
    return "$(status_to_exit_code "$agg" "$worst_reason")"
  fi

  # Single-lang path: derive return code from the one child (status + reason), so a
  # single touched lang with an UNEXPECTED verb-missing returns exit 2 (the headline
  # criterion-(b) case).
  return "$(status_to_exit_code "${child_statuses[0]:-UNKNOWN}" "${child_reasons[0]:-}")"
}
