#!/bin/bash
# absorb-devloop.sh — Absorb a completed devloop's commits back into the source branch.
#
# Detects whether the devloop branch fast-forwards from the current HEAD;
# if so, runs `git merge --ff-only`. Otherwise cherry-picks the commits
# the devloop added on top of its (potentially older) base.
#
# Uses `git cherry` (patch-id comparison) to filter the cherry-pick queue,
# skipping commits whose content is already on the target branch. This is
# the common case when the clone's base chain was rebased into target under
# different SHAs (e.g., after a `--rebase` merge of a parent PR).
#
# Conflict resolution relies on `.gitattributes` `merge=union` for
# `docs/user-stories/*.md` and `docs/TODO.md` (the two most common conflict
# sources). For anything else, the script stops and surfaces the conflict for
# manual resolution.
#
# RESUME: after a conflict stop, resolve and `git add` the files, then rerun the
# SAME command. The rerun detects the interrupted cherry-pick and refuses (exit 3)
# unless the resolution is complete: no unmerged paths, no unstaged edits, and no
# leftover conflict markers in the staged content. It also requires the paused
# commit to belong to this devloop's branch. It then finishes the paused commit
# (the pre-commit hook runs, as it would for `git cherry-pick --continue`) and
# goes on to absorb whatever the devloop branch still has that the target lacks.
#
# Every absorbed commit carries git's `(cherry picked from commit <sha>)` line
# (`-x`). Planning skips a commit whose sha is recorded that way on the target as
# well as one whose patch-id is already there. A conflict-resolved commit has a
# different patch-id, so without the record a rerun would pick it again.
#
# Usage:
#   ./absorb-devloop.sh <task-slug>             # canonical form
#   ./absorb-devloop.sh <worktree-path>         # explicit path
#   ./absorb-devloop.sh --dry-run <task-slug>   # show plan, don't execute
#
# Examples:
#   ./infra/devloop/absorb-devloop.sh browser-client-join-task35
#   ./infra/devloop/absorb-devloop.sh ~/code/worktrees/browser-client-join-task35
#   ./infra/devloop/absorb-devloop.sh --dry-run browser-client-join-task35
#
# Exit codes:
#   0  — success
#   1  — usage error
#   2  — fetch/setup failure
#   3  — cherry-pick conflict (resolve, `git add`, rerun), an incomplete
#        resolution on resume, a paused commit the pre-commit hook refused, or an
#        unrelated rebase/merge/revert/cherry-pick in progress
#   4  — working tree dirty; refuses to run

set -euo pipefail

# ─── Args + helpers ─────────────────────────────────────────────

DRY_RUN=false
if [[ "${1:-}" == --dry-run ]]; then
    DRY_RUN=true
    shift
fi

ARG="${1:?Usage: absorb-devloop.sh [--dry-run] <task-slug-or-worktree-path>}"

REPO_ROOT="$(git rev-parse --show-toplevel)"
cd "$REPO_ROOT"

# Resolve worktree path: accept either a slug or an explicit path
if [[ -d "$ARG/.git" || -f "$ARG/.git" ]]; then
    WORKTREE="$(cd "$ARG" && pwd)"
else
    WORKTREE="${REPO_ROOT}/../worktrees/${ARG}"
    if [[ ! -d "$WORKTREE/.git" && ! -f "$WORKTREE/.git" ]]; then
        echo "ERROR: No worktree found at: $WORKTREE" >&2
        echo "  (passed argument was '$ARG'; expected slug or path)" >&2
        exit 1
    fi
fi

say()  { echo "==> $*"; }
warn() { echo "WARN: $*" >&2; }
fail() { echo "ERROR: $1" >&2; exit "${2:-2}"; }

# ─── Preflight ──────────────────────────────────────────────────

GIT_DIR_ABS="$(git rev-parse --absolute-git-dir)"
if [[ -d "$GIT_DIR_ABS/rebase-merge" || -d "$GIT_DIR_ABS/rebase-apply" ]] \
    || git rev-parse -q --verify MERGE_HEAD >/dev/null \
    || git rev-parse -q --verify REVERT_HEAD >/dev/null; then
    fail "A rebase, merge or revert is in progress in $REPO_ROOT — that is not an interrupted absorb. Finish or abort it, then rerun." 3
fi
PICK_IN_PROGRESS=false
if git rev-parse -q --verify CHERRY_PICK_HEAD >/dev/null || [[ -d "$GIT_DIR_ABS/sequencer" ]]; then
    PICK_IN_PROGRESS=true
fi

SOURCE_BRANCH="$(git -C "$WORKTREE" branch --show-current 2>/dev/null || true)"
[[ -n "$SOURCE_BRANCH" ]] || fail "Source worktree is in detached HEAD; cannot identify branch."

TARGET_BRANCH="$(git branch --show-current)"
say "Source: $SOURCE_BRANCH @ $WORKTREE"
say "Target: $TARGET_BRANCH @ $REPO_ROOT"

# ─── Fetch into a temp ref ──────────────────────────────────────

