#!/usr/bin/env bash
# Proto test: intentional-gap placeholder (task #52).
#
# Proto has no unit-test phase — `buf` contract checks (compile/lint/breaking) are its
# gates. This one-line placeholder registers that gap at the FILESYSTEM level so the
# dispatcher sees an executable verb wrapper (emitting N/A) instead of a missing one
# (which is now always a FAIL-MISSING-VERB wiring fault). To register a future
# intentional gap for another lang, drop an identical placeholder in its dir.
#
# No install_wrapper_exit_trap: it emits immediately and cannot abort before emitting.
set -euo pipefail
IFS=$'\n\t'
source "$(dirname "${BASH_SOURCE[0]}")/../_common.sh"
emit_status N/A not-applicable-to-this-lang
