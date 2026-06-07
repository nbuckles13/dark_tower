#!/usr/bin/env bash
# audit-suppressions-check.sh — always-run hygiene gate for the audit-suppression
# manifest (ADR-0033 §3 amendment, task #47). This is the discipline backstop that
# replaces the old "scan every advisory every run" always-run guarantee: it runs on
# EVERY devloop + CI run (Layer 3 guard), independent of whether any dependency
# manifest changed, and hard-fails when a suppression goes past its `expires` date.
#
# Three checks (all run; one aggregated FAIL if any fails), each with a DISTINCT
# REASON token so on-call greps to the right runbook row (docs/runbooks/devloop-
# validation.md §6.3):
#   date-check     -> STATUS=FAIL REASON=suppression-past-due
#   sync-check     -> STATUS=FAIL REASON=suppression-drift
#   quality-check  -> STATUS=FAIL REASON=suppression-quality
#   parse error    -> STATUS=FAIL REASON=suppression-malformed
# Plus a trust-boundary guard:
#   override env present without exact DEVLOOP_TEST=1
#                  -> STATUS=FAIL REASON=suppression-override-without-test-sentinel
#
# `--fix` regenerates the derived files (.cargo/audit.toml, .pnpm-audit-ignore.json)
# from the manifest. Without --fix the check is read-only verify (the production
# Layer-3 guard runs it read-only).
#
# Suppression flows ONLY through audit-suppressions.toml -> generated derived files,
# never via CLI flags or hand-edited derived files (ADR-0033 §11). See
# docs/contributor/audit-suppressions.md.
#
# TEST SEAM (security trust boundary, §J A+B+C): the override envs below are read
# ONLY when the test sentinel is set EXACTLY to "1" in this process. The production
# guard never sets it; only *.test.sh does, in a hermetic subshell. This makes the
# always-run path inert to the ambient environment, provable by reading this script:
#   DEVLOOP_SUPPRESSIONS_MANIFEST  — manifest path override (default: repo-root)
#   AUDIT_SUPPRESSIONS_NOW         — "today" override (default: real `date -u +%F`)
#   DEVLOOP_AUDIT_IGNORE_JSON      — .pnpm-audit-ignore.json path override (default: repo-root)
#   DEVLOOP_CARGO_AUDIT_TOML       — .cargo/audit.toml path override (default: repo-root)
# DO NOT change the gating to a truthy `-n` test — `DEVLOOP_TEST=0` must NOT enable
# overrides (that is the bypass operations refinement #1 closes).
set -euo pipefail
IFS=$'\n\t'

__here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lang/_common.sh
source "${__here}/lang/_common.sh"
# shellcheck source=lang/_audit_suppressions_lib.sh
# Shared pure reader read_pnpm_ignore_ids (the sole .pnpm-audit-ignore.json parse
# locus; sync-check applies fail-LOUD, the ts filter applies fail-SAFE).
source "${__here}/lang/_audit_suppressions_lib.sh"

readonly REPO_ROOT="$(cd "${__here}/.." && pwd)"

# -----------------------------------------------------------------------------
# Test-seam: exact-match sentinel (§J A+B). Overrides are OFF unless DEVLOOP_TEST
# is EXACTLY "1". Any other value (absent, "0", "false", "yes") leaves overrides
# inert AND, if an override env is nonetheless present, is treated as a tamper /
# misconfig signal (fail-loud below).
# -----------------------------------------------------------------------------
__test_sentinel_active() {
  [[ "${DEVLOOP_TEST:-}" == "1" ]]
}

# True if any test-injection override env is set (regardless of sentinel).
__any_override_present() {
  [[ -n "${DEVLOOP_SUPPRESSIONS_MANIFEST:-}" ]] && return 0
  [[ -n "${AUDIT_SUPPRESSIONS_NOW:-}" ]] && return 0
  [[ -n "${DEVLOOP_AUDIT_IGNORE_JSON:-}" ]] && return 0
  [[ -n "${DEVLOOP_CARGO_AUDIT_TOML:-}" ]] && return 0
  return 1
}

