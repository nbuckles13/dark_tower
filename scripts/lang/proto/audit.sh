#!/usr/bin/env bash
# Proto audit: intentional-gap placeholder (task #52).
#
# Proto has no dependency-vuln audit phase — `buf breaking` (run unconditionally from
# scripts/audit.sh) is proto's audit-class gate. This one-line placeholder registers
# that gap at the FILESYSTEM level so the dispatcher sees an executable verb wrapper
# (emitting N/A) instead of a missing one (which is now always a FAIL-MISSING-VERB
# wiring fault). To register a future intentional gap for another lang, drop an
# identical placeholder in its dir AND add it to the N/A emitter allow-list in
# scripts/lang/_dispatch.test.sh (N/A ranks below OK, so the allow-list — not the
# ladder — is what keeps an unexpected N/A from passing quietly).
#
# No install_wrapper_exit_trap: it emits immediately and cannot abort before emitting.
set -euo pipefail
IFS=$'\n\t'
source "$(dirname "${BASH_SOURCE[0]}")/../_common.sh"
emit_status N/A not-applicable-to-this-lang
