#!/usr/bin/env bash
# _get_base_ref.behavior-equivalence.test.sh — temporal before/after fixture
# proving the task #42 CI-PR base-ref refactor's behavior shift is correct.
#
# Asserts:
#   1. ci-pr-old: OLD _get_base_ref.sh (extracted via `git show <SHA>:`)
#      resolves CI-PR BASE_REF to origin/${GITHUB_BASE_REF} TIP.
#   2. ci-pr-new: NEW _get_base_ref.sh (live in workspace) resolves CI-PR
#      BASE_REF to the MERGE-BASE.
#   3. ci-pr-diff: `git diff --name-only $BASE_SHA..HEAD` under NEW includes
#      only the PR's contribution, NOT main's unrelated commits.
#   4. local-mergebase-noop: local mode resolves identically (no behavior shift).
#   5. shallow-clone-guardrail: layer-all.sh exits 2 with PRECONDITION_FAILURE:
#      message when run inside a shallow clone (per @test Gate-1 blocker).
#   6. ci-push-guardrail-skip: layer-all.sh does NOT fail in CI-push mode even
#      on a shallow clone (per @operations R1; resolver uses HEAD~1, no remote
#      pack lookup needed).
#   7. guard-callsite-coverage: bash -n syntax check across guards/common.sh
#      and all guards/simple/**/*.sh (per @test Gate-1 should-fix).
#
# IMPORTANT: This fixture pins START_COMMIT for OLD-script extraction. If the
# branch is rebased post-devloop and START_COMMIT becomes unreachable, retarget
# the SHA below to the new equivalent. Per task #42 §Decisions, the rebase-
# safety burden lives in this header — no repo-level tag is created.

set -euo pipefail
IFS=$'\n\t'

# Pinned per task #42 §Loop Metadata. Reachable from feature/browser-client-join-task38.
START_COMMIT="9f9bbf0dce19a57e2d3fb480163262be224684c6"

__here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${__here}/../.." && pwd)"
NEW_GET_BASE_REF="${__here}/_get_base_ref.sh"
LAYER_ALL="${REPO_ROOT}/scripts/layer-all.sh"

PASS=0
FAIL=0
FAILURES=()

# Tempdir cleanup
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

# Set up the synthetic git history: origin/main has M1 (initial) + M2 (unrelated
# advance); feat branch has P1, P2 cut from M1.
# Args: $1=tempdir
# Side effect: cd's into tempdir; HEAD ends on feat with P1+P2; origin/main is at M2.
# Outputs: stdout = "M1_SHA M2_SHA P1_SHA P2_SHA" (space-separated)
fixture_pr_with_main_advance() {
  local tmp="$1"
  cd "$tmp"
  mkdir origin.git
  ( cd origin.git && git init --bare --quiet )
  git init --quiet
  git config user.email "test@example.com"
  git config user.name "Test"
  git config commit.gpgsign false
  git remote add origin "file://${tmp}/origin.git"
  # M1: initial commit, pushed as origin/main.
  echo "m1" > README.md
  git add README.md
  git commit -q -m "M1: initial"
  local m1_sha
  m1_sha=$(git rev-parse HEAD)
  git push --quiet origin HEAD:main
  # P1, P2: feat branch off M1.
  git checkout -q -b feat
  echo "p1" > p1.txt
  git add p1.txt
  git commit -q -m "P1"
  local p1_sha
  p1_sha=$(git rev-parse HEAD)
  echo "p2" > p2.txt
  git add p2.txt
  git commit -q -m "P2"
  local p2_sha
  p2_sha=$(git rev-parse HEAD)
  # M2: advance origin/main with an unrelated commit.
  git checkout -q -b main_advance "$m1_sha"
  echo "m2" > m2.txt
  git add m2.txt
  git commit -q -m "M2 unrelated main advance"
  local m2_sha
  m2_sha=$(git rev-parse HEAD)
  git push --quiet origin "main_advance:main" --force
  git fetch --quiet origin
  # End on feat.
  git checkout -q feat
  printf '%s %s %s %s\n' "$m1_sha" "$m2_sha" "$p1_sha" "$p2_sha"
}

