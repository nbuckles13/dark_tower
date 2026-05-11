#!/usr/bin/env bash
# Audit dispatcher: invokes lang/<X>/audit.sh always-run (ADR-0033 §3 + §6),
# then invokes proto's lang/proto/breaking.sh unconditionally per §10:397.
#
# Proto deliberately has no audit.sh (§1:96 + §6:236) — breaking.sh IS the
# proto audit gate, wired here (not via thin wrapper) so dispatch emits the
# §6 SKIPPED-NO-VERB signal for proto and breaking.sh runs independently.
set -euo pipefail
IFS=$'\n\t'
source "$(dirname "$0")/lang/_dispatch.sh"

dispatch_rc=0
DEVLOOP_DISPATCH_ALWAYS_RUN=1 for_each_lang_with_verb "audit" "$@" || dispatch_rc=$?

breaking_rc=0
"$(dirname "$0")/lang/proto/breaking.sh" || breaking_rc=$?

# Worst exit code wins. Numeric max is correct because status_to_exit_code()
# maps 0=OK/SKIPPED, 1=FAIL, 2=UNKNOWN — monotonic, so max == worst per the
# STATUS-precedence contract. A `set -e` short-circuit would mask the second
# invocation and break the always-run guarantee; explicit RC capture preserves
# both gates.
exit "$(( dispatch_rc > breaking_rc ? dispatch_rc : breaking_rc ))"
