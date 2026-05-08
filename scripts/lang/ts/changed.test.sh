#!/usr/bin/env bash
# changed.test.sh — locality self-test for lang/ts/changed.sh.
#
# Mirrors the rust self-test layout — small representative path set fired
# against the predicate via injected DEVLOOP_TMP cache. Catches local-only
# regressions; full cross-language drift detection lives in
# _test_changed_predicates.sh (Wave 1 only asserts rust there; ts column TBD).
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

# Touched cases (exit 0). One assertion per predicate arm so locality
# self-test catches drift on any individual root-file or the prefix arm.
assert_rc "packages/foo/src/index.ts"  0 "$(run_with_cache "packages/foo/src/index.ts")"
assert_rc "package.json"               0 "$(run_with_cache "package.json")"
assert_rc "pnpm-lock.yaml"             0 "$(run_with_cache "pnpm-lock.yaml")"
assert_rc "pnpm-workspace.yaml"        0 "$(run_with_cache "pnpm-workspace.yaml")"
assert_rc "nx.json"                    0 "$(run_with_cache "nx.json")"
assert_rc "tsconfig.base.json"         0 "$(run_with_cache "tsconfig.base.json")"
assert_rc ".nvmrc"                     0 "$(run_with_cache ".nvmrc")"

# Untouched cases (exit 1).
assert_rc "docs/x.md"                  1 "$(run_with_cache "docs/x.md")"
assert_rc "crates/foo/src/lib.rs"      1 "$(run_with_cache "crates/foo/src/lib.rs")"
assert_rc "scripts/test.sh"            1 "$(run_with_cache "scripts/test.sh")"

printf '\nlang/ts/changed.test.sh: %d passed, %d failed\n' "$PASS" "$FAIL"
if [[ $FAIL -gt 0 ]]; then
  for f in "${FAILURES[@]}"; do printf '  - %s\n' "$f"; done
  exit 1
fi
exit 0
