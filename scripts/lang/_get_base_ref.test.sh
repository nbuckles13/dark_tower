#!/usr/bin/env bash
# _get_base_ref.test.sh — 8-case matrix self-test (test §1, §A).
#
# Each test runs in its own mktemp tempdir with a fresh git init.
# Hermeticity: no real `git fetch` against actual `origin`; uses local file:// remotes
# (or no remote in unreachable cases). No reads from /work workspace state.
set -euo pipefail
IFS=$'\n\t'

__here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
GET_BASE_REF="${__here}/_get_base_ref.sh"

PASS=0
FAIL=0
FAILURES=()

# -----------------------------------------------------------------------------
# Helpers
# -----------------------------------------------------------------------------

# Args: $1=label  $2=stderr-content
# Returns: 0 if line shape valid, 1 if not
assert_base_ref_line_well_formed() {
  local label="$1"
  local stderr="$2"
  local missing=()

  for tok in BASE_REF= BASE_SOURCE= DIFF_MODE= FILES_CHANGED=; do
    if ! grep -q "$tok" <<<"$stderr"; then
      missing+=("$tok")
    fi
  done
  if [[ ${#missing[@]} -gt 0 ]]; then
    FAIL=$((FAIL + 1))
    FAILURES+=("[${label}] missing tokens: ${missing[*]}")
    return 1
  fi
  PASS=$((PASS + 1))
  return 0
}

# Args: $1=label  $2=expected-source-token  $3=stderr
assert_base_source() {
  local label="$1"
  local expected="$2"
  local stderr="$3"
  if grep -q "BASE_SOURCE=${expected}" <<<"$stderr"; then
    PASS=$((PASS + 1))
  else
    FAIL=$((FAIL + 1))
    FAILURES+=("[${label}] expected BASE_SOURCE=${expected}, stderr was: ${stderr}")
  fi
}

# Set up an isolated repo with a local origin remote.
# Args: $1=tempdir
# Side effect: cd's into tempdir, configures git, creates origin
init_repo_with_origin() {
  local tmp="$1"
  cd "$tmp"
  # Origin is a bare repo on disk (file:// remote) — never the real origin.
  mkdir origin.git
  ( cd origin.git && git init --bare --quiet )
  git init --quiet
  git config user.email "test@example.com"
  git config user.name "Test"
  git config commit.gpgsign false
  git remote add origin "file://${tmp}/origin.git"
  echo "first" > README.md
  git add README.md
  git commit -q -m "initial"
  git push --quiet origin HEAD:main
  git fetch --quiet origin
}

# Run _get_base_ref.sh in a clean env (only what's explicitly passed survives).
# Args: $@=KEY=VALUE pairs to export
# Outputs: env-cleaned invocation; stdout=sha, stderr=BASE_REF= line
run_isolated() {
  env -i \
      PATH="$PATH" \
      HOME="$HOME" \
      DEVLOOP_TMP="$(pwd)/.tmp-devloop" \
      "$@" \
      "$GET_BASE_REF"
}

# Cleanup helper — removes a tempdir registered for cleanup.
__TEMPDIRS=()
mktemp_register() {
  local d
  d=$(mktemp -d)
  __TEMPDIRS+=("$d")
  printf '%s\n' "$d"
}
trap '
  for d in "${__TEMPDIRS[@]}"; do rm -rf "$d"; done
' EXIT

# -----------------------------------------------------------------------------
# Cases
# -----------------------------------------------------------------------------

# Helper: cd into tempdir without using subshell (so PASS/FAIL counts survive).
__push_cwd() { __SAVED_CWD="$(pwd)"; cd "$1"; }
__pop_cwd()  { cd "$__SAVED_CWD"; }

# 1. local-clean: branch off main, one commit ahead.
test_local_clean() {
  local tmp; tmp=$(mktemp_register)
  __push_cwd "$tmp"
  init_repo_with_origin "$tmp"
  git checkout -q -b feat
  echo "x" > a.txt
  git add a.txt
  git commit -q -m "feat: a"
  out=$(run_isolated 2>&1 >/dev/null)
  assert_base_ref_line_well_formed "local-clean" "$out"
  assert_base_source "local-clean" "local-mergebase" "$out"
  __pop_cwd
}

# 2. local-dirty: same as local-clean + uncommitted edit.
test_local_dirty() {
  local tmp; tmp=$(mktemp_register)
  __push_cwd "$tmp"
  init_repo_with_origin "$tmp"
  git checkout -q -b feat
  echo "x" > a.txt
  git add a.txt
  git commit -q -m "feat: a"
  echo "dirty" >> README.md
  out=$(run_isolated 2>&1 >/dev/null)
  assert_base_ref_line_well_formed "local-dirty" "$out"
  assert_base_source "local-dirty" "local-mergebase" "$out"
  __pop_cwd
}

# 3. local-with-untracked: untracked file present.
test_local_with_untracked() {
  local tmp; tmp=$(mktemp_register)
  __push_cwd "$tmp"
  init_repo_with_origin "$tmp"
  git checkout -q -b feat
  echo "x" > a.txt
  git add a.txt
  git commit -q -m "feat: a"
  echo "untracked" > NEW.txt
  out=$(run_isolated 2>&1 >/dev/null)
  assert_base_ref_line_well_formed "local-with-untracked" "$out"
  files_changed=$(grep -oE 'FILES_CHANGED=[0-9]+' <<<"$out" | cut -d= -f2)
  if [[ -z "$files_changed" || "$files_changed" -lt 1 ]]; then
    FAIL=$((FAIL + 1))
    FAILURES+=("[local-with-untracked] FILES_CHANGED=${files_changed:-empty}, expected >=1")
  else
    PASS=$((PASS + 1))
  fi
  __pop_cwd
}

# 4. local-no-mergebase: no origin/main reachable; fall back to HEAD.
test_local_no_mergebase() {
  local tmp; tmp=$(mktemp_register)
  __push_cwd "$tmp"
  git init --quiet
  git config user.email "test@example.com"
  git config user.name "Test"
  git config commit.gpgsign false
  echo "x" > a.txt
  git add a.txt
  git commit -q -m "only commit"
  out=$(run_isolated 2>&1 >/dev/null)
  assert_base_ref_line_well_formed "local-no-mergebase" "$out"
  assert_base_source "local-no-mergebase" "local-no-mergebase" "$out"
  __pop_cwd
}

# 5. ci-pr: PR event, local file:// remote stands in for origin.
#    Post task #42: BASE_REF resolves to merge-base(origin/main, HEAD), DIFF_MODE=two-dot.
test_ci_pr() {
  local tmp; tmp=$(mktemp_register)
  __push_cwd "$tmp"
  init_repo_with_origin "$tmp"
  git checkout -q -b feat
  echo "x" > a.txt
  git add a.txt
  git commit -q -m "feat: a"
  git push --quiet origin feat
  local expected_sha
  expected_sha=$(git merge-base origin/main HEAD)
  out=$(run_isolated \
          GITHUB_ACTIONS=1 \
          GITHUB_EVENT_NAME=pull_request \
          GITHUB_BASE_REF=main \
          2>&1 >/dev/null)
  assert_base_ref_line_well_formed "ci-pr" "$out"
  assert_base_source "ci-pr" "ci-pr" "$out"
  if grep -q "DIFF_MODE=two-dot" <<<"$out"; then
    PASS=$((PASS + 1))
  else
    FAIL=$((FAIL + 1))
    FAILURES+=("[ci-pr] expected DIFF_MODE=two-dot, stderr: $out")
  fi
  if grep -q "BASE_REF=${expected_sha} " <<<"$out"; then
    PASS=$((PASS + 1))
  else
    FAIL=$((FAIL + 1))
    FAILURES+=("[ci-pr] expected BASE_REF=${expected_sha} (merge-base), stderr: $out")
  fi
  __pop_cwd
}

# 5b. ci-pr-merge-base-vs-tip: assert that BASE_REF is the MERGE-BASE, not the
#     base-branch tip. Constructs M2 (unrelated progress on origin/main after
#     PR branch was cut) and asserts BASE_REF != origin/main tip SHA.
test_ci_pr_merge_base_vs_tip() {
  local tmp; tmp=$(mktemp_register)
  __push_cwd "$tmp"
  init_repo_with_origin "$tmp"
  # init_repo_with_origin leaves HEAD on the initial commit (the merge-base anchor).
  local merge_base_sha
  merge_base_sha=$(git rev-parse HEAD)
  # Create feat branch with one PR commit (P1).
  git checkout -q -b feat
  echo "p1" > p1.txt
  git add p1.txt
  git commit -q -m "feat: p1"
  # Advance origin/main with an unrelated commit (M2) via a worktree on main.
  # Use a detached branch from the merge-base to simulate main advancing.
  git checkout -q -b main_advance "$merge_base_sha"
  echo "m2" > m2.txt
  git add m2.txt
  git commit -q -m "main: m2 unrelated"
  git push --quiet origin "main_advance:main" --force
  local main_tip_sha
  main_tip_sha=$(git rev-parse HEAD)
  # Refresh origin/main and switch back to feat (HEAD = PR tip).
  git fetch --quiet origin
  git checkout -q feat
  out=$(run_isolated \
          GITHUB_ACTIONS=1 \
          GITHUB_EVENT_NAME=pull_request \
          GITHUB_BASE_REF=main \
          2>&1 >/dev/null)
  if grep -q "BASE_REF=${merge_base_sha} " <<<"$out"; then
    PASS=$((PASS + 1))
  else
    FAIL=$((FAIL + 1))
    FAILURES+=("[ci-pr-merge-base-vs-tip] expected BASE_REF=${merge_base_sha} (merge-base), stderr: $out")
  fi
  if grep -q "BASE_REF=${main_tip_sha} " <<<"$out"; then
    FAIL=$((FAIL + 1))
    FAILURES+=("[ci-pr-merge-base-vs-tip] BASE_REF resolved to main tip ${main_tip_sha}; should be merge-base")
  else
    PASS=$((PASS + 1))
  fi
  __pop_cwd
}

# 5c. local-mergebase-regression: local-mode BASE_REF equals merge-base. Pre-#42
#     behavior preserved; this assertion guards against future regressions.
test_local_clean_merge_base_regression() {
  local tmp; tmp=$(mktemp_register)
  __push_cwd "$tmp"
  init_repo_with_origin "$tmp"
  git checkout -q -b feat
  echo "x" > a.txt
  git add a.txt
  git commit -q -m "feat: a"
  local expected_sha
  expected_sha=$(git merge-base origin/main HEAD)
  out=$(run_isolated 2>&1 >/dev/null)
  if grep -q "BASE_REF=${expected_sha} " <<<"$out"; then
    PASS=$((PASS + 1))
  else
    FAIL=$((FAIL + 1))
    FAILURES+=("[local-mergebase-regression] expected BASE_REF=${expected_sha}, stderr: $out")
  fi
  __pop_cwd
}

# 6. ci-push: push event with HEAD~1 reachable.
test_ci_push() {
  local tmp; tmp=$(mktemp_register)
  __push_cwd "$tmp"
  init_repo_with_origin "$tmp"
  echo "x" > a.txt
  git add a.txt
  git commit -q -m "feat: a"
  out=$(run_isolated \
          GITHUB_ACTIONS=1 \
          GITHUB_EVENT_NAME=push \
          2>&1 >/dev/null)
  assert_base_ref_line_well_formed "ci-push" "$out"
  assert_base_source "ci-push" "ci-push-main" "$out"
  __pop_cwd
}

# 7. ci-push-first-commit: push event but only one commit (no HEAD~1).
test_ci_push_first_commit() {
  local tmp; tmp=$(mktemp_register)
  __push_cwd "$tmp"
  git init --quiet
  git config user.email "test@example.com"
  git config user.name "Test"
  git config commit.gpgsign false
  echo "x" > a.txt
  git add a.txt
  git commit -q -m "only commit"
  out=$(run_isolated \
          GITHUB_ACTIONS=1 \
          GITHUB_EVENT_NAME=push \
          2>&1 >/dev/null)
  assert_base_ref_line_well_formed "ci-push-first-commit" "$out"
  assert_base_source "ci-push-first-commit" "ci-push-first-commit" "$out"
  __pop_cwd
}

# 8. ci-pr-base-ref-unreachable: PR event with base ref that doesn't exist on origin.
#    Per security §5: stderr must NOT leak GITHUB_TOKEN if set.
test_ci_pr_base_ref_unreachable() {
  local tmp; tmp=$(mktemp_register)
  __push_cwd "$tmp"
  init_repo_with_origin "$tmp"
  git checkout -q -b feat
  echo "x" > a.txt
  git add a.txt
  git commit -q -m "feat: a"
  rc=0
  out=$(run_isolated \
          GITHUB_ACTIONS=1 \
          GITHUB_EVENT_NAME=pull_request \
          GITHUB_BASE_REF=does-not-exist-anywhere \
          GITHUB_TOKEN=test-token-DO-NOT-LEAK \
          2>&1 >/dev/null) || rc=$?
  if [[ $rc -ne 0 ]]; then
    PASS=$((PASS + 1))
  else
    FAIL=$((FAIL + 1))
    FAILURES+=("[ci-pr-base-ref-unreachable] expected non-zero exit, got 0")
  fi
  if grep -q "GITHUB_BASE_REF" <<<"$out"; then
    PASS=$((PASS + 1))
  else
    FAIL=$((FAIL + 1))
    FAILURES+=("[ci-pr-base-ref-unreachable] stderr should name GITHUB_BASE_REF; got: $out")
  fi
  if grep -q "test-token-DO-NOT-LEAK" <<<"$out"; then
    FAIL=$((FAIL + 1))
    FAILURES+=("[ci-pr-base-ref-unreachable] GITHUB_TOKEN value LEAKED to stderr")
  else
    PASS=$((PASS + 1))
  fi
  __pop_cwd
}

# -----------------------------------------------------------------------------
# Run all
# -----------------------------------------------------------------------------

test_local_clean
test_local_dirty
test_local_with_untracked
test_local_no_mergebase
test_ci_pr
test_ci_pr_merge_base_vs_tip
test_local_clean_merge_base_regression
test_ci_push
test_ci_push_first_commit
test_ci_pr_base_ref_unreachable

printf '\n_get_base_ref.test.sh: %d passed, %d failed\n' "$PASS" "$FAIL"
if [[ $FAIL -gt 0 ]]; then
  printf 'Failures:\n'
  for f in "${FAILURES[@]}"; do
    printf '  - %s\n' "$f"
  done
  exit 1
fi
exit 0
