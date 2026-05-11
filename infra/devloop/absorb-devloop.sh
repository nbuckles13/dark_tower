#!/bin/bash
# absorb-devloop.sh — Absorb a completed devloop's commits back into the source branch.
#
# Detects whether the devloop branch fast-forwards from the current HEAD;
# if so, runs `git merge --ff-only`. Otherwise cherry-picks the commits
# the devloop added on top of its (potentially older) base.
#
# Conflict resolution relies on `.gitattributes` `merge=union` for
# `docs/user-stories/*.md` and `docs/TODO.md` (the two most common conflict
# sources). For anything else, the script aborts cleanly and surfaces the
# conflict for manual resolution.
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
#   3  — cherry-pick or merge had a conflict; manual resolution needed
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
fail() { echo "ERROR: $*" >&2; exit "${2:-2}"; }

# ─── Preflight ──────────────────────────────────────────────────

if ! git diff --quiet || ! git diff --cached --quiet; then
    fail "Working tree dirty. Commit or stash before absorbing." 4
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

# ─── Plan ───────────────────────────────────────────────────────

MERGE_BASE="$(git merge-base HEAD "$TMP_REF")"
HEAD_SHA="$(git rev-parse HEAD)"
TIP_SHA="$(git rev-parse "$TMP_REF")"

if [[ "$HEAD_SHA" == "$TIP_SHA" ]]; then
    say "Already at $TIP_SHA — nothing to absorb."
    exit 0
fi

# Commits to bring in (in chronological order, oldest first)
mapfile -t COMMITS < <(git log --reverse --format=%H "${MERGE_BASE}..${TMP_REF}")
COMMIT_COUNT=${#COMMITS[@]}

if [[ "$COMMIT_COUNT" -eq 0 ]]; then
    say "No new commits to absorb."
    exit 0
fi

if [[ "$MERGE_BASE" == "$HEAD_SHA" ]]; then
    STRATEGY="fast-forward"
else
    STRATEGY="cherry-pick"
fi

say "Plan: $STRATEGY $COMMIT_COUNT commit(s) into $TARGET_BRANCH"
echo
git log --oneline --reverse "${MERGE_BASE}..${TMP_REF}" | sed 's/^/    /'
echo

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
        if ! git cherry-pick "${COMMITS[@]}"; then
            echo >&2
            warn "Cherry-pick paused on conflict."
            warn "  Resolve conflicts, then run: git cherry-pick --continue"
            warn "  Or to abort entirely: git cherry-pick --abort"
            warn ""
            warn "  Hint: .gitattributes resolves most user-story + TODO.md"
            warn "  conflicts via merge=union. Other conflicts are genuine."
            exit 3
        fi
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