# Resolve effective paths / now, honoring overrides ONLY under the exact sentinel.
__manifest_path() {
  if __test_sentinel_active && [[ -n "${DEVLOOP_SUPPRESSIONS_MANIFEST:-}" ]]; then
    printf '%s\n' "$DEVLOOP_SUPPRESSIONS_MANIFEST"
  else
    printf '%s\n' "${REPO_ROOT}/audit-suppressions.toml"
  fi
}

__cargo_audit_toml_path() {
  if __test_sentinel_active && [[ -n "${DEVLOOP_CARGO_AUDIT_TOML:-}" ]]; then
    printf '%s\n' "$DEVLOOP_CARGO_AUDIT_TOML"
  else
    printf '%s\n' "${REPO_ROOT}/.cargo/audit.toml"
  fi
}

__pnpm_ignore_json_path() {
  if __test_sentinel_active && [[ -n "${DEVLOOP_AUDIT_IGNORE_JSON:-}" ]]; then
    printf '%s\n' "$DEVLOOP_AUDIT_IGNORE_JSON"
  else
    printf '%s\n' "${REPO_ROOT}/.pnpm-audit-ignore.json"
  fi
}

__now() {
  if __test_sentinel_active && [[ -n "${AUDIT_SUPPRESSIONS_NOW:-}" ]]; then
    printf '%s\n' "$AUDIT_SUPPRESSIONS_NOW"
  else
    date -u +%F
  fi
}

