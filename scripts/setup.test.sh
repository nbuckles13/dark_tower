#!/usr/bin/env bash
# setup.test.sh — self-test for infra/kind/scripts/setup.sh's disk precondition guard.
#
# The keystone of this devloop (.dockerignore + cleanup GC) is self-validated by Gate-2
# Layer 7's cluster rebuild — but that rebuild only exercises the disk guard's PASS path
# (the host has space → the guard returns 0). The guard's WHOLE POINT — emitting
# `PRECONDITION_FAILURE: … REASON=insufficient-disk` and `exit 2` — is a documented operator
# contract (docs/runbooks/devloop-validation.md §6.7 + the §4 two-token convention) that
# Gate 2 never reaches. Without this test a future refactor could drop/rename the token or
# break exit 2 unnoticed.
#
# REAL-input test (ADR-0034 — no mocked internals): we force DEVLOOP_MIN_DISK_GB absurdly
# high so the comparison trips on any real filesystem, then assert exit 2 + the line-anchored
# token. One case covers override-read + graphroot fallback + df + arithmetic + comparison +
# banner + exit. setup.sh's `BASH_SOURCE[0]==$0` guard lets us source it without running main.
#
# Wired into scripts/layer3.sh (there is no *.test.sh auto-runner), so it runs every devloop
# + CI. Consumes scripts/lang/_test_helpers.sh (assert_rc/assert_status/report_results).
set -euo pipefail
IFS=$'\n\t'

__here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${__here}/.." && pwd)"
SETUP="${REPO_ROOT}/infra/kind/scripts/setup.sh"
# shellcheck source=lang/_test_helpers.sh
source "${__here}/lang/_test_helpers.sh"

# The guard deliberately `exit 2`s on the trip lane; run each case in its own `bash -c`
# subshell (fresh _DT_DISK_CHECKED sentinel + isolated exit) and capture rc/output, so the
# harness's set -e must not abort. report_results sets the final code.
set +e

# Source setup.sh (BASH_SOURCE guard suppresses main), then call the guard. $1=setup path,
# $2=runtime arg. `set --` clears the bash -c positional params BEFORE sourcing — otherwise
# the sourced setup.sh inherits them as its own args and its arg-parser `exit 1`s on the
# unknown "option" (the setup path). Source noise is discarded; the guard's stderr banner is
# what we capture.
RUN_GUARD='sp="$1"; rt="$2"; set --; source "$sp" >/dev/null 2>&1; check_build_disk_space "$rt"'

# === (1) forced TRIP: min-disk floor absurdly high → comparison trips on the real fs ========
# Real df on the real graphroot (or its PROJECT_ROOT fallback — always df-able, so avail_gb
# is always populated and the huge floor always trips, even on a host without podman).
trip_out="$(DEVLOOP_MIN_DISK_GB=999999999 bash -c "$RUN_GUARD" _ "$SETUP" podman 2>&1)"
trip_rc=$?
assert_rc "trip-exit2" 2 "$trip_rc"
# The operator contract: line-anchored, greppable by the §4 one-pass scan.
grep -Eq '^PRECONDITION_FAILURE:.*REASON=insufficient-disk' <<<"$trip_out"
assert_rc "trip-banner-anchored" 0 $?
assert_status "trip-remediation-hint" "podman image prune -f" "$trip_out"

# === (2) PASS path: a 0 GB floor cannot trip (avail_gb >= 0) → exit 0, no banner ============
# Guards against a regression where the guard trips unconditionally (which would still pass
# case 1). Proves the comparison is real, not always-fail.
pass_out="$(DEVLOOP_MIN_DISK_GB=0 bash -c "$RUN_GUARD" _ "$SETUP" podman 2>&1)"
pass_rc=$?
assert_rc "pass-exit0" 0 "$pass_rc"
grep -Eq 'PRECONDITION_FAILURE' <<<"$pass_out"
assert_rc "pass-no-banner" 1 $?  # grep exit 1 == token NOT present (correct)

report_results "scripts/setup.test.sh"