TMP_REF="_absorb-tmp/${SOURCE_BRANCH##*/}"
cleanup() { git branch -D "$TMP_REF" 2>/dev/null || true; }
trap cleanup EXIT

say "Fetching $SOURCE_BRANCH from worktree..."
git fetch "$WORKTREE" "+${SOURCE_BRANCH}:${TMP_REF}" >/dev/null 2>&1 || \
    fail "Fetch failed (is the worktree branch '$SOURCE_BRANCH' valid?)"

# ─── Resume an interrupted absorb ───────────────────────────────

# paused_sha — the sha of the paused commit, or empty when none is paused.
paused_sha() { git rev-parse -q --verify CHERRY_PICK_HEAD 2>/dev/null || true; }

stop_on_conflict() {
    echo >&2
    warn "Cherry-pick paused on conflict."
    warn "  Resolve the conflicts and \`git add\` them, then rerun this script with the"
    warn "  same argument — it finishes the paused commit and absorbs the rest."
    warn "  Or to abort entirely: git cherry-pick --abort"
    warn ""
    warn "  Hint: .gitattributes resolves most user-story + TODO.md"
    warn "  conflicts via merge=union. Other conflicts are genuine."
    exit 3
}

resume_interrupted_absorb() {
    local paused unmerged markers msg_file before
    paused="$(paused_sha)"

    if [[ -z "$paused" ]]; then
        # A sequence with no paused commit (someone committed mid-sequence by hand).
        say "Interrupted absorb detected: a cherry-pick sequence is pending."
        if $DRY_RUN; then
            say "Dry run — would continue the pending sequence, then plan the remaining absorb."
            exit 0
        fi
        GIT_EDITOR=true git cherry-pick --continue || stop_on_conflict
        return 0
    fi

    # The paused commit must come from THIS devloop's branch; never finish
    # someone else's cherry-pick under an absorb banner.
    git merge-base --is-ancestor "$paused" "$TMP_REF" \
        || fail "The paused cherry-pick (${paused:0:7}) is not a commit from $SOURCE_BRANCH. Finish or abort it (git cherry-pick --continue / --abort), then rerun." 3

    say "Interrupted absorb detected: paused on $(git log -1 --format='%h %s' "$paused")"

    unmerged="$(git diff --name-only --diff-filter=U)"
    [[ -z "$unmerged" ]] || fail "Resolution incomplete — unmerged path(s):
$(printf '%s\n' "$unmerged" | sed 's/^/    /')
  Resolve them, \`git add\` them, then rerun." 3

    git diff --quiet || fail "Resolution incomplete — tracked files have unstaged edits:
$(git diff --name-only | sed 's/^/    /')
  Stage the resolution (\`git add\`) or discard the edits, then rerun." 3

    # Leftover conflict markers in ADDED staged lines. `=======` alone is left
    # out: it is a legitimate Markdown setext underline.
    markers="$(git diff --cached -U0 | awk '
        /^\+\+\+ /                                   { f=substr($0,7); next }
        /^\+(<<<<<<<|\|\|\|\|\|\|\||>>>>>>>)( |$)/   { print "    " f ": " substr($0,2) }')"
    [[ -z "$markers" ]] || fail "Resolution incomplete — conflict markers are staged:
$markers
  Finish resolving those hunks, \`git add\` them, then rerun." 3

    if $DRY_RUN; then
        say "Dry run — resolution is complete; would finish the paused commit, then plan the remaining absorb."
        exit 0
    fi

    # Record the origin (-x) so later planning recognises this commit even
    # though its resolved patch-id differs from the source.
    msg_file="$GIT_DIR_ABS/MERGE_MSG"
    if ! grep -qF "(cherry picked from commit $paused)" "$msg_file" 2>/dev/null; then
        printf '\n(cherry picked from commit %s)\n' "$paused" >> "$msg_file"
    fi

    before="$(git rev-parse HEAD)"
    if git diff --cached --quiet; then
        # The resolution is identical to HEAD: the change is already on the
        # target. Commit it empty so the origin record exists and a rerun does
        # not pick it again.
        say "Resolution is empty (the change is already on $TARGET_BRANCH) — recording it as an empty commit."
        git commit --quiet --allow-empty --no-edit \
            || fail "The pre-commit hook refused the paused commit (output above). Fix what it reports, then rerun. (Gate-2 refusals: docs/runbooks/devloop-validation.md §8.5.)" 3
        if [[ -d "$GIT_DIR_ABS/sequencer" ]]; then
            GIT_EDITOR=true git cherry-pick --continue || stop_on_conflict
        fi
    elif ! GIT_EDITOR=true git cherry-pick --continue; then
        if [[ "$(git rev-parse HEAD)" == "$before" && "$(paused_sha)" == "$paused" ]]; then
            fail "The pre-commit hook refused the paused commit (output above); the resolution is still staged. An unedited devloop replay needs no Gate-2 verdict, so a Gate-2 refusal names its reason. If main.md was edited, take --theirs for docs/devloop-outputs/** and rerun. Otherwise a local verdict needs Layer 7's devloop cluster helper (./scripts/layer-all.sh && git add -A, then rerun); without it, committing with --no-verify is the user's decision. See docs/runbooks/devloop-validation.md §8.5." 3
        fi
        stop_on_conflict
    fi
    say "Finished the paused commit: $(git log -1 --format='%h %s')"
}

if $PICK_IN_PROGRESS; then
    resume_interrupted_absorb
fi

if ! git diff --quiet || ! git diff --cached --quiet; then
    fail "Working tree dirty. Commit or stash before absorbing." 4
fi

# ─── Plan ───────────────────────────────────────────────────────

MERGE_BASE="$(git merge-base HEAD "$TMP_REF")"
HEAD_SHA="$(git rev-parse HEAD)"
TIP_SHA="$(git rev-parse "$TMP_REF")"

if [[ "$HEAD_SHA" == "$TIP_SHA" ]]; then
    say "Already at $TIP_SHA — nothing to absorb."
    exit 0
fi

# Filter via patch-id: drop commits whose content is already on target.
# `git cherry <upstream> <head>` walks <upstream>..<head> and prints
#   `+ <sha>` for commits whose patch-id is NOT on <upstream>
#   `- <sha>` for commits whose patch-id IS already on <upstream>
# Output is OLDEST-first, which is the natural cherry-pick apply order —
# later commits can depend on file-existence semantics from earlier ones
# (e.g., a chain that creates main.md in commit N and modifies it in N+1
# only cherry-picks cleanly when applied in that order).
#
# Also skip a commit whose sha the target records as its origin (the `-x` line):
# a conflict-resolved absorb has a different patch-id, so patch-id alone would
# queue it again on the next run.
ABSORBED_ORIGINS="$(git log --format=%B "${MERGE_BASE}..HEAD" \
    | sed -n 's/^(cherry picked from commit \([0-9a-f]\{40\}\))$/\1/p' | sort -u)"
COMMITS=()
SKIPPED=()
while IFS=' ' read -r flag sha; do
    if [[ "$flag" == + ]] && grep -qxF "$sha" <<< "$ABSORBED_ORIGINS"; then
        flag=-
    fi
    case "$flag" in
        +) COMMITS+=("$sha") ;;
        -) SKIPPED+=("$sha") ;;
    esac
done < <(git cherry HEAD "$TMP_REF")

COMMIT_COUNT=${#COMMITS[@]}
SKIPPED_COUNT=${#SKIPPED[@]}

if [[ "$COMMIT_COUNT" -eq 0 ]]; then
    if [[ "$SKIPPED_COUNT" -gt 0 ]]; then
        say "All $SKIPPED_COUNT commit(s) from $SOURCE_BRANCH already on $TARGET_BRANCH (patch-id match or recorded origin). Nothing to absorb."
    else
        say "No new commits to absorb."
    fi
    exit 0
fi

if [[ "$MERGE_BASE" == "$HEAD_SHA" && "$SKIPPED_COUNT" -eq 0 ]]; then
    STRATEGY="fast-forward"
else
    STRATEGY="cherry-pick"
fi

say "Plan: $STRATEGY $COMMIT_COUNT commit(s) into $TARGET_BRANCH"
echo
for sha in "${COMMITS[@]}"; do
    git log -1 --oneline "$sha" | sed 's/^/    /'
done
echo

if [[ "$SKIPPED_COUNT" -gt 0 ]]; then
    say "Skipping $SKIPPED_COUNT commit(s) already on $TARGET_BRANCH (patch-id match or recorded origin):"
    SHOW=5
    for sha in "${SKIPPED[@]:0:$SHOW}"; do
        git log -1 --oneline "$sha" | sed 's/^/    - /'
    done
    if [[ "$SKIPPED_COUNT" -gt "$SHOW" ]]; then
        echo "    ... and $((SKIPPED_COUNT - SHOW)) more"
    fi
    echo
fi

if $DRY_RUN; then
    say "Dry run — no changes applied."
    exit 0
fi

# ─── Execute ────────────────────────────────────────────────────

case "$STRATEGY" in
    fast-forward)
        say "Fast-forwarding..."
        git merge --ff-only "$TMP_REF"
        ;;
    cherry-pick)
        say "Cherry-picking $COMMIT_COUNT commit(s)..."
        git cherry-pick -x "${COMMITS[@]}" || stop_on_conflict
        ;;
esac

# ─── Summary ────────────────────────────────────────────────────

FINAL_SHA="$(git rev-parse HEAD)"
say "Absorbed $COMMIT_COUNT commit(s) from $SOURCE_BRANCH"
say "Branch $TARGET_BRANCH advanced: ${HEAD_SHA:0:7} -> ${FINAL_SHA:0:7}"

# Reminder about the worktree
echo
say "Worktree at $WORKTREE is now stale relative to $TARGET_BRANCH."
say "Clean it up when ready: rm -rf '$WORKTREE'  # (or keep for re-runs)"
