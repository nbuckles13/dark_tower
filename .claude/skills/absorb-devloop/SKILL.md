---
name: absorb-devloop
description: Bring commits from a devloop clone back into the current branch
---

# Absorb Devloop

Merge commits from a devloop clone (created by `devloop.sh`) back into the current branch.

## When to Use

After a devloop completes in a clone, use this skill to bring those commits into your working branch. Devloop clones live at `<repo_root>/../worktrees/<slug>/` and work on `feature/<slug>` branches.

## Arguments

```
/absorb-devloop <slug>
```

- **slug** (required): The devloop task slug (e.g., `mh-quic-mh-notify`). Same slug passed to `devloop.sh`.

## Instructions

### Step 1: Locate and Validate the Clone

Derive the clone path and branch from the slug:

```bash
REPO_ROOT=$(git rev-parse --show-toplevel)
SLUG="<slug>"
CLONE_DIR="${REPO_ROOT}/../worktrees/${SLUG}"
CLONE_BRANCH="feature/${SLUG}"
```

Verify the clone exists and check its state:

```bash
cd "$CLONE_DIR"
git rev-parse --git-dir        # confirm it's a repo
git rev-parse --abbrev-ref HEAD # confirm branch
git status --short              # check for uncommitted work
```

If there's uncommitted work in the clone, warn the user and stop.

Show the clone's commits:

```bash
git log --oneline
```

### Step 2: Add Clone as Remote and Fetch

From the main repo:

```bash
git remote add "$SLUG" "$CLONE_DIR"
git fetch "$SLUG"
```

### Step 3: Analyze Divergence

Find what needs to come over:

```bash
# Merge base — where the clone branched from
MERGE_BASE=$(git merge-base HEAD "$SLUG/$CLONE_BRANCH")

# What we have that the clone doesn't
git log --oneline "$MERGE_BASE..HEAD"

# What the clone has that we don't
git log --oneline "$MERGE_BASE..$SLUG/$CLONE_BRANCH"
```

Check for duplicate commits (same change independently applied to both sides):

```bash
comm -12 \
  <(git log --format="%s" "$MERGE_BASE..HEAD" | sort) \
  <(git log --format="%s" "$MERGE_BASE..$SLUG/$CLONE_BRANCH" | sort)
```

Report the analysis to the user before proceeding:
- The merge base commit
- Commit counts on each side
- Any detected duplicates

### Step 4: Cherry-Pick from Clone

The default strategy is to cherry-pick the clone's commits onto the current branch. This preserves our existing history and adds the clone's work on top.

```bash
# List clone-only commits in chronological order
git log --oneline --reverse "$MERGE_BASE..$SLUG/$CLONE_BRANCH"
```

**If duplicates were detected:** identify which clone commits are duplicates (same commit message as commits already on our branch). Confirm with the user which to skip, then cherry-pick only the non-duplicate commits.

**If no duplicates:** cherry-pick all commits:

```bash
git cherry-pick <first-sha>^..<last-sha>
```

### Step 5: Resolve Conflicts

If conflicts occur during cherry-pick:

**Specialist INDEX files** (`docs/specialist-knowledge/*/INDEX.md`): These are updated by every devloop and always conflict in parallel work. Take the version from the cherry-picked commit (theirs):

```bash
git checkout --theirs docs/specialist-knowledge/*/INDEX.md
git add docs/specialist-knowledge/*/INDEX.md
```

**Devloop output files** (`docs/devloop-outputs/`): Same treatment — take theirs.

**Source code and other files:** Read the conflicting hunks, understand both sides, and resolve the merge. Most conflicts will be straightforward (e.g., adjacent additions in the same file).

After resolving:

```bash
git cherry-pick --continue --no-edit
```

Repeat for each conflicting commit in the sequence.

### Step 6: Verify

```bash
cargo check 2>&1 | tail -5
git log --oneline "$MERGE_BASE..HEAD"
```

### Step 7: Clean Up

```bash
git remote remove "$SLUG"
```

**Do NOT delete the clone directory** — the user may want to reference it or the containers may still be running.

## Special Cases

### Clone is a strict fast-forward (our branch has no unique commits)

Skip cherry-pick entirely:

```bash
git merge --ff-only "$SLUG/$CLONE_BRANCH"
```

### Our branch has unique commits that are duplicates of clone commits

If ALL our unique commits are duplicates of clone commits (same changes, different SHAs from independent creation), consider the reset approach:

1. Save our unique non-duplicate commits on a temp branch
2. Reset to the merge base
3. Fast-forward to the clone's HEAD
4. Cherry-pick our non-duplicate commits on top
5. Delete the temp branch

This produces a cleaner linear history since the clone's commit chain is unbroken. Only use this when confirmed with the user.

## See Also

- Devloop setup: `infra/devloop/devloop.sh`
- Devloop workflow: `.claude/skills/devloop/SKILL.md`