# -----------------------------------------------------------------------------
# Single manifest parser (DRY — sole TOML-parse locus, @dry-reviewer watch-point 2).
# Emits one normalized pipe-delimited record per [[suppression]]:
#   id|ecosystem|expires|ticket|reason
# Flat `key = "value"` only (no multiline) — the manifest format is constrained so
# this stays a trivial, robust awk pass. A malformed entry is NEVER degraded to
# "0 suppressions": a structural problem prints to stderr and the awk exits non-zero,
# which the caller turns into REASON=suppression-malformed (fail-closed).
#
# Args: $1=manifest path
# Outputs: stdout=one record per entry; stderr=parse diagnostics on malformed input
# Returns: 0 on clean parse (incl. zero entries for an absent/empty manifest), 1 on malformed
# -----------------------------------------------------------------------------
parse_suppressions() {
  local manifest="$1"
  # Absent or empty manifest is legitimate (zero suppressions) -> clean, no records.
  [[ -f "$manifest" ]] || return 0

  awk '
    function trim(s) { gsub(/^[ \t]+|[ \t]+$/, "", s); return s }
    # Strip surrounding double quotes from a TOML basic string value.
    function unquote(s,   t) {
      t = trim(s)
      if (t ~ /^".*"$/) { return substr(t, 2, length(t) - 2) }
      return t  # caller validates; non-quoted scalars handled by quality-check
    }
    function flush(   missing) {
      if (!in_entry) return
      missing = ""
      if (id == "")        missing = missing " id"
      if (ecosystem == "") missing = missing " ecosystem"
      if (expires == "")   missing = missing " expires"
      if (ticket == "")    missing = missing " ticket"
      if (reason == "")    missing = missing " reason"
      if (missing != "") {
        printf("MALFORMED: entry ending before line %d missing required field(s):%s\n", NR, missing) > "/dev/stderr"
        bad = 1
        return
      }
      if (seen[id]) {
        printf("MALFORMED: duplicate id %s\n", id) > "/dev/stderr"
        bad = 1
        return
      }
      seen[id] = 1
      printf("%s|%s|%s|%s|%s\n", id, ecosystem, expires, ticket, reason)
    }
    BEGIN { in_entry = 0; bad = 0 }
    # Comments and blank lines.
    /^[ \t]*#/  { next }
    /^[ \t]*$/  { next }
    # New array-of-tables header.
    /^[ \t]*\[\[suppression\]\][ \t]*$/ {
      flush()
      in_entry = 1; id = ""; ecosystem = ""; expires = ""; ticket = ""; reason = ""
      next
    }
    # Any other [section] header is unexpected in this single-purpose file.
    /^[ \t]*\[/ {
      printf("MALFORMED: unexpected table header at line %d: %s\n", NR, $0) > "/dev/stderr"
      bad = 1; next
    }
    # key = value lines (only inside an entry).
    /^[ \t]*[A-Za-z_]+[ \t]*=/ {
      if (!in_entry) {
        printf("MALFORMED: key=value outside any [[suppression]] at line %d: %s\n", NR, $0) > "/dev/stderr"
        bad = 1; next
      }
      eq = index($0, "=")
      k = trim(substr($0, 1, eq - 1))
      v = unquote(substr($0, eq + 1))
      if (k == "id")             id = v
      else if (k == "ecosystem") ecosystem = v
      else if (k == "expires")   expires = v
      else if (k == "ticket")    ticket = v
      else if (k == "reason")    reason = v
      else {
        printf("MALFORMED: unknown key %s at line %d\n", k, NR) > "/dev/stderr"
        bad = 1
      }
      next
    }
    # Anything else is unparseable.
    {
      printf("MALFORMED: unparseable line %d: %s\n", NR, $0) > "/dev/stderr"
      bad = 1
    }
    END { flush(); if (bad) exit 1 }
  ' "$manifest"
}

# Parse once into an array of records, or fail-closed loud on malformed.
# Sets the global __RECORDS array. Returns 0 clean, 1 malformed.
__load_records() {
  local manifest; manifest="$(__manifest_path)"
  local parse_err=0 out
  out="$(parse_suppressions "$manifest")" || parse_err=1
  if [[ $parse_err -ne 0 ]]; then
    return 1
  fi
  __RECORDS=()
  local line
  while IFS= read -r line; do
    [[ -n "$line" ]] && __RECORDS+=("$line")
  done <<< "$out"
  return 0
}

# Field extractor for a pipe record.
__field() { printf '%s\n' "$1" | cut -d'|' -f"$2"; }

# Days between two ISO dates (a - b), positive if a > b. Uses `date -d`.
__days_between() {
  local a b sa sb
  a="$1"; b="$2"
  sa=$(date -u -d "$a" +%s 2>/dev/null) || return 2
  sb=$(date -u -d "$b" +%s 2>/dev/null) || return 2
  printf '%s\n' $(( (sa - sb) / 86400 ))
}

# -----------------------------------------------------------------------------
# Checks
# -----------------------------------------------------------------------------

# date-check: hard-fail strictly-past-due (expires < NOW). expires == NOW is VALID.
# Lists EACH past-due id with its OWN days-past + remediation + renewal doc pointer.
__date_check() {
  local now; now="$(__now)"
  local rec id expires days_past failed=0
  local -a past_due=()
  for rec in "${__RECORDS[@]:-}"; do
    [[ -n "$rec" ]] || continue
    id="$(__field "$rec" 1)"
    expires="$(__field "$rec" 3)"
    days_past="$(__days_between "$now" "$expires")" || {
      # Unparseable date -> quality-check territory; skip here (quality catches it).
      continue
    }
    if [[ "$days_past" -gt 0 ]]; then
      past_due+=("${id} expired ${days_past} day(s) ago (expires ${expires})")
      failed=1
    fi
  done
  if [[ $failed -eq 1 ]]; then
    local entry
    for entry in "${past_due[@]}"; do
      echo "FAIL: ${entry}. Either re-suppress with a new date or fix the advisory. Renewal workflow: docs/contributor/audit-suppressions.md" >&2
    done
    return 1
  fi
  return 0
}

# quality-check: reason + ticket non-empty; expires ISO YYYY-MM-DD and a real date;
# ecosystem in {rust, js}.
__quality_check() {
  local rec id ecosystem expires ticket reason failed=0
  for rec in "${__RECORDS[@]:-}"; do
    [[ -n "$rec" ]] || continue
    id="$(__field "$rec" 1)"
    ecosystem="$(__field "$rec" 2)"
    expires="$(__field "$rec" 3)"
    ticket="$(__field "$rec" 4)"
    reason="$(__field "$rec" 5)"
    if [[ "$ecosystem" != "rust" && "$ecosystem" != "js" ]]; then
      echo "FAIL: ${id} has invalid ecosystem '${ecosystem}' (must be rust|js)" >&2; failed=1
    fi
    if [[ ! "$expires" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}$ ]]; then
      echo "FAIL: ${id} has malformed expires '${expires}' (must be YYYY-MM-DD)" >&2; failed=1
    elif ! date -u -d "$expires" +%s >/dev/null 2>&1; then
      echo "FAIL: ${id} expires '${expires}' is not a real date" >&2; failed=1
    fi
    [[ -z "$reason" ]] && { echo "FAIL: ${id} has empty reason" >&2; failed=1; }
    [[ -z "$ticket" ]] && { echo "FAIL: ${id} has empty ticket" >&2; failed=1; }
  done
  return $failed
}

