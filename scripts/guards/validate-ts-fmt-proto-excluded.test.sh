#!/usr/bin/env bash
#
# Self-test for scripts/guards/simple/validate-ts-fmt-proto-excluded.sh (ADR-0037 §D7; @dry-reviewer
# standing assertion). The guard PASSES on the real tree, so its failure branches — the two preconditions
# that keep buf codegen out of prettier's set, plus the vacuity guard — are exercised here against synthetic
# config via the TSFMT_* seams, PLUS a real-tree positive control.
#
# NOT under guards/simple/: run-guards.sh discovers with `find … -name '*.sh'`, which matches `*.test.sh`
# (same reasoning as the other guard self-tests). Hermetic: mktemp + EXIT trap, no cluster/network/cargo.
set -euo pipefail
IFS=$'\n\t'
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
# shellcheck source=../lang/_test_helpers.sh
source "${REPO_ROOT}/scripts/lang/_test_helpers.sh"

GUARD="${REPO_ROOT}/scripts/guards/simple/validate-ts-fmt-proto-excluded.sh"
[[ -x "$GUARD" ]] || { printf '  - [precondition] guard missing/not executable at %s\n' "$GUARD"; printf '\n%s: 0 passed, 1 failed\n' "$0"; exit 1; }

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# Baseline synthetic config: proto line present + one format target with NO cwd (the guard must pass).
reset_fixtures() {
  IGNORE="${WORK}/.prettierignore"; PKGROOT="${WORK}/packages"
  printf '# comment\npackages/sdk-core/src/proto/\ndist/\n' > "$IGNORE"
  rm -rf "$PKGROOT"; mkdir -p "${PKGROOT}/sdk-core"
  printf '{"targets":{"format":{"options":{}}}}\n' > "${PKGROOT}/sdk-core/project.json"
}
run_guard() {
  OUT="$(TSFMT_PRETTIERIGNORE="$IGNORE" TSFMT_PKGROOT="$PKGROOT" bash "$GUARD" 2>&1)" && RC=0 || RC=$?
}

# --- P0: REAL-TREE positive control (no seams) — both preconditions hold today ---
OUT="$(bash "$GUARD" 2>&1)" && RC=0 || RC=$?
assert_exit "p0-real-tree-passes" 0 "$RC"

# --- P1: synthetic baseline passes (so a later FAIL is the mutation under test, not a broken baseline) ---
reset_fixtures; run_guard
assert_exit "p1-synthetic-baseline-passes" 0 "$RC"

# --- V-i: the proto-exclusion line is gone from .prettierignore -> fail ---
reset_fixtures; printf '# comment\ndist/\n' > "$IGNORE"; run_guard
assert_exit   "vi-missing-proto-line-fails" 1 "$RC"
assert_status "vi-missing-proto-line-token" "is not a line in" "$OUT"

# --- V-ii: a format target sets a cwd -> fail (cwd defeats the root, cwd-relative ignore) ---
reset_fixtures; printf '{"targets":{"format":{"options":{"cwd":"packages/sdk-core"}}}}\n' > "${PKGROOT}/sdk-core/project.json"; run_guard
assert_exit   "vii-format-cwd-fails" 1 "$RC"
assert_status "vii-format-cwd-token" "sets cwd=" "$OUT"

# --- V-iii (dry addition): a format command carrying --ignore-path -> fail (it redirects ignore resolution
#     away from the root .prettierignore — the one path around clauses i+ii even with cwd correct). ---
reset_fixtures; printf '{"targets":{"format":{"options":{"command":"prettier \\"packages/sdk-core/src/**/*.ts\\" --ignore-path .other-ignore"}}}}\n' > "${PKGROOT}/sdk-core/project.json"; run_guard
assert_exit   "viii-ignore-path-fails" 1 "$RC"
assert_status "viii-ignore-path-token" "carries --ignore-path" "$OUT"

# --- V-vacuity: no format target anywhere -> fail (a guard checking zero targets asserts nothing) ---
reset_fixtures; printf '{"targets":{"lint":{}}}\n' > "${PKGROOT}/sdk-core/project.json"; run_guard
assert_exit   "vvac-no-format-target-fails" 1 "$RC"
assert_status "vvac-no-format-target-token" "no 'format' target found" "$OUT"

# --- V-missing-file: .prettierignore absent -> loud fail (not a silent skip) ---
reset_fixtures; rm -f "$IGNORE"; run_guard
assert_exit   "vmiss-no-prettierignore-fails" 1 "$RC"
assert_status "vmiss-no-prettierignore-token" "not found" "$OUT"

report_results "scripts/guards/validate-ts-fmt-proto-excluded.test.sh"
