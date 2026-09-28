#!/usr/bin/env bash
# absorb-devloop.test.sh — self-test for infra/devloop/absorb-devloop.sh.
#
# Hermetic: throwaway target repo + devloop clone under a temp dir, REAL git (the
# script's behaviour IS git's cherry-pick/sequencer state), global/system git config
# isolated so a developer's hooksPath or aliases cannot leak in.
#
# Covers the RESUME contract: after a conflict stop, a rerun refuses an incomplete
# resolution (unmerged paths, unstaged edits, staged conflict markers), refuses a
# paused commit that is not from the devloop branch and an unrelated merge, reports
# a pre-commit-hook refusal without losing the staged resolution, and otherwise
# finishes the paused commit and absorbs what the devloop branch still has —
# including commits that landed after the stop. A conflict-resolved commit is
# recorded by origin (-x) and not picked again on the next run.
#
# Wired into scripts/layer3.sh. Consumes scripts/lang/_test_helpers.sh.
set -euo pipefail
IFS=$'\n\t'

__here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${__here}/../.." && pwd)"
ABSORB="${__here}/absorb-devloop.sh"
# shellcheck source=../../scripts/lang/_test_helpers.sh
source "${REPO_ROOT}/scripts/lang/_test_helpers.sh"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1
export GIT_AUTHOR_NAME=t GIT_AUTHOR_EMAIL=t@example.invalid
export GIT_COMMITTER_NAME=t GIT_COMMITTER_EMAIL=t@example.invalid
set +e

T=""; W=""
# fixture <name>: target repo $T (branch main) and devloop clone $W (branch
# feature/<name>). The clone's first commit conflicts with the target's; the next
# two apply cleanly.
fixture() {
  T="${WORK}/$1/target"; W="${WORK}/$1/worktree"
  git -c init.defaultBranch=main init -q "$T"
  printf 'base\n' > "$T/f"; git -C "$T" add f; git -C "$T" commit -qm base
  git clone -q "$T" "$W"; git -C "$W" checkout -qb "feature/$1"
  printf 'devloop\n' > "$W/f"; git -C "$W" commit -qam "wt: change f"
  printf 'g\n' > "$W/g"; git -C "$W" add g; git -C "$W" commit -qm "wt: add g"
  printf 'h\n' > "$W/h"; git -C "$W" add h; git -C "$W" commit -qm "wt: add h"
  printf 'target\n' > "$T/f"; git -C "$T" commit -qam "target: change f"
}
absorb() { OUT="$(cd "$T" && bash "$ABSORB" "$@" 2>&1)"; RC=$?; }
paused() { git -C "$T" rev-parse -q --verify CHERRY_PICK_HEAD >/dev/null && echo yes || echo no; }
subjects() { git -C "$T" log --format=%s | tr '\n' '|'; }

# === (1) conflict stop, incomplete resolutions refused, then resume + remaining ====
fixture resume
absorb "$W"
assert_rc     "stop-on-conflict-exit3" 3 "$RC"
assert_status "stop-says-rerun" "rerun this script" "$OUT"
assert_status "stop-leaves-pick-paused" "yes" "$(paused)"

absorb "$W"
assert_rc     "unmerged-refused-exit3" 3 "$RC"
assert_status "unmerged-named" "unmerged path(s)" "$OUT"
assert_status "unmerged-still-paused" "yes" "$(paused)"

git -C "$T" add f   # stages the file WITH its conflict markers
absorb "$W"
assert_rc     "markers-refused-exit3" 3 "$RC"
assert_status "markers-named" "conflict markers are staged" "$OUT"
assert_status "markers-file-named" "f: <<<<<<<" "$OUT"

printf 'resolved\n' > "$T/f"; git -C "$T" add f
printf 'resolved, then edited\n' > "$T/f"
absorb "$W"
assert_rc     "unstaged-refused-exit3" 3 "$RC"
assert_status "unstaged-named" "unstaged edits" "$OUT"
git -C "$T" checkout -q -- f   # back to the staged resolution

head_before="$(git -C "$T" rev-parse HEAD)"
absorb --dry-run "$W"
assert_rc     "dry-run-resume-exit0" 0 "$RC"
assert_status "dry-run-resume-says-would" "would finish the paused commit" "$OUT"
assert_status "dry-run-resume-head-unchanged" "$head_before" "$(git -C "$T" rev-parse HEAD)"
assert_status "dry-run-resume-still-paused" "yes" "$(paused)"

# A commit lands on the devloop branch while the absorb is paused.
printf 'late\n' > "$W/late"; git -C "$W" add late; git -C "$W" commit -qm "wt: late commit"
absorb "$W"
assert_rc     "resume-exit0" 0 "$RC"
assert_status "resume-detected" "Interrupted absorb detected" "$OUT"
assert_status "resume-finished-paused" "Finished the paused commit: " "$OUT"
assert_status "resume-no-longer-paused" "no" "$(paused)"
assert_status "resume-absorbed-all-in-order" \
  "wt: late commit|wt: add h|wt: add g|wt: change f|target: change f|base|" "$(subjects)"
