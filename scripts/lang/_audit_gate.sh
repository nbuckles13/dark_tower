#!/usr/bin/env bash
# _audit_gate.sh — shared dep-change gate + suppression-filter helpers for the
# per-language audit wrappers (lang/rust/audit.sh, lang/ts/audit.sh).
#
# ADR-0033 §3 AMENDMENT (task #47): `cargo audit` / `pnpm audit` are reclassified
# from unconditional always-run to DEP-CHANGE-GATED. When no dependency manifest is
# in the changed-file set, the wrapper emits SKIPPED-NO-DIFF (non-dominating, exit 0)
# instead of scanning. The always-run discipline moved to the Layer-3
# audit-suppressions-check guard + the weekly scheduled full scan (audit-scheduled.yml).
#
# Gate placement (D-b): the audit dispatch KEEPS DEVLOOP_DISPATCH_ALWAYS_RUN=1 so the
# dispatcher invokes each wrapper unconditionally; the FAIL-CLOSED tri-state gate lives
# HERE, inside the wrapper — never via the dispatcher's changed.sh short-circuit (which
# treats non-zero as "skip" = fail-OPEN, unsafe for a security scan).
#
# Suppression is sourced ONLY from the manifest -> generated derived files, never from
# CLI flags (preserves the Wave-1 no-CLI-pass-through finding, ADR-0033 §11).
#
# Reuses _changed_helpers.sh predicates (diff_touches_root_files / diff_touches_path)
# and _common.sh emit_status — no bespoke diff parsing, no reinvented STATUS lines.

set -euo pipefail
IFS=$'\n\t'

[[ -n "${__DEVLOOP_AUDIT_GATE_SH:-}" ]] && return 0
readonly __DEVLOOP_AUDIT_GATE_SH=1

__audit_gate_here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=_common.sh
source "${__audit_gate_here}/_common.sh"
# shellcheck source=_changed_helpers.sh
source "${__audit_gate_here}/_changed_helpers.sh"

# Repo root (scripts/lang -> repo root is two up).
__audit_repo_root="$(cd "${__audit_gate_here}/../.." && pwd)"

# -----------------------------------------------------------------------------
# FAIL-CLOSED tri-state dep-change gate.
#
# Args: $@ = the predicate invocation (e.g. `audit_dep_changed_rust`) — a function
#            that returns 0=dep-changed, 1=no-dep-change, and is expected to be
#            reliable ONLY after the changed-files cache is populated.
#
# Returns (tri-state):
#   0 = deps changed            -> RUN the scan
#   1 = proven no-dep-change    -> SKIP (caller emits SKIPPED-NO-DIFF)
#   2 = indeterminate           -> RUN the scan (any doubt runs; fail-closed)
#
# DEVLOOP_AUDIT_FORCE_RUN=1 short-circuits to 0 (RUN). It is FORCE-RUN-ONLY: there is
# no value of this var, and no sibling var, that forces a SKIP. The only effect is
# "run even when the gate would skip" (used by the weekly scheduled full scan).
# This is a GATE bypass, NOT a suppression bypass — suppressions still filter.
# -----------------------------------------------------------------------------
audit_gate() {
  # Force-run-only: any set value means RUN; absence falls through to the gate.
  if [[ -n "${DEVLOOP_AUDIT_FORCE_RUN:-}" ]]; then
    return 0
  fi

  # Populate the changed-files cache up front. If that fails, the diff is
  # INDETERMINATE -> run (fail-closed). We do NOT trust the predicate's 0/1 unless
  # the cache is known-good.
  if ! "${__audit_gate_here}/_get_base_ref.sh" >/dev/null 2>&1; then
    echo "# audit-gate: changed-files resolution failed — running scan (fail-closed indeterminate)" >&2
    return 2
  fi

  # Predicate is reliable now. 0=changed (run), 1=unchanged (skip).
  if "$@"; then
    return 0
  else
    return 1
  fi
}

