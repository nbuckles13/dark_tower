#!/usr/bin/env bash
# changed.test.sh — locality self-test for lang/rust/changed.sh.
#
# Fires the rust predicate against a small set of representative paths to
# catch local-only regressions without waiting for the meta-test to run.
# The full cross-language drift detection lives in _test_changed_predicates.sh.
set -euo pipefail
IFS=$'\n\t'

__here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

PASS=0
FAIL=0
FAILURES=()

assert_rc() {
  local label="$1" expected="$2" actual="$3"
  if [[ "$actual" == "$expected" ]]; then
    PASS=$((PASS + 1))
  else
    FAIL=$((FAIL + 1))
    FAILURES+=("[${label}] expected_rc=${expected} actual_rc=${actual}")
  fi
}

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

# Touched cases (exit 0).
assert_rc "crates/foo/src/lib.rs"  0 "$(run_with_cache "crates/foo/src/lib.rs")"
assert_rc "Cargo.toml"             0 "$(run_with_cache "Cargo.toml")"
assert_rc "Cargo.lock"             0 "$(run_with_cache "Cargo.lock")"

# Untouched cases (exit 1).
assert_rc "docs/x.md"              1 "$(run_with_cache "docs/x.md")"
assert_rc "scripts/test.sh"        1 "$(run_with_cache "scripts/test.sh")"

printf '\nlang/rust/changed.test.sh: %d passed, %d failed\n' "$PASS" "$FAIL"
if [[ $FAIL -gt 0 ]]; then
  for f in "${FAILURES[@]}"; do printf '  - %s\n' "$f"; done
  exit 1
fi
exit 0