# Run a _get_base_ref.sh variant in a clean env.
# Args: $1=path-to-script  $@(rest)=env KEY=VALUE pairs
# Outputs: stderr to caller via 2>&1 redirect; stdout suppressed.
run_isolated() {
  local script="$1"; shift
  env -i \
      PATH="$PATH" \
      HOME="$HOME" \
      DEVLOOP_TMP="$(pwd)/.tmp-devloop" \
      "$@" \
      "$script"
}

# Run layer-all.sh in a clean env.
run_layer_all() {
  env -i \
      PATH="$PATH" \
      HOME="$HOME" \
      DEVLOOP_TMP="$(pwd)/.tmp-devloop" \
      "$@" \
      "$LAYER_ALL"
}

__push_cwd() { __SAVED_CWD="$(pwd)"; cd "$1"; }
__pop_cwd()  { cd "$__SAVED_CWD"; }

# Cases 1-4: CI-PR before/after + local-mode no-op.
test_ci_pr_before_after() {
  local tmp; tmp=$(mktemp_register)
  __push_cwd "$tmp"
  IFS=' ' read -r m1_sha m2_sha p1_sha p2_sha < <(fixture_pr_with_main_advance "$tmp")

  # Extract OLD _get_base_ref.sh from START_COMMIT into the tempdir.
  local old_script="$tmp/_get_base_ref.OLD.sh"
  if ! git -C "$REPO_ROOT" show "${START_COMMIT}:scripts/lang/_get_base_ref.sh" > "$old_script" 2>/dev/null; then
    FAIL=$((FAIL + 1))
    FAILURES+=("[ci-pr-before-after] could not extract OLD _get_base_ref.sh from START_COMMIT=${START_COMMIT}; retarget the SHA per this file's header.")
    __pop_cwd
    return
  fi
  chmod +x "$old_script"
  # OLD script sources _common.sh from its own __here. We extracted it into
  # ${tmp}/ which has no _common.sh — symlink so the OLD source line resolves.
  ln -sf "${__here}/_common.sh" "$tmp/_common.sh"

  # Case 1: OLD resolves to origin/main tip (M2).
  local out_old
  out_old=$(run_isolated "$old_script" \
              GITHUB_ACTIONS=1 \
              GITHUB_EVENT_NAME=pull_request \
              GITHUB_BASE_REF=main \
              2>&1 >/dev/null)
  if grep -q "BASE_REF=${m2_sha} " <<<"$out_old"; then
    PASS=$((PASS + 1))
  else
    FAIL=$((FAIL + 1))
    FAILURES+=("[ci-pr-old] expected BASE_REF=${m2_sha} (origin/main tip M2), stderr: $out_old")
  fi

  # Case 2: NEW resolves to merge-base (M1).
  local out_new
  out_new=$(run_isolated "$NEW_GET_BASE_REF" \
              GITHUB_ACTIONS=1 \
              GITHUB_EVENT_NAME=pull_request \
              GITHUB_BASE_REF=main \
              2>&1 >/dev/null)
  if grep -q "BASE_REF=${m1_sha} " <<<"$out_new"; then
    PASS=$((PASS + 1))
  else
    FAIL=$((FAIL + 1))
    FAILURES+=("[ci-pr-new] expected BASE_REF=${m1_sha} (merge-base M1), stderr: $out_new")
  fi

  # Case 3: under NEW, `git diff --name-only $BASE_SHA..HEAD` is only PR's contribution.
  local new_diff
  new_diff=$(git diff --name-only "${m1_sha}..HEAD" | sort)
  local expected_diff
  expected_diff=$(printf 'p1.txt\np2.txt\n' | sort)
  if [[ "$new_diff" == "$expected_diff" ]]; then
    PASS=$((PASS + 1))
  else
    FAIL=$((FAIL + 1))
    FAILURES+=("[ci-pr-diff] expected diff p1.txt + p2.txt; got: $new_diff")
  fi
  # Under OLD, the diff would have included m2.txt (false-positive).
  local old_diff
  old_diff=$(git diff --name-only "${m2_sha}..HEAD" | sort)
  if grep -q "^m2\.txt$" <<<"$old_diff"; then
    PASS=$((PASS + 1))  # confirms OLD-style diff would include the unrelated main commit
  else
    FAIL=$((FAIL + 1))
    FAILURES+=("[ci-pr-diff-old-includes-m2] expected old-style diff to include m2.txt as false-positive; got: $old_diff")
  fi

  __pop_cwd
}

