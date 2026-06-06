#!/usr/bin/env bash
# _test_helpers.sh — shared scaffolding for per-language `changed.test.sh`.
#
# Wave-1 left rust+ts changed.test.sh as 2 byte-identical copies of the same
# PASS/FAIL counter + assert_rc + run_with_cache scaffolding. Adding a 3rd
# copy in lang/proto/ tipped the abstraction threshold (dry-reviewer D1), so
# this helper hosts the shared mechanism. Per-lang test files collapse to
# source + assertions + report_results.
#
# Migration: lang/proto/ consumes this helper at birth. lang/rust/ and
# lang/ts/ carry a TODO note pointing here; their migration is a future
# devloop's concern (staged adoption — keeps Wave-2 #4 blast radius narrow).

set -euo pipefail
IFS=$'\n\t'

[[ -n "${__DEVLOOP_TEST_HELPERS_SH:-}" ]] && return 0
readonly __DEVLOOP_TEST_HELPERS_SH=1

# State (initialized at source).
PASS=0
FAIL=0
FAILURES=()

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

# Run the calling lang's changed.sh against an injected synthetic
# changed-files cache; return its exit code on stdout.
#
# Hermeticity: scrubs env (env -i) preserving only PATH, HOME, the synthetic
# DEVLOOP_TMP, and DEVLOOP_LAYER=locality. Caller's CWD must be the lang
# directory holding changed.sh (the source-line in each test file ensures
# this via `__here`).
#
# Args: $1=cache-content (newline-separated paths to inject)
# Outputs: stdout=changed.sh exit code (string)
# Returns: 0 always (the rc is on stdout)
run_with_cache() {
  local content="$1"
  local tmp; tmp=$(mktemp -d)
  printf '%s\n' "$content" > "${tmp}/changed-files.layer-locality"
  local rc=0
  env -i \
      PATH="$PATH" \
      HOME="$HOME" \
      DEVLOOP_TMP="$tmp" \
      DEVLOOP_LAYER=locality \
      "${__here}/changed.sh" >/dev/null 2>&1 || rc=$?
  rm -rf "$tmp"
  printf '%s\n' "$rc"
}

# Print final pass/fail summary. Exit 0 if all pass, 1 if any fail.
# Args: $1=label (typically the test file path for runbook context)
# Outputs: stdout=summary line + per-failure detail
report_results() {
  local label="$1"
  printf '\n%s: %d passed, %d failed\n' "$label" "$PASS" "$FAIL"
  if [[ $FAIL -gt 0 ]]; then
    local f
    for f in "${FAILURES[@]}"; do printf '  - %s\n' "$f"; done
    exit 1
  fi
  exit 0
}