assert_status "resume-resolution-kept" "resolved" "$(cat "$T/f")"
assert_status "resume-records-origin" "(cherry picked from commit $(git -C "$W" rev-parse HEAD~3))" \
  "$(git -C "$T" log -1 --format=%B HEAD~3)"
assert_status "resume-late-commit-records-origin" "(cherry picked from commit" "$(git -C "$T" log -1 --format=%B)"

head_after="$(git -C "$T" rev-parse HEAD)"
absorb "$W"
assert_rc     "rerun-nothing-left-exit0" 0 "$RC"
assert_status "rerun-resolved-commit-not-repicked" "Nothing to absorb" "$OUT"
assert_status "rerun-head-unchanged" "$head_after" "$(git -C "$T" rev-parse HEAD)"

# === (2) the pre-commit hook refuses the paused commit ============================
fixture hook
absorb "$W"
assert_rc "hook-setup-conflict" 3 "$RC"
printf 'resolved\n' > "$T/f"; git -C "$T" add f
printf '#!/bin/sh\necho "hook says no" >&2\nexit 1\n' > "$T/.git/hooks/pre-commit"
chmod +x "$T/.git/hooks/pre-commit"
absorb "$W"
assert_rc     "hook-refused-exit3" 3 "$RC"
assert_status "hook-refused-named" "pre-commit hook refused the paused commit" "$OUT"
assert_status "hook-output-shown" "hook says no" "$OUT"
assert_status "hook-refused-still-paused" "yes" "$(paused)"
assert_status "hook-refused-resolution-still-staged" "f" "$(git -C "$T" diff --cached --name-only)"
rm "$T/.git/hooks/pre-commit"
absorb "$W"
assert_rc     "hook-fixed-resume-exit0" 0 "$RC"
assert_status "hook-fixed-absorbed" "wt: add h|wt: add g|wt: change f|" "$(subjects)"

# === (3) a pick paused WITHOUT -x (started by hand / an older script) =============
fixture legacy
git -C "$T" fetch -q "$W" "feature/legacy"
git -C "$T" cherry-pick "$(git -C "$W" rev-parse HEAD~2)" >/dev/null 2>&1
printf 'resolved\n' > "$T/f"; git -C "$T" add f
absorb "$W"
assert_rc     "legacy-resume-exit0" 0 "$RC"
assert_status "legacy-origin-appended" "(cherry picked from commit $(git -C "$W" rev-parse HEAD~2))" \
  "$(git -C "$T" log -1 --format=%B HEAD~2)"
assert_status "legacy-remaining-absorbed" "wt: add h|wt: add g|wt: change f|" "$(subjects)"

# === (4) empty resolution: the change is already on the target ====================
fixture empty
absorb "$W"
assert_rc "empty-setup-conflict" 3 "$RC"
git -C "$T" checkout -q HEAD -- f   # resolve to the target's side: nothing to commit
absorb "$W"
assert_rc     "empty-resume-exit0" 0 "$RC"
assert_status "empty-recorded" "Resolution is empty" "$OUT"
assert_status "empty-rest-absorbed" "wt: add h|wt: add g|wt: change f|" "$(subjects)"
absorb "$W"
assert_status "empty-not-repicked" "Nothing to absorb" "$OUT"

# === (5) not an interrupted absorb: refuse, change nothing ========================
fixture foreign
git -C "$T" checkout -qb side HEAD~1
printf 'side\n' > "$T/f"; git -C "$T" commit -qam "side: change f"
git -C "$T" checkout -q main
git -C "$T" cherry-pick side >/dev/null 2>&1
printf 'resolved\n' > "$T/f"; git -C "$T" add f
absorb "$W"
assert_rc     "foreign-pick-refused-exit3" 3 "$RC"
assert_status "foreign-pick-named" "is not a commit from feature/foreign" "$OUT"
assert_status "foreign-pick-left-paused" "yes" "$(paused)"
git -C "$T" cherry-pick --abort

git -C "$T" merge side >/dev/null 2>&1
absorb "$W"
assert_rc     "merge-in-progress-refused-exit3" 3 "$RC"
assert_status "merge-in-progress-named" "rebase, merge or revert is in progress" "$OUT"
git -C "$T" merge --abort

# === (6) a clean cherry-pick records origins too; a fast-forward keeps the shas ===
fixture clean
git -C "$T" reset -q --hard HEAD~1   # drop the target's conflicting commit...
printf 'other\n' > "$T/other"; git -C "$T" add other; git -C "$T" commit -qm "target: unrelated"
absorb "$W"
assert_rc     "clean-exit0" 0 "$RC"
assert_status "clean-was-cherry-pick" "Plan: cherry-pick 3" "$OUT"
assert_status "clean-records-origin" "(cherry picked from commit $(git -C "$W" rev-parse HEAD))" \
  "$(git -C "$T" log -1 --format=%B)"

fixture ff
git -C "$T" reset -q --hard HEAD~1
absorb "$W"
assert_rc     "ff-exit0" 0 "$RC"
assert_status "ff-strategy" "Plan: fast-forward 3" "$OUT"
assert_status "ff-same-shas" "$(git -C "$W" rev-parse HEAD)" "$(git -C "$T" rev-parse HEAD)"

report_results "infra/devloop/absorb-devloop.test.sh"