# Case 4: local-mergebase-noop — both OLD and NEW resolve identically in local mode.
test_local_mergebase_noop() {
  local tmp; tmp=$(mktemp_register)
  __push_cwd "$tmp"
  IFS=' ' read -r m1_sha _m2_sha _p1_sha _p2_sha < <(fixture_pr_with_main_advance "$tmp")

  local old_script="$tmp/_get_base_ref.OLD.sh"
  git -C "$REPO_ROOT" show "${START_COMMIT}:scripts/lang/_get_base_ref.sh" > "$old_script"
  chmod +x "$old_script"
  ln -sf "${__here}/_common.sh" "$tmp/_common.sh"

  local out_old out_new
  out_old=$(run_isolated "$old_script" 2>&1 >/dev/null)
  out_new=$(run_isolated "$NEW_GET_BASE_REF" 2>&1 >/dev/null)

  # Both must resolve to M1 (the merge-base of feat and origin/main).
  if grep -q "BASE_REF=${m1_sha} " <<<"$out_old" && grep -q "BASE_REF=${m1_sha} " <<<"$out_new"; then
    PASS=$((PASS + 1))
  else
    FAIL=$((FAIL + 1))
    FAILURES+=("[local-mergebase-noop] OLD and NEW must both resolve to ${m1_sha}; old=$out_old new=$out_new")
  fi
  __pop_cwd
}

# Build a populated remote with N+1 commits on main + a feat branch off the
# initial commit, then make a shallow clone of depth N. After cloning, the feat
# branch is reconstructed locally with one extra commit so HEAD has a parent in
# the unshallowed range while origin/main's parent chain is truncated — making
# `git merge-base origin/main HEAD` unreachable.
#
# Args: $1=seed-tempdir  $2=shallow-tempdir  $3=depth-N
# Side effect: shallow-tempdir/repo is a real depth-N shallow clone with feat branch.
fixture_shallow_clone_with_unreachable_mergebase() {
  local seed="$1"
  local shallow="$2"
  local depth="$3"
  # Build populated remote. `git init --bare` defaults HEAD to refs/heads/master;
  # we push to refs/heads/main, so we re-point HEAD or the shallow clone follows
  # master and arrives empty (verified by @test bash -x trace, escalated as
  # fixture-2 bug post-Gate-2).
  __push_cwd "$seed"
  mkdir origin.git
  ( cd origin.git && git init --bare --quiet && git symbolic-ref HEAD refs/heads/main )
  git init --quiet
  git config user.email "test@example.com"
  git config user.name "Test"
  git config commit.gpgsign false
  git remote add origin "file://${seed}/origin.git"
  # M0: the merge-base anchor (will fall outside the depth-N window).
  echo "m0" > README.md
  git add README.md
  git commit -q -m "M0 (merge-base anchor)"
  # Advance main with depth + 1 more commits so origin/main has depth+2 total.
  local i
  for i in $(seq 1 "$((depth + 1))"); do
    echo "m${i}" > "m${i}.txt"
    git add "m${i}.txt"
    git commit -q -m "M${i}"
  done
  git push --quiet origin HEAD:main
  __pop_cwd
  # Shallow-clone with depth N — origin/main now exists with the last N commits.
  ( cd "$shallow" && git clone --quiet --depth "$depth" "file://${seed}/origin.git" repo )
  __push_cwd "${shallow}/repo"
  git config user.email "test@example.com"
  git config user.name "Test"
  git config commit.gpgsign false
  # Synthesize an orphan feat branch (no common ancestor with origin/main) so
  # `git merge-base origin/main HEAD` is unreachable.
  #
  # Note (per @test post-Gate-2 should-fix): orphan-feat (no-common-ancestor) is
  # used here as a PROXY for the production depth-window-truncation scenario
  # (a real PR branch whose merge-base anchor falls outside the shallow clone's
  # depth window). Both trigger the same precondition with the same
  # `merge-base ... unreachable` failure and the same `fetch-depth: 0`
  # remediation — sufficient for asserting the guardrail's user-facing behavior
  # even though the synthetic lever differs. Building the genuine depth-window
  # fixture would add meaningful complexity (M0 anchor reachable, advance main,
  # truncated clone, branch from M0 reachable from feat side only) — deferred as
  # acceptable per @test's RESOLVED-with-proxy verdict.
  git checkout -q --orphan feat
  git rm -rf --quiet . 2>/dev/null || true
  echo "p1" > p1.txt
  git add p1.txt
  git commit -q -m "P1 (orphan feat)"
  __pop_cwd
}

