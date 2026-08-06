#!/usr/bin/env bash
# TS audit: pnpm audit --audit-level=high, DEP-CHANGE-GATED (ADR-0033 §3 amendment,
# task #47) with a post-hoc suppression filter.
#
# pnpm has no native ignore file, so suppression is applied here: we run
# `pnpm audit --json`, drop advisories whose GHSA/RUSTSEC id is in the tracked,
# generated .pnpm-audit-ignore.json, then FAIL only if non-suppressed advisories
# at/above the threshold remain.
#
# IMPORTANT (security finding 1, PRESERVED + EXTENDED): we deliberately do NOT pass
# "$@" through to pnpm audit. Threshold and ignore-list edits are security's domain
# (ADR-0033 §11). The ONLY sanctioned suppression channel is audit-suppressions.toml
# -> generated .pnpm-audit-ignore.json — never an ad-hoc CLI flag (`--ignore=GHSA-…`),
# never a hand-edited derived file, never a Dependabot alert dismissal. The filter
# reads ONLY the tracked derived file, never CLI/env.
#
# GATE (fail-closed tri-state, _audit_gate.sh): runs the scan if deps changed OR the
# diff is indeterminate; emits SKIPPED-NO-DIFF only on a PROVEN no-dep-change. The gate
# matches ONLY true dep manifests (root package.json/pnpm-lock.yaml/pnpm-workspace.yaml +
# packages/*/package.json) — a packages/-SOURCE-only edit now SKIPS (it cannot move the
# resolved dep graph); an indeterminate diff still RUNS. DEVLOOP_AUDIT_FORCE_RUN=1 forces
# a run (weekly scheduled scan); force-RUN-only.
#
# FILTER fail-mode (security §D.1.2 PATH-1): if .pnpm-audit-ignore.json is malformed,
# the filter applies ZERO suppressions and PROCEEDS (every advisory surfaces; the scan
# can still go RED) + emits a loud stderr WARN. It MUST NOT abort the scan — a
# filter-abort on a malformed ignore-file is denial-of-coverage. audit-suppressions-check
# hard-fails the same malformed file separately.
set -euo pipefail
IFS=$'\n\t'
source "$(dirname "${BASH_SOURCE[0]}")/../_audit_gate.sh"
install_wrapper_exit_trap  # task #50: emit STATUS=FAIL if we abort before emitting

gate_rc=0
audit_gate audit_dep_changed_ts || gate_rc=$?
if [[ "$gate_rc" -eq 1 ]]; then
  emit_status SKIPPED-NO-DIFF no-dep-changes
  exit 0
fi

# Deps changed (0) or indeterminate (>=2): RUN. Capture pnpm audit JSON (it exits
# non-zero when advisories are found; we make our own pass/fail decision after
# filtering, so don't let its exit abort us).
audit_json=""
pnpm_rc=0
audit_json="$(pnpm audit --audit-level=high --json 2>/dev/null)" || pnpm_rc=$?

# Suppressed id list from the tracked derived file (fail-safe on malformed: empty).
mapfile -t suppressed < <(audit_read_pnpm_suppressions)

# Decide pass/fail: count advisories at/above the threshold whose id is NOT suppressed.
# pnpm audit --json emits an `advisories` object (npm-audit v1) or `vulnerabilities`
# (v2); we handle both, matching on the advisory's GHSA/CVE id and severity. The
# threshold is high (so high+critical count). Suppressed ids drop out.
decision="$(python3 - "$pnpm_rc" "${suppressed[@]+"${suppressed[@]}"}" <<'PY'
import json, sys
pnpm_rc = sys.argv[1]
suppressed = set(sys.argv[2:])
raw = sys.stdin.read().strip()
if not raw:
    # No JSON (e.g. pnpm not installed / empty) — if pnpm exited non-zero with no
    # parseable output, surface that as a fail; else treat as clean.
    print("FAIL no-json" if pnpm_rc != "0" else "OK")
    sys.exit(0)
try:
    data = json.loads(raw)
except Exception:
    print("FAIL bad-json")
    sys.exit(0)

THRESH = {"high", "critical"}
applied = set()
remaining = []

def consider(adv_id, severity):
    if severity not in THRESH:
        return
    if adv_id in suppressed:
        applied.add(adv_id)
    else:
        remaining.append(adv_id)

# npm-audit v1 shape: {"advisories": {id: {"severity":..., "github_advisory_id":...}}}
for _, adv in (data.get("advisories") or {}).items():
    sev = (adv.get("severity") or "").lower()
    aid = adv.get("github_advisory_id") or adv.get("cves", [None])[0] or str(adv.get("id"))
    consider(aid, sev)

# npm-audit v2 shape: {"vulnerabilities": {name: {"severity":..., "via":[{...}]}}}
for _, v in (data.get("vulnerabilities") or {}).items():
    sev = (v.get("severity") or "").lower()
    for via in (v.get("via") or []):
        if isinstance(via, dict):
            aid = via.get("url", "").rstrip("/").split("/")[-1] or via.get("name")
            consider(aid, (via.get("severity") or sev).lower())

print(("FAIL" if remaining else "OK"),
      "applied=" + ",".join(sorted(applied)),
      "remaining=" + ",".join(sorted(set(remaining))))
PY
)"

# Surface applied suppressions (observability SUPPRESSED= line) when any were hit.
applied_ids="$(printf '%s\n' "$decision" | sed -n 's/.*applied=\([^ ]*\).*/\1/p')"
[[ -n "$applied_ids" ]] && echo "SUPPRESSED=${applied_ids}" >&2

if printf '%s\n' "$decision" | grep -q '^FAIL'; then
  remaining_ids="$(printf '%s\n' "$decision" | sed -n 's/.*remaining=\([^ ]*\).*/\1/p')"
  echo "pnpm audit: unsuppressed high/critical advisories remain: ${remaining_ids:-<see pnpm audit output>}" >&2
  emit_status FAIL "pnpm-audit-failed"
  exit 1
fi
emit_status OK "pnpm-audit-passed"