# Read the id list from the generated .cargo/audit.toml `ignore = [...]`.
read_cargo_audit_ignore_ids() {
  local f="$1"
  [[ -f "$f" ]] || return 0
  # Extract quoted ids from the ignore = [ ... ] array (single- or multi-line tolerant
  # for the simple one-array-per-file shape we generate).
  awk '
    /^[ \t]*ignore[ \t]*=/ { inarr = 1 }
    inarr {
      while (match($0, /"[^"]+"/)) {
        s = substr($0, RSTART + 1, RLENGTH - 2)
        print s
        $0 = substr($0, RSTART + RLENGTH)
      }
      if ($0 ~ /\]/) inarr = 0
    }
  ' "$f"
}

# read_pnpm_ignore_ids (the .pnpm-audit-ignore.json "ignore" parse) is provided by the
# shared scripts/lang/_audit_suppressions_lib.sh (sourced at the top) — the sole parse
# locus. sync-check below applies the fail-LOUD policy on a non-zero return; the ts
# wrapper's audit_read_pnpm_suppressions applies fail-SAFE (@dry-reviewer Gate-2).

# dependabot-ignore-check: mechanically enforce that .github/dependabot.yml is NOT
# used as a second suppression surface (security a1 + §F.2). Dependabot is for bump
# PRs only; advisory suppression goes ONLY in audit-suppressions.toml. A non-empty
# `ignore:` block in dependabot.yml would be a shadow suppression list -> FAIL.
#
# Detection: a real (non-comment) `ignore:` key with at least one list entry. Our own
# dependabot.yml mentions `ignore:` in a prohibition COMMENT — comment lines (leading
# `#`) are excluded so the prohibition text does not self-trip.
# Returns 0 if clean (no real non-empty ignore block), 1 if a populated ignore block exists.
__dependabot_ignore_check() {
  local f="${REPO_ROOT}/.github/dependabot.yml"
  [[ -f "$f" ]] || return 0
  # awk exits 0 when a populated `ignore:` list entry is found (the VIOLATION), else 1.
  if awk '
    { line = $0 }
    # Skip comment-only lines (our prohibition text mentions ignore: in a comment).
    line ~ /^[ \t]*#/ { next }
    # A real ignore: key (tolerating an inline trailing comment).
    line ~ /^[ \t]*ignore:[ \t]*(#.*)?$/ { in_ignore = 1; next }
    in_ignore {
      if (line ~ /^[ \t]*$/) next           # blank line inside block — keep scanning
      if (line ~ /^[ \t]*-/) { found = 1; exit }  # a list entry -> populated -> violation
      in_ignore = 0                          # any other key -> ignore: was empty
    }
    # Single exit point: an in-rule `exit` falls through to END, so decide here.
    END { exit !found }
  ' "$f"; then
    echo "FAIL: .github/dependabot.yml contains a non-empty 'ignore:' block — Dependabot is for bump PRs ONLY and must NOT be a suppression surface. The ONLY sanctioned suppression channel is audit-suppressions.toml (ADR-0033 §11). See docs/contributor/audit-suppressions.md." >&2
    return 1
  fi
  return 0
}

# sync-check: manifest id-set per ecosystem must EXACTLY equal the derived files'
# id-sets (both directions). Drift -> FAIL. A malformed derived file -> FAIL (the
# CHECK side is fail-loud; the ts wrapper's runtime FILTER is the fail-safe side).
__sync_check() {
  local failed=0 rec id ecosystem
  local -a rust_ids=() js_ids=()
  for rec in "${__RECORDS[@]:-}"; do
    [[ -n "$rec" ]] || continue
    id="$(__field "$rec" 1)"; ecosystem="$(__field "$rec" 2)"
    [[ "$ecosystem" == "rust" ]] && rust_ids+=("$id")
    [[ "$ecosystem" == "js" ]] && js_ids+=("$id")
  done

  local cargo_toml pnpm_json
  cargo_toml="$(__cargo_audit_toml_path)"
  pnpm_json="$(__pnpm_ignore_json_path)"

  local derived_rust derived_js
  derived_rust="$(read_cargo_audit_ignore_ids "$cargo_toml")"
  if ! derived_js="$(read_pnpm_ignore_ids "$pnpm_json")"; then
    echo "FAIL: ${pnpm_json} is unparseable (malformed JSON)" >&2
    return 1
  fi

  # Compare sorted id sets.
  if ! diff <(printf '%s\n' "${rust_ids[@]:-}" | grep -v '^$' | sort -u) \
            <(printf '%s\n' "$derived_rust" | grep -v '^$' | sort -u) >/dev/null; then
    echo "FAIL: rust suppression ids drifted between audit-suppressions.toml and .cargo/audit.toml — run scripts/audit-suppressions-check.sh --fix" >&2
    failed=1
  fi
  if ! diff <(printf '%s\n' "${js_ids[@]:-}" | grep -v '^$' | sort -u) \
            <(printf '%s\n' "$derived_js" | grep -v '^$' | sort -u) >/dev/null; then
    echo "FAIL: js suppression ids drifted between audit-suppressions.toml and .pnpm-audit-ignore.json — run scripts/audit-suppressions-check.sh --fix" >&2
    failed=1
  fi
  return $failed
}

# -----------------------------------------------------------------------------
# --fix: regenerate derived files from the manifest (deterministic).
# -----------------------------------------------------------------------------
__regenerate() {
  local cargo_toml pnpm_json manifest
  cargo_toml="$(__cargo_audit_toml_path)"
  pnpm_json="$(__pnpm_ignore_json_path)"
  manifest="$(__manifest_path)"

  local -a rust_ids=() js_ids=()
  local rec id ecosystem
  for rec in "${__RECORDS[@]:-}"; do
    [[ -n "$rec" ]] || continue
    id="$(__field "$rec" 1)"; ecosystem="$(__field "$rec" 2)"
    [[ "$ecosystem" == "rust" ]] && rust_ids+=("$id")
    [[ "$ecosystem" == "js" ]] && js_ids+=("$id")
  done

  mkdir -p "$(dirname "$cargo_toml")"

  # .cargo/audit.toml — cargo-audit native + verbatim rationale comments (mandatory,
  # security: a bare ignore=[...] with no rationale at the point of suppression is a
  # finding). The rationale lines are lifted from the manifest's #-comment block.
  {
    echo "# GENERATED from audit-suppressions.toml — DO NOT EDIT."
    echo "# Regenerate: scripts/audit-suppressions-check.sh --fix"
    echo "# Suppression is sourced ONLY from the manifest (ADR-0033 §11)."
    echo "#"
    echo "# Per-id rationale (mirrored from audit-suppressions.toml):"
    for rec in "${__RECORDS[@]:-}"; do
      [[ -n "$rec" ]] || continue
      ecosystem="$(__field "$rec" 2)"
      [[ "$ecosystem" == "rust" ]] || continue
      id="$(__field "$rec" 1)"
      # Entry-agnostic expiry line: the entry-specific verify command lives in the
      # reason line above (task #48 fix — the previous hardcoded rsa-specific text
      # was wrong for every entry except RUSTSEC-2023-0071 once a second entry landed).
      echo "#   ${id}: $(__field "$rec" 5)"
      echo "#     expires $(__field "$rec" 3) — re-verify per the reason line above before renewing; renewal workflow: docs/contributor/audit-suppressions.md."
    done
    echo ""
    echo "[advisories]"
    if [[ ${#rust_ids[@]} -eq 0 ]]; then
      echo "ignore = []"
    else
      printf 'ignore = ['
      local first=1 rid
      for rid in "${rust_ids[@]}"; do
        if [[ $first -eq 1 ]]; then first=0; else printf ', '; fi
        printf '"%s"' "$rid"
      done
      printf ']\n'
    fi
  } > "$cargo_toml"

  # .pnpm-audit-ignore.json — custom post-hoc filter list (JSON has no comments, so a
  # "_generated" marker key carries the do-not-edit notice). Deterministic field order.
  {
    printf '{\n'
    printf '  "_generated": "from audit-suppressions.toml — DO NOT EDIT; run scripts/audit-suppressions-check.sh --fix",\n'
    printf '  "ignore": ['
    if [[ ${#js_ids[@]} -gt 0 ]]; then
      printf '\n'
      local first=1 jid
      for jid in "${js_ids[@]}"; do
        if [[ $first -eq 1 ]]; then first=0; else printf ',\n'; fi
        printf '    "%s"' "$jid"
      done
      printf '\n  ]\n'
    else
      printf ']\n'
    fi
    printf '}\n'
  } > "$pnpm_json"

  echo "regenerated: ${cargo_toml}, ${pnpm_json} (from ${manifest})" >&2
}

# -----------------------------------------------------------------------------
# Main
# -----------------------------------------------------------------------------
main() {
  local do_fix=0
  case "${1:-}" in
    --fix) do_fix=1 ;;
    "") : ;;
    *) echo "usage: $0 [--fix]" >&2; emit_status FAIL "suppression-bad-usage"; return 2 ;;
  esac

  # Trust-boundary guard (§J A+B): an override env present without the EXACT sentinel
  # is a tamper/misconfig signal — fail loud, never silently honor or silently ignore.
  if __any_override_present && ! __test_sentinel_active; then
    echo "FAIL: a test-injection override env is set but DEVLOOP_TEST != \"1\" — refusing to honor it (possible CI env injection). Investigate; do not unset-and-rerun blindly." >&2
    emit_status FAIL "suppression-override-without-test-sentinel"
    return 1
  fi

  # Parse once (fail-closed loud on malformed).
  if ! __load_records; then
    echo "FAIL: audit-suppressions.toml is present but unparseable (see MALFORMED lines above)" >&2
    emit_status FAIL "suppression-malformed"
    return 1
  fi

  if [[ $do_fix -eq 1 ]]; then
    __regenerate
  fi

  # Run all checks; accumulate. Each prints its own diagnostics to stderr.
  local date_rc=0 sync_rc=0 quality_rc=0 depbot_rc=0
  __date_check             || date_rc=1
  __quality_check          || quality_rc=1
  __sync_check             || sync_rc=1
  __dependabot_ignore_check || depbot_rc=1

  # Emit the worst single token (all FAILs are non-zero; we surface the most
  # actionable first). dependabot-ignore is a SSOT-integrity violation, ranked with
  # the other suppression-policy failures.
  if [[ $date_rc -eq 1 ]]; then
    emit_status FAIL "suppression-past-due"; return 1
  fi
  if [[ $sync_rc -eq 1 ]]; then
    emit_status FAIL "suppression-drift"; return 1
  fi
  if [[ $quality_rc -eq 1 ]]; then
    emit_status FAIL "suppression-quality"; return 1
  fi
  if [[ $depbot_rc -eq 1 ]]; then
    emit_status FAIL "dependabot-ignore-present"; return 1
  fi

  emit_status OK "suppressions-clean"
  return 0
}

main "$@"