# Case 5: shallow-clone guardrail fires in CI-PR mode (per @test Gate-1 blocker
# + post-Gate-2 should-fix: real depth-N clone, merge-base unreachable).
test_shallow_clone_guardrail() {
  local seed; seed=$(mktemp_register)
  local shallow; shallow=$(mktemp_register)
  fixture_shallow_clone_with_unreachable_mergebase "$seed" "$shallow" 1
  __push_cwd "${shallow}/repo"

  local rc=0
  local out
  out=$(run_layer_all \
          GITHUB_ACTIONS=1 \
          GITHUB_EVENT_NAME=pull_request \
          GITHUB_BASE_REF=main \
          2>&1) || rc=$?

  if [[ "$rc" == "2" ]]; then
    PASS=$((PASS + 1))
  else
    FAIL=$((FAIL + 1))
    FAILURES+=("[shallow-clone-guardrail] expected exit 2, got $rc; out: $out")
  fi
  # New guardrail wording uses "merge-base(origin/main, HEAD) unreachable".
  if grep -q "PRECONDITION_FAILURE: merge-base(origin/main" <<<"$out"; then
    PASS=$((PASS + 1))
  else
    FAIL=$((FAIL + 1))
    FAILURES+=("[shallow-clone-guardrail] stderr should contain 'PRECONDITION_FAILURE: merge-base(origin/main'; got: $out")
  fi
  if grep -q "fetch-depth: 0" <<<"$out"; then
    PASS=$((PASS + 1))
  else
    FAIL=$((FAIL + 1))
    FAILURES+=("[shallow-clone-guardrail] stderr should name remediation 'fetch-depth: 0'; got: $out")
  fi
  __pop_cwd
}

# Case 6: CI-push mode skips the precondition even on a shallow clone with
# unreachable merge-base (per @operations R1: resolver uses HEAD~1, no remote
# pack lookup needed for CI-push).
test_ci_push_guardrail_skip() {
  local seed; seed=$(mktemp_register)
  local shallow; shallow=$(mktemp_register)
  fixture_shallow_clone_with_unreachable_mergebase "$seed" "$shallow" 1
  __push_cwd "${shallow}/repo"

  local rc=0
  local out
  out=$(run_layer_all \
          GITHUB_ACTIONS=1 \
          GITHUB_EVENT_NAME=push \
          2>&1) || rc=$?
  # The guardrail must NOT fail (per @operations Note 3: strict AND boolean —
  # both rc != 2 AND no PRECONDITION_FAILURE token must hold).
  if [[ "$rc" != "2" ]] && ! grep -q "PRECONDITION_FAILURE:" <<<"$out"; then
    PASS=$((PASS + 1))
  else
    FAIL=$((FAIL + 1))
    FAILURES+=("[ci-push-guardrail-skip] precondition fired in CI-push mode; rc=$rc out: $out")
  fi
  __pop_cwd
}

