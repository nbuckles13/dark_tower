#!/usr/bin/env bash
# _test_changed_predicates.sh — meta-test for per-language changed.sh predicates.
#
# Each language's changed.sh is the sole authority for its language's footprint.
# This meta-test asserts each predicate fires correctly against a hand-curated
# fixture set, so drift between predicates (one language stricter than another)
# is detectable in CI.
#
# Hermetic per test §A: tests run by injecting a synthetic changed-files cache
# into a tempdir-rooted DEVLOOP_TMP, never reading the real workspace state.
#
# Failure mode (test §C): when a predicate disagrees with its fixture row,
# prints the row, expected vs actual exit code, and pointer to the relevant
# lang/<X>/changed.sh.

set -euo pipefail
IFS=$'\n\t'

__here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=_common.sh
source "${__here}/_common.sh"

PASS=0
FAIL=0
FAILURES=()

# Inject a synthetic changed-files cache, then invoke the lang's changed.sh
# in a clean env. Returns its exit code.
# Args: $1=lang  $2=cache-content (newline-separated paths)
# Returns: changed.sh exit code
__invoke_changed_sh_with_cache() {
  local lang="$1"
  local cache_content="$2"
  local tmp; tmp=$(mktemp -d)
  local cache="${tmp}/changed-files.layer-meta-test"
  printf '%s\n' "$cache_content" > "$cache"

  local rc=0
  env -i \
      PATH="$PATH" \
      HOME="$HOME" \
      DEVLOOP_TMP="$tmp" \
      DEVLOOP_LAYER=meta-test \
      "${__here}/${lang}/changed.sh" >/dev/null 2>&1 || rc=$?
  rm -rf "$tmp"
  printf '%s\n' "$rc"
}

# Args: $1=lang  $2=fixture-row (path)  $3=expected-exit-code (0=touched, 1=untouched)  $4=rationale
__assert_predicate() {
  local lang="$1" path="$2" expected="$3" rationale="$4"
  local actual
  actual=$(__invoke_changed_sh_with_cache "$lang" "$path")
  if [[ "$actual" == "$expected" ]]; then
    PASS=$((PASS + 1))
  else
    FAIL=$((FAIL + 1))
    FAILURES+=("[${lang}] path=${path} expected_rc=${expected} actual_rc=${actual}
  rationale: ${rationale}
  see: scripts/lang/${lang}/changed.sh")
  fi
}

# -----------------------------------------------------------------------------
# Fixtures (test §2)
#
# For Wave 1, only the `rust` column is asserted. The fixture file is structured
# so Wave 2 can add ts/proto columns without rewriting it.
# -----------------------------------------------------------------------------

# Wave 1 — Rust predicate fixtures.

# happy-path Rust src
__assert_predicate rust "crates/foo/src/lib.rs" 0 "happy-path Rust src under crates/"

# root cargo manifest
__assert_predicate rust "Cargo.toml" 0 "root cargo manifest"

# root files arm — Cargo.lock
__assert_predicate rust "Cargo.lock" 0 "exercises root-files arm of predicate (3 root files)"

# root files arm — rust-toolchain.toml
__assert_predicate rust "rust-toolchain.toml" 0 "exercises root-files arm — proves helper iterates all listed files"

# manifest under crates/ — covered via crates/ prefix arm
__assert_predicate rust "crates/common/Cargo.toml" 0 "manifest under crates/ — covered via crates/ prefix arm"

# intentional over-classification per ADR-0033 §3
__assert_predicate rust "crates/foo/README.md" 0 "intentional over-classification per ADR-0033 §3 — refine later if it bothers anyone"

# gap flagged for discussion: .cargo/ outside crates/ and not in root-files list
__assert_predicate rust ".cargo/config.toml" 1 "gap flagged for discussion: not covered by current predicate; documented untouched so the gap is visible"

# TS happy path (rust untouched)
__assert_predicate rust "packages/foo/src/index.ts" 1 "TS happy path → rust untouched"

# proto happy path (rust untouched)
__assert_predicate rust "proto/foo.proto" 1 "proto happy path → rust untouched"

# docs outside any lang
__assert_predicate rust "docs/x.md" 1 "docs outside any lang"

# infra outside any lang
__assert_predicate rust "infra/y.yaml" 1 "infra outside any lang"

# negative case: validation pipeline itself is not in any lang's footprint
__assert_predicate rust "scripts/test.sh" 1 "negative case: classifier doesn't classify our own infra as Rust"
__assert_predicate rust "scripts/lang/rust/test.sh" 1 "negative case: Rust-the-language footprint vs Rust-related-files distinction"

# -----------------------------------------------------------------------------
# Summary
# -----------------------------------------------------------------------------

printf '\n_test_changed_predicates.sh: %d passed, %d failed\n' "$PASS" "$FAIL"
if [[ $FAIL -gt 0 ]]; then
  printf 'Failures:\n'
  for f in "${FAILURES[@]}"; do
    printf '%s\n' "$f"
  done
  emit_status FAIL "predicate-meta-test-failed"
  exit 1
fi
emit_status OK "predicate-meta-test-passed"
