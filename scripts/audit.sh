#!/usr/bin/env bash
# Audit dispatcher: invokes lang/<X>/audit.sh always-run (ADR-0033 §3 + §6),
# then invokes proto's lang/proto/breaking.sh unconditionally per §10:397.
#
# Proto has no real dependency-vuln audit — breaking.sh IS the proto audit gate,
# wired here (not via thin wrapper) so it runs independently. proto/audit.sh is an
# intentional-gap placeholder emitting N/A (task #52), so dispatch records a benign
# N/A for proto rather than treating it as a missing verb.
set -euo pipefail
IFS=$'\n\t'
source "$(dirname "$0")/lang/_dispatch.sh"

# Audit dispatch, always-run (ADR-0033 §3). Stream STATUS lines straight to stdout so
# the layer's `audit.sh 2>&1 | tee_collect_statuses` (layer6.sh) collects them. A deleted
# or chmod-stripped `<lang>/audit.sh` now natively reds the layer: the dispatcher emits
# FAIL-MISSING-VERB (rank 5), which beats a sibling's OK in aggregate_worst_status → the
# layer aggregate is FAIL-MISSING-VERB → exit 2. No post-processing/tee/grep needed
# (task #52 retired #50's audit-slice workaround — the ladder closes the masking).
dispatch_rc=0
DEVLOOP_DISPATCH_ALWAYS_RUN=1 for_each_lang_with_verb "audit" "$@" || dispatch_rc=$?

breaking_rc=0
"$(dirname "$0")/lang/proto/breaking.sh" || breaking_rc=$?

# Worst exit code wins. Numeric max is correct because status_to_exit_code()
# maps 0=OK/SKIPPED/N/A, 1=FAIL, 2=FAIL-MISSING-VERB/UNKNOWN — monotonic, so max ==
# worst per the STATUS-precedence contract. A `set -e` short-circuit would mask the
# second invocation and break the always-run guarantee; explicit RC capture preserves
# both gates.
exit "$(( dispatch_rc > breaking_rc ? dispatch_rc : breaking_rc ))"
