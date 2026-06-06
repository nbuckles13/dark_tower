#!/usr/bin/env bash
# _audit_suppressions_lib.sh — shared low-level readers for the audit-suppression
# derived files. Sourceable (no top-level main), idempotent-source-guarded.
#
# Extracted (task #47, Gate-2 @dry-reviewer finding) because the .pnpm-audit-ignore.json
# "ignore"-list PARSE was implemented twice with identical bodies — once in
# audit-suppressions-check.sh (sync-check, fail-LOUD on malformed) and once in
# _audit_gate.sh (ts-filter, fail-SAFE on malformed). The PARSE is identical; only the
# post-parse error POLICY differs (intentional, security §D.1.2 PATH-1). So the parse
# lives here as ONE pure function; each caller applies its own fail-policy on top.

set -euo pipefail
IFS=$'\n\t'

[[ -n "${__DEVLOOP_AUDIT_SUPPRESSIONS_LIB_SH:-}" ]] && return 0
readonly __DEVLOOP_AUDIT_SUPPRESSIONS_LIB_SH=1

# Pure reader for a .pnpm-audit-ignore.json file's "ignore" id list.
# Args: $1 = path to the .pnpm-audit-ignore.json
# Outputs: stdout = one id per line (none if the file is absent or ignore=[])
# Returns: 0 on clean read (incl. absent file = zero ids); 1 on MALFORMED
#          (unreadable / invalid JSON / "ignore" not a list).
# NOTE: this function applies NO error policy — callers decide fail-loud vs fail-safe
# on a non-zero return. Absent file is NOT malformed (legitimate zero suppressions).
read_pnpm_ignore_ids() {
  local f="$1"
  [[ -f "$f" ]] || return 0
  python3 - "$f" <<'PY' 2>/dev/null || return 1
import json, sys
with open(sys.argv[1]) as fh:
    data = json.load(fh)
ids = data.get("ignore", [])
if not isinstance(ids, list):
    raise SystemExit(1)
for i in ids:
    print(i)
PY
}