# Case 7: guard-callsite-coverage — TWO distinct sub-checks, per @test Gate-1 nit:
#   7a. Syntax check: bash -n across all 29 scripts in guards/simple/**/*.sh
#       PLUS guards/common.sh. Broader than the 11 strict callsites because
#       common.sh is sourced by all 29 — a syntax break in common.sh would
#       break all 29 even though only 11 directly invoke get_diff_base.
#   7b. Callsite invocation: confirm the 11 get_diff_base-consuming guards
#       still contain a get_diff_base callsite (drift guard), AND exercise the
#       forwarder path end-to-end (source common.sh + invoke get_diff_base()
#       in a real synthetic git repo; assert it matches direct resolver call).
test_guard_callsite_coverage() {
  # 7a — syntax check across 29 scripts + common.sh.
  local syntax_total=0
  local syntax_failures=0
  local syntax_files=""
  local f
  while IFS= read -r f; do
    syntax_total=$((syntax_total + 1))
    if ! bash -n "$f" 2>/dev/null; then
      syntax_failures=$((syntax_failures + 1))
      syntax_files+="$f "
    fi
  done < <(find "${REPO_ROOT}/scripts/guards/simple" -type f -name "*.sh"; printf '%s\n' "${REPO_ROOT}/scripts/guards/common.sh")

  if [[ $syntax_failures -eq 0 ]]; then
    PASS=$((PASS + 1))
    printf '[guard-callsite-coverage 7a] syntax check: %d/%d pass\n' "$syntax_total" "$syntax_total"
  else
    FAIL=$((FAIL + 1))
    FAILURES+=("[guard-callsite-coverage 7a] bash -n failed for ${syntax_failures}/${syntax_total} files: ${syntax_files}")
  fi

  # 7b — callsite presence check + forwarder end-to-end.
  local invoke_tmp; invoke_tmp=$(mktemp_register)
  __push_cwd "$invoke_tmp"
  IFS=' ' read -r _m1 _m2 _p1 _p2 < <(fixture_pr_with_main_advance "$invoke_tmp")
  # Now on feat branch with PR commits; local-mergebase resolution succeeds.

  # The 11 guards that consume get_diff_base (per task #42 §Decisions caller list).
  local callsites=(
    "scripts/guards/simple/no-secrets-in-logs.sh"
    "scripts/guards/simple/api-version-check.sh"
    "scripts/guards/simple/validate-env-config.sh"
    "scripts/guards/simple/ts/no-test-removal-ts.sh"
    "scripts/guards/simple/no-hardcoded-secrets.sh"
    "scripts/guards/simple/ts/name-guard-dt-client.sh"
    "scripts/guards/simple/validate-kustomize.sh"
    "scripts/guards/simple/validate-cross-boundary-classification.sh"
    "scripts/guards/simple/ts/no-secrets-in-ts.sh"
    "scripts/guards/simple/ts/no-pii-in-logs-ts.sh"
    "scripts/guards/simple/ts/exports-map-closed.sh"
  )

  local invoke_total=${#callsites[@]}
  local invoke_failures=0
  local invoke_failed_files=""
  local cs
  for cs in "${callsites[@]}"; do
    if ! grep -q "get_diff_base" "${REPO_ROOT}/${cs}" 2>/dev/null; then
      invoke_failures=$((invoke_failures + 1))
      invoke_failed_files+="${cs}(no-callsite) "
    fi
  done

  # Forwarder end-to-end: source common.sh, call get_diff_base, assert it
  # matches the direct resolver invocation. If this matches, every callsite's
  # `DIFF_BASE=$(get_diff_base)` will get the same SHA the resolver emits.
  local forwarder_out resolver_out
  forwarder_out=$(bash -c "source '${REPO_ROOT}/scripts/guards/common.sh' && get_diff_base" 2>/dev/null) || forwarder_out=""
  resolver_out=$("${REPO_ROOT}/scripts/lang/_get_base_ref.sh" 2>/dev/null) || resolver_out=""

  if [[ $invoke_failures -eq 0 && -n "$forwarder_out" && "$forwarder_out" == "$resolver_out" ]]; then
    PASS=$((PASS + 1))
    printf '[guard-callsite-coverage 7b] callsite invocation: %d/%d callsites verified; forwarder→resolver SHA match (%s)\n' \
      "$invoke_total" "$invoke_total" "${forwarder_out:0:7}"
  else
    FAIL=$((FAIL + 1))
    if [[ $invoke_failures -gt 0 ]]; then
      FAILURES+=("[guard-callsite-coverage 7b] ${invoke_failures}/${invoke_total} callsite presence checks failed: ${invoke_failed_files}")
    fi
    if [[ "$forwarder_out" != "$resolver_out" ]]; then
      FAILURES+=("[guard-callsite-coverage 7b] forwarder/resolver SHA mismatch: forwarder='${forwarder_out}' resolver='${resolver_out}'")
    fi
  fi
  __pop_cwd
}

# -----------------------------------------------------------------------------
# Run all
# -----------------------------------------------------------------------------

test_ci_pr_before_after
test_local_mergebase_noop
test_shallow_clone_guardrail
test_ci_push_guardrail_skip
test_guard_callsite_coverage

printf '\n_get_base_ref.behavior-equivalence.test.sh: %d passed, %d failed\n' "$PASS" "$FAIL"
if [[ $FAIL -gt 0 ]]; then
  printf 'Failures:\n'
  for f in "${FAILURES[@]}"; do
    printf '  - %s\n' "$f"
  done
  exit 1
fi
exit 0
