#!/usr/bin/env bash
# Rust audit: cargo audit, DEP-CHANGE-GATED (ADR-0033 §3 amendment, task #47).
#
# Suppression is sourced ONLY from the tracked, generated .cargo/audit.toml (which
# cargo-audit reads NATIVELY at the repo's .cargo/ — no flag needed). That derived
# file is regenerated from audit-suppressions.toml by audit-suppressions-check.sh
# --fix; it is never hand-edited (sync-check enforces this).
#
# IMPORTANT (security finding 1, PRESERVED + EXTENDED): we deliberately do NOT pass
# "$@" through to cargo audit. Threshold/allowlist edits and `--ignore` flags are
# security's domain (ADR-0033 §11). The ONLY sanctioned suppression channel is
# audit-suppressions.toml -> generated .cargo/audit.toml — never an ad-hoc CLI flag,
# never a hand-edited derived file, never a Dependabot alert dismissal. Allowing CLI
# pass-through would let a caller silence advisories at runtime without leaving a
# trace in the tracked manifest. If a future caller needs a legitimate arg (e.g.
# `--db /custom/path`), add an explicit allowlist of safe flags, not blanket pass-through.
#
# GATE (fail-closed tri-state, _audit_gate.sh): runs the scan if deps changed OR the
# diff is indeterminate; emits SKIPPED-NO-DIFF only on a PROVEN no-dep-change. The gate
# matches ONLY true dep manifests (root Cargo.toml/Cargo.lock + crates/*/Cargo.toml) —
# a crates/-SOURCE-only edit now SKIPS (it cannot move the resolved dep graph); an
# indeterminate diff still RUNS. DEVLOOP_AUDIT_FORCE_RUN=1 forces a run (weekly
# scheduled scan); force-RUN-only.
set -euo pipefail
IFS=$'\n\t'
source "$(dirname "${BASH_SOURCE[0]}")/../_audit_gate.sh"
install_wrapper_exit_trap  # task #50: emit STATUS=FAIL if we abort before emitting

gate_rc=0
audit_gate audit_dep_changed_rust || gate_rc=$?
if [[ "$gate_rc" -eq 1 ]]; then
  # Proven no dep-manifest change. Non-dominating skip (SKIPPED-NO-DIFF ranks below
  # OK so a sibling's real OK still wins). NOT N/A (which would dominate). See
  # ADR-0033 §3 amendment + docs/runbooks/devloop-validation.md §6.6.
  emit_status SKIPPED-NO-DIFF no-dep-changes
  exit 0
fi

# Deps changed (0) or indeterminate (>=2): RUN. cargo-audit applies the tracked
# .cargo/audit.toml ignore list NATIVELY; suppressed advisories drop out of its
# failure decision. Surface the configured suppression set for 3am legibility
# (observability): cargo-audit does not report which ignores it hit, so this is the
# tracked ignore list in effect this run, not a per-hit list — but it makes "an
# advisory is being silenced here, here's its id" greppable without re-deriving from
# the manifest. No line is emitted when nothing is configured to suppress.
__cargo_audit_toml="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)/.cargo/audit.toml"
if [[ -f "$__cargo_audit_toml" ]]; then
  applied="$(grep -oE 'RUSTSEC-[0-9]{4}-[0-9]{4}' "$__cargo_audit_toml" | sort -u | paste -sd, -)"
  [[ -n "$applied" ]] && echo "SUPPRESSED=${applied}" >&2
fi

run_and_emit "cargo-audit" cargo audit
