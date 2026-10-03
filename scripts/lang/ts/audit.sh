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
# pnpm audit --json emits an `advisories` object; we match on each advisory's GHSA/CVE id
# and severity. The threshold is high (so high+critical count). Suppressed ids drop out.
# The PROGRAM is passed with `python3 -c` and the audit JSON on stdin. (Until 2026-10-03
# both went to stdin — `python3 - <<'PY'` — so Python consumed stdin as its program,
# `sys.stdin.read()` returned "", and every scan was decided on pnpm's exit code alone
# with the suppression filter never seeing an advisory.) stdin has no size cap; an
# argv/env string is capped at 128 KiB, which a large audit report exceeds.
read -r -d '' __audit_decide_py <<'PY' || true
import json, sys
pnpm_rc = sys.argv[1]
suppressed = set(sys.argv[2:])
raw = sys.stdin.read().strip()
if not raw:
    # No JSON at all. A healthy `pnpm audit --json` always prints an object (a clean
    # tree is `{"advisories":{},...}`), so empty stdout — whatever pnpm's exit code —
    # means nothing was evaluated: never a pass, and not an advisory finding either.
    print("FAIL no-json")
    sys.exit(0)
try:
    data = json.loads(raw)
except Exception:
    print("FAIL bad-json")
    sys.exit(0)

# Fail CLOSED on a shape we do not recognise. The filter below reads only the
# `advisories` object — the shape pnpm (12.x, bulk advisory endpoint) emits; any other
# shape would yield zero considered advisories and print OK — indistinguishable from a
# clean scan. An empty `advisories: {}` IS recognised (clean). (An npm-audit v2
# `vulnerabilities` parser lived here; pnpm never emits it and it was never exercised,
# so it was removed: that shape now fails as unrecognised rather than being parsed by
# untested code.)
if not isinstance(data, dict) or "advisories" not in data:
    print("FAIL unrecognised-shape")
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
    aid = adv.get("github_advisory_id") or (adv.get("cves") or [None])[0] or str(adv.get("id"))
    consider(aid, sev)

print(("FAIL" if remaining else "OK"),
      "applied=" + ",".join(sorted(applied)),
      "remaining=" + ",".join(sorted(set(remaining))))
PY
decision="$(printf '%s' "$audit_json" | python3 -c "$__audit_decide_py" "$pnpm_rc" "${suppressed[@]+"${suppressed[@]}"}")" || true

# Surface applied suppressions (observability SUPPRESSED= line) when any were hit.
applied_ids="$(printf '%s\n' "$decision" | sed -n 's/.*applied=\([^ ]*\).*/\1/p')"
[[ -n "$applied_ids" ]] && echo "SUPPRESSED=${applied_ids}" >&2

if printf '%s\n' "$decision" | grep -q '^FAIL no-json'; then
  echo "pnpm audit: --json printed nothing (pnpm rc=${pnpm_rc}) — no report to evaluate. This is not an advisory finding; do not suppress." >&2
  emit_status FAIL "pnpm-audit-unrecognised-output"
  exit 1
fi
if printf '%s\n' "$decision" | grep -q '^FAIL unrecognised-shape'; then
  # Its own token, distinct from pnpm-audit-failed: the remedy is to teach the parser the
  # new shape, NEVER to suppress an advisory.
  echo "pnpm audit: --json output has no 'advisories' object — the parser does not recognise this shape; update scripts/lang/ts/audit.sh, do not suppress." >&2
  emit_status FAIL "pnpm-audit-unrecognised-output"
  exit 1
fi
if printf '%s\n' "$decision" | grep -q '^FAIL'; then
  remaining_ids="$(printf '%s\n' "$decision" | sed -n 's/.*remaining=\([^ ]*\).*/\1/p')"
  echo "pnpm audit: unsuppressed high/critical advisories remain: ${remaining_ids:-<see pnpm audit output>}" >&2
  emit_status FAIL "pnpm-audit-failed"
  exit 1
fi
# Pass ONLY on an explicit OK decision: an empty or garbled decision (python missing,
# killed, printed nothing) is never read as clean.
if ! printf '%s\n' "$decision" | grep -q '^OK'; then
  echo "pnpm audit: the decision step produced no verdict (got: '${decision}') — the scan was not evaluated." >&2
  emit_status FAIL "pnpm-audit-no-decision"
  exit 1
fi
emit_status OK "pnpm-audit-passed"
