---
name: worktree-setup
description: Create and configure a git worktree ready for Claude Code development with Dark Tower
---

# Worktree Setup

Create a new git worktree configured for parallel Claude Code development. This skill automates the setup of worktrees with proper permissions sharing and validation.

## Purpose

Git worktrees allow you to work on multiple branches simultaneously without switching contexts. This is perfect for running parallel devloops on different features.

## Arguments

```
/worktree-setup <branch-name> [worktree-path]
```

- **branch-name** (required): Name of the branch to create/checkout in the worktree
- **worktree-path** (optional): Where to create the worktree (default: `../branch-name`)

## What This Skill Does

1. **Creates git worktree** at the specified path
2. **Shares permissions** by symlinking `.claude/settings.local.json` from main repo
3. **Validates setup** with cargo check and guards
4. **Reports status** so you know it's ready to use

## Instructions

### Step 1: Validate Arguments

Check that:
- Branch name is provided
- Branch name is valid (alphanumeric, hyphens, slashes allowed)
- Target path doesn't already exist (if provided)

### Step 2: Determine Worktree Path

If worktree-path not provided, default to:
```bash
WORKTREE_PATH="../${BRANCH_NAME}"
```

Convert to absolute path:
```bash
WORKTREE_PATH=$(realpath -m "$WORKTREE_PATH")
```

### Step 3: Find Main Repository Root

```bash
MAIN_REPO=$(git rev-parse --show-toplevel)
```

This works even when run from a worktree.

### Step 4: Create Git Worktree

```bash
cd "$MAIN_REPO"
git worktree add "$WORKTREE_PATH" "$BRANCH_NAME"
```

If the branch doesn't exist, git will create it. If it exists, git will check it out.

**Handle errors**:
- If path already exists: error message
- If branch conflicts: let user decide (git will prompt)

### Step 5: Set Up Claude Code Permissions

Create `.claude` directory and symlink settings:

```bash
mkdir -p "$WORKTREE_PATH/.claude"
ln -sf "$MAIN_REPO/.claude/settings.local.json" \
       "$WORKTREE_PATH/.claude/settings.local.json"
```

**Verify symlink**:
```bash
if [[ -L "$WORKTREE_PATH/.claude/settings.local.json" ]]; then
    echo "✅ Permissions shared from main repo"
else
    echo "❌ Failed to create permissions symlink"
    exit 1
fi
```

### Step 6: Run Quick Validation

Verify the worktree is minimally functional:

```bash
cd "$WORKTREE_PATH"

# Quick check: verify we can find .git and scripts exist
if [[ ! -e .git ]]; then
    echo "❌ .git not found in worktree"
    exit 1
fi

if [[ ! -x ./scripts/guards/run-guards.sh ]]; then
    echo "❌ Guards script not found or not executable"
    exit 1
fi

echo "✅ Basic structure validated"
```

**Note**: We skip `cargo check` here because it's slow. The worktree is ready to use - any compilation issues will be caught when you start working.

### Step 7: Report Status

**If all successful**:
```
✅ Worktree created: $WORKTREE_PATH
✅ Branch: $BRANCH_NAME
✅ Permissions shared from main repo
✅ Basic structure validated

Ready for development! To start working:
  cd $WORKTREE_PATH
  claude code

When finished with this worktree:
  git worktree remove $WORKTREE_PATH
```

## Common Usage Patterns

### Create worktree for a new feature
```
/worktree-setup feature/new-feature
```
Creates worktree at `../feature/new-feature`

### Create worktree in specific location
```
/worktree-setup feature/auth-v2 /home/nathan/code/auth-v2
```
Creates worktree at specified path

### Work on existing branch in new worktree
```
/worktree-setup existing-branch
```
Checks out existing branch in new worktree

## Cleanup After Finishing Work

When done with a worktree:

```bash
# List all worktrees
git worktree list

# Remove a worktree (from any git directory in the repo)
git worktree remove /path/to/worktree

# Or from the worktree itself
cd /path/to/worktree
git worktree remove .
```

**Note**: Removing a worktree does NOT delete the branch, only the working directory.

## Troubleshooting

### "Permission denied" errors in worktree

The symlink may have failed. Manually create it:
```bash
cd /path/to/worktree
MAIN_REPO=$(git rev-parse --show-toplevel)
ln -sf "$MAIN_REPO/.claude/settings.local.json" \
       .claude/settings.local.json
```

### Cargo/compilation errors

These are usually branch-specific issues, not worktree issues. Debug normally:
```bash
cd /path/to/worktree
cargo check --workspace
cargo test --workspace
```

## Technical Notes

### Why Symlink Permissions?

Claude Code's permission system is path-specific. When you approve `cargo check` in the main repo, that approval is stored in `.claude/settings.local.json` with the full path.

By symlinking this file, the worktree inherits all permissions from the main repo without re-prompting.

### Why Validate After Setup?

Early validation catches issues before you start a devloop:
- Missing dependencies
- Guard violations (e.g., from branch state)
- Compilation errors

Better to fix these before invoking specialists.

### Worktree Locations

Git worktrees can be anywhere on the filesystem. Common patterns:
- **Sibling directories**: `../feature-name` (default)
- **Flat hierarchy**: `/home/nathan/code/project-feature-name`
- **Temp directory**: `/tmp/worktree-feature-name` (for short-lived work)

The symlink approach works regardless of location.

## Limitations

- **Database tests**: All worktrees share the same test database. If running tests in parallel, use different database URLs or sequential execution.
- **Git operations**: Some git commands (like `git worktree list`) work from any worktree, others are worktree-specific.

## See Also

- Git worktrees: `git worktree --help`
- Claude Code documentation: https://code.claude.com/docs/en/common-workflows.md (Git worktrees section)
- Devloop workflow: `.claude/skills/devloop/SKILL.md`
