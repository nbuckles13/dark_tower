#!/usr/bin/env bash
# _changed_helpers.test.sh — direct hermetic self-tests for the diff predicates in
# _changed_helpers.sh, focused on diff_touches_glob (added 2026-08-05 to narrow the
# Layer 6 audit dep-change gate to TRUE dep manifests). Before this, the predicates
# were exercised only INDIRECTLY (via _test_changed_predicates.sh + _audit_gate.test.sh);
# diff_touches_glob is security-load-bearing — a wrong match/skip = wrong audit run/skip —
# so it gets a direct predicate-level test co-located with its module.
#
# No network / no git: cases drive off an injected changed-files cache under env -i,
# sourcing _changed_helpers.sh (mirrors _audit_gate.test.sh::pred_rc). Wired into
# scripts/layer3.sh (no *.test.sh auto-runner — an unrun test is untested).
set -euo pipefail
IFS=$'\n\t'

__here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=_test_helpers.sh
source "${__here}/_test_helpers.sh"
set +e  # the predicate returns non-zero (1=no-match) by design; assert, don't abort

# glob_rc <glob> <cache-content> -> echoes diff_touches_glob exit code (0=match, 1=no-match).
# Hermetic: synthetic DEVLOOP_TMP cache (which suppresses the _get_base_ref.sh fallback in
# __ensure_cache since the cache file already exists) + env -i, sourcing _changed_helpers.sh.
glob_rc() {
  local glob="$1" content="$2"
  local tmp; tmp="$(mktemp -d)"
  printf '%s\n' "$content" > "${tmp}/changed-files.layer-locality"
  local rc=0
  env -i PATH="$PATH" HOME="$HOME" DEVLOOP_TMP="$tmp" DEVLOOP_LAYER=locality \
      bash -c "source '${__here}/_changed_helpers.sh'; diff_touches_glob '$glob'" >/dev/null 2>&1 || rc=$?
  rm -rf "$tmp"
  printf '%s\n' "$rc"
}

# -----------------------------------------------------------------------------
# diff_touches_glob — crate manifests (crates/*/Cargo.toml)
# -----------------------------------------------------------------------------
# match: a one-level crate manifest.
assert_rc "glob: crate Cargo.toml matches"                 0 "$(glob_rc 'crates/*/Cargo.toml' 'crates/ac-service/Cargo.toml')"

# *-CROSSES-/ : a two-level (nested) manifest MATCHES. WHY this asserts MATCH (not skip):
# bash [[ == ]] pattern-matching's `*` crosses `/` (UNLIKE filename globbing), and this fuzz
# manifest is workspace-EXCLUDED (root Cargo.toml `exclude`) — matching it = the fail-SAFE
# over-trigger direction (run the cheap scan against the unchanged root Cargo.lock). Do NOT
# "fix" this to no-match under a filename-glob mental model: excluding it would be fail-OPEN
# and needs attribution logic the task ruled out of scope.
assert_rc "glob: * crosses / (nested/excluded fuzz manifest matches)" 0 "$(glob_rc 'crates/*/Cargo.toml' 'crates/ac-service/fuzz/Cargo.toml')"

# negative — prefix shares crates/ but suffix differs / is not a manifest:
assert_rc "glob: crate SOURCE shares prefix, no Cargo.toml suffix -> no match" 1 "$(glob_rc 'crates/*/Cargo.toml' 'crates/ac-service/src/lib.rs')"
assert_rc "glob: suffix-differing sibling (.bak) -> no match"        1 "$(glob_rc 'crates/*/Cargo.toml' 'crates/ac-service/Cargo.toml.bak')"
# anchoring hardening: full-line anchored, so a direct child (no subdir) and a cross-tree
# path sharing the basename do NOT match.
assert_rc "glob: direct child crates/Cargo.toml (no subdir) -> no match" 1 "$(glob_rc 'crates/*/Cargo.toml' 'crates/Cargo.toml')"
assert_rc "glob: cross-tree packages/crates/x/Cargo.toml -> no match"    1 "$(glob_rc 'crates/*/Cargo.toml' 'packages/crates/x/Cargo.toml')"
# root manifest is NOT matched by the glob (the root-files arm handles it) — proves the
# glob does not accidentally swallow the root Cargo.toml.
assert_rc "glob: root Cargo.toml does NOT match crates/*/Cargo.toml"     1 "$(glob_rc 'crates/*/Cargo.toml' 'Cargo.toml')"
# unrelated path + empty cache -> no match.
assert_rc "glob: unrelated docs path -> no match"          1 "$(glob_rc 'crates/*/Cargo.toml' 'docs/x.md')"
assert_rc "glob: empty cache -> no match"                  1 "$(glob_rc 'crates/*/Cargo.toml' '')"
# match when ONE of several changed files matches (multi-line cache).
assert_rc "glob: matches among several changed files"      0 "$(glob_rc 'crates/*/Cargo.toml' $'docs/x.md\ncrates/mc-service/Cargo.toml\nsrc/y.rs')"

# -----------------------------------------------------------------------------
# diff_touches_glob — ts package manifests (packages/*/package.json)
# -----------------------------------------------------------------------------
assert_rc "glob: package.json matches"                     0 "$(glob_rc 'packages/*/package.json' 'packages/sdk-core/package.json')"
assert_rc "glob: * crosses / (nested package.json matches)" 0 "$(glob_rc 'packages/*/package.json' 'packages/group/sub/package.json')"
assert_rc "glob: package SOURCE shares prefix -> no match"  1 "$(glob_rc 'packages/*/package.json' 'packages/sdk-core/src/index.ts')"
assert_rc "glob: package-lock.json suffix differs -> no match" 1 "$(glob_rc 'packages/*/package.json' 'packages/sdk-core/package-lock.json')"
assert_rc "glob: root package.json does NOT match packages/*/package.json" 1 "$(glob_rc 'packages/*/package.json' 'package.json')"


# ---- A missing helper is a FAILURE, not a skipped assertion ----------------
# Runs a throwaway harness in a CHILD so its report_results/exit cannot end this
# suite. The child calls one real assertion and one helper that does not exist;
# it must exit 1 and name the missing helper. Before the file-backed
# command_not_found_handle this reported "1 passed, 0 failed" and exited 0.
__missing_out=$(bash -c 'source "$1"; set +e; assert_rc ok 0 0; no_such_helper_abc x 1 1; report_results child' _ "${__here:-$(dirname "${BASH_SOURCE[0]}")}/_test_helpers.sh" 2>/dev/null)
__missing_rc=$?
assert_rc "harness: missing helper makes the suite exit 1" 1 "$__missing_rc"
if [[ "$__missing_out" == *"missing command/helper 'no_such_helper_abc'"* ]]; then PASS=$((PASS + 1)); else
  FAIL=$((FAIL + 1)); FAILURES+=("[harness-missing-named] summary did not name the missing helper: ${__missing_out@Q}"); fi

report_results "scripts/lang/_changed_helpers.test.sh"
