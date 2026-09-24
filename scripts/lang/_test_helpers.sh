#!/usr/bin/env bash
# _test_helpers.sh — shared assertion scaffolding for the pipeline's shell
# self-tests (`scripts/**/*.test.sh`).
#
# Hosts the shared PASS/FAIL counter + assert_* helpers (assert_rc/assert_exit,
# assert_status, assert_marker/assert_no_marker/assert_absent, report_results)
# that the pipeline's shell self-tests source rather than re-implement. Consumed
# broadly by the layer/orchestrator/workflow/guard self-tests (layer7, layer-all,
# run-story, setup, audit-suppressions, _audit_gate, _changed_helpers,
# validate-*-sync). A helper with a single consumer belongs in that consumer,
# not here.

set -euo pipefail
IFS=$'\n\t'

[[ -n "${__DEVLOOP_TEST_HELPERS_SH:-}" ]] && return 0
readonly __DEVLOOP_TEST_HELPERS_SH=1

# State (initialized at source).
PASS=0
FAIL=0
FAILURES=()

# ---------------------------------------------------------------------------
# A MISSING HELPER MUST BE A FAILURE, NOT A SKIPPED ASSERTION.
# ---------------------------------------------------------------------------
#
# Harnesses run under `set +e` so that expected non-zero exits don't abort them.
# That also means a call to a helper that does not exist — a typo'd `assert_rc`,
# or one renamed out from under its caller — prints "command not found", moves
# NEITHER counter, and the suite reports "N passed, 0 failed" and exits 0: the
# assertion never ran and the harness reads clean. That has already happened
# once (an `assert_eq` that was never defined, in scripts/setup.test.sh).
#
# WHY A FILE AND NOT `FAIL=$((FAIL+1))`: bash invokes `command_not_found_handle`
# in a SEPARATE EXECUTION ENVIRONMENT, so a counter incremented inside it is
# discarded on return — the obvious version of this fix is itself vacuous. The
# handler records the missing name in a file; `report_results` folds it into
# FAIL/FAILURES. Scope: the harness's own shell only. A `bash -c` child does not
# inherit the function, which is correct — those children are code under test.
__DT_HARNESS_MISSING="$(mktemp "${TMPDIR:-/tmp}/dt-harness-missing.XXXXXX")"
command_not_found_handle() {
  printf '%s\n' "$1" >>"$__DT_HARNESS_MISSING"
  printf 'bash: %s: command not found (counted as a harness FAILURE)\n' "$1" >&2
  return 127
}

# Increment PASS/FAIL; record failure context.
# Args: $1=label  $2=expected-exit-code  $3=actual-exit-code
# Returns: 0 always
assert_rc() {
  local label="$1" expected="$2" actual="$3"
  if [[ "$actual" == "$expected" ]]; then
    PASS=$((PASS + 1))
  else
    FAIL=$((FAIL + 1))
    FAILURES+=("[${label}] expected_rc=${expected} actual_rc=${actual}")
  fi
}

# Increment PASS/FAIL on exact exit-code match (alias of assert_rc with clearer
# naming for STATUS-emitting tools; same contract). Args: $1=label $2=expected $3=actual.
assert_exit() { assert_rc "$@"; }

# Increment PASS/FAIL on a substring match; record failure context with the haystack.
# For asserting a STATUS=/REASON= line or a message fragment in captured output.
# Args: $1=label  $2=needle (substring)  $3=haystack (captured stdout+stderr)
# Returns: 0 always
assert_status() {
  local label="$1" needle="$2" haystack="$3"
  if [[ "$haystack" == *"$needle"* ]]; then
    PASS=$((PASS + 1))
  else
    FAIL=$((FAIL + 1))
    FAILURES+=("[${label}] expected substring '${needle}' not found in output")
  fi
}

# Increment PASS/FAIL when a substring must be ABSENT; record failure context.
# The negative twin of assert_status — for proving a control STAYS SILENT when it
# should not fire, which a positive-only suite structurally cannot show.
# Args: $1=label  $2=needle (substring that must NOT appear)  $3=haystack
# Returns: 0 always
assert_absent() {
  local label="$1" needle="$2" haystack="$3"
  if [[ "$haystack" != *"$needle"* ]]; then
    PASS=$((PASS + 1))
  else
    FAIL=$((FAIL + 1))
    FAILURES+=("[${label}] unexpected substring '${needle}' WAS present")
  fi
}