# rust dep-manifest predicate. Over-trigger via diff_touches_path "crates/" is a
# deliberate FAIL-SAFE choice (@dry-reviewer): _changed_helpers.sh has no glob
# predicate, so editing any crate source re-runs the audit. That never produces a
# FALSE skip — a clean SKIPPED-NO-DIFF still guarantees no dep manifest moved.
#
# skip predicate is conservative: a clean SKIPPED-NO-DIFF guarantees no dep manifest
# moved; a crates/-source-only edit RUNS audit (fail-safe over-trigger), it does not skip.
audit_dep_changed_rust() {
  diff_touches_root_files "Cargo.toml" "Cargo.lock" || diff_touches_path "crates/"
}

# ts dep-manifest predicate. pnpm-workspace.yaml included (security): editing it
# changes which packages resolve into the dep graph without touching a package.json.
# packages/ over-trigger is the same fail-safe choice as rust.
#
# skip predicate is conservative: a clean SKIPPED-NO-DIFF guarantees no dep manifest
# moved; a packages/-source-only edit RUNS audit (fail-safe over-trigger), it does not skip.
audit_dep_changed_ts() {
  diff_touches_root_files "package.json" "pnpm-lock.yaml" "pnpm-workspace.yaml" \
    || diff_touches_path "packages/"
}

# -----------------------------------------------------------------------------
# Suppression-filter (ts side). The rust side needs NO filter helper: cargo-audit
# reads .cargo/audit.toml natively (the wrapper passes no flags), so suppression is
# applied by cargo-audit itself from the tracked derived file.
#
# The pure .pnpm-audit-ignore.json PARSE is the shared `read_pnpm_ignore_ids` in
# _audit_suppressions_lib.sh — also consumed by audit-suppressions-check.sh sync-check
# (@dry-reviewer Gate-2: one parse locus, two fail-policies). Here we apply the
# fail-SAFE policy; the check applies fail-LOUD.
# -----------------------------------------------------------------------------
# shellcheck source=_audit_suppressions_lib.sh
source "${__audit_gate_here}/_audit_suppressions_lib.sh"

# Resolve the .pnpm-audit-ignore.json path. Production ALWAYS uses the repo-root file;
# the DEVLOOP_AUDIT_IGNORE_JSON override is honored ONLY under the EXACT test sentinel
# (DEVLOOP_TEST == "1"), mirroring the check script's __pnpm_ignore_json_path (§J A+B).
# This keeps the gate testable the same way the check is, and keeps the production path
# inert to the ambient environment (the always-run guard never sets DEVLOOP_TEST).
__audit_pnpm_ignore_path() {
  if [[ "${DEVLOOP_TEST:-}" == "1" && -n "${DEVLOOP_AUDIT_IGNORE_JSON:-}" ]]; then
    printf '%s\n' "$DEVLOOP_AUDIT_IGNORE_JSON"
  else
    printf '%s\n' "${__audit_repo_root}/.pnpm-audit-ignore.json"
  fi
}

# Read suppressed ids from the generated .pnpm-audit-ignore.json, applying the
# ts-filter FAIL-SAFE policy (security §D.1.2 PATH-1): a malformed/unreadable file ⇒
# apply ZERO suppressions and PROCEED (every advisory surfaces, scan can still go RED)
# + a loud stderr WARN. It must NOT abort the scan — a filter-abort on a malformed
# ignore-file would be denial-of-coverage (worse than the drift). The CHECK side
# hard-fails the same malformed file separately.
audit_read_pnpm_suppressions() {
  local f; f="$(__audit_pnpm_ignore_path)"
  if ! read_pnpm_ignore_ids "$f"; then
    echo "WARN: .pnpm-audit-ignore.json unparseable — applying ZERO suppressions (all advisories will surface); audit-suppressions-check will FAIL separately. Fix the file." >&2
    return 0  # fail-safe: zero suppressions, proceed
  fi
}