# Marker assertions: prove a stub actually EXECUTED (or did not).
#
# Why these exist: a case that asserts only an exit code cannot distinguish "the
# path I meant to test ran" from "another path produced the same status". A
# rejection case in particular passes trivially if the harness is broken — every
# broken fixture also exits non-zero — so the distinguishing evidence is that the
# stubs did NOT run. Stubs drop a marker file when invoked; these check for it.
#
# Parameterized <dir> <glob> deliberately: these were LA_DT-specific closures over
# a hardcoded `ran.layer*` in layer-all.test.sh. Reused verbatim by a second suite
# they would silently always PASS (unset dir => no match => "no stub ran"), which
# is exactly the vacuous assertion they exist to prevent.
#
# `compgen -G` rather than `ls`: glob existence without an unquoted expansion.
# Args: $1=label  $2=directory  $3=glob (quoted at the call site, expanded here)
# Returns: 0 always
assert_marker() {
  local label="$1" dir="$2" glob="$3"
  if compgen -G "${dir}/${glob}" >/dev/null 2>&1; then
    PASS=$((PASS + 1))
  else
    FAIL=$((FAIL + 1))
    FAILURES+=("[${label}] no stub ran (${dir}/${glob} absent) but it should have")
  fi
}

assert_no_marker() {
  local label="$1" dir="$2" glob="$3"
  if compgen -G "${dir}/${glob}" >/dev/null 2>&1; then
    FAIL=$((FAIL + 1))
    FAILURES+=("[${label}] a stub RAN (${dir}/${glob} present) but should not have")
  else
    PASS=$((PASS + 1))
  fi
}

# Print final pass/fail summary. Exit 0 if all pass, 1 if any fail.
# Args: $1=label (typically the test file path for runbook context)
# Outputs: stdout=summary line + per-failure detail
report_results() {
  local label="$1"
  # Fold in any command-not-found recorded by the handler above.
  if [[ -s "${__DT_HARNESS_MISSING:-}" ]]; then
    local missing
    while IFS= read -r missing; do
      FAIL=$((FAIL + 1))
      FAILURES+=("[harness] missing command/helper '${missing}' — an assertion that never ran")
    done <"$__DT_HARNESS_MISSING"
  fi
  rm -f "${__DT_HARNESS_MISSING:-}"
  # THE LEADING PREFIXES ARE LOAD-BEARING — do not "clean up" either printf to
  # start a line at column zero.
  #
  # Every consumer of this helper (10 files) runs under `run_and_emit` inside a
  # layer script, whose stdout is piped to `tee_collect_statuses`. That collector
  # matches `^STATUS=` at LINE START, and each match becomes a VOTE in the
  # enclosing layer's verdict via `aggregate_worst_status`. A FAILURES[] entry
  # routinely contains the text `STATUS=...` — any assertion about a STATUS line
  # quotes one.
  #
  # TWO INDEPENDENT CONTROLS hold this, and neither may be removed on the
  # grounds that the other suffices: the `'  - '` prefix here, and the
  # `[${label}]` prefix that every FAILURES+= producer in this file uses. Today
  # the label convention is the one actually doing the work, since no standard
  # helper can emit an entry starting with `STATUS=` — but it is a convention,
  # not a mechanism: callers append to FAILURES directly rather than through any
  # helper (layer-all.test.sh does so twice, in its non-demotion-floor and
  # sentinel-token cases), so nothing enforces it at those sites. Drop the
  # prefix here and the property survives only as long as every future consumer
  # remembers a convention no code checks.
  #
  # Unprefixed, a test's own failure DIAGNOSTIC would be parsed as a status
  # emission: a suite reporting `expected STATUS=OK` could vote OK into a red
  # layer, or vote FAIL-MISSING-VERB and exit the pipeline 2. The property is
  # invisible from both ends — the collector never sees this file, and this file
  # never names the collector — and whoever breaks it will be editing here, not
  # reading the layer. Asserted in _changed_helpers.test.sh; see the
  # `report_results` case there.
  printf '\n%s: %d passed, %d failed\n' "$label" "$PASS" "$FAIL"
  if [[ $FAIL -gt 0 ]]; then
    local f
    for f in "${FAILURES[@]}"; do printf '  - %s\n' "$f"; done
    exit 1
  fi
  exit 0
}
