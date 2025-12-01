# Git Hooks for Dark Tower

This directory contains shared git hooks for the Dark Tower project.

## Installation

After cloning the repository, run:

```bash
./.githooks/install.sh
```

This will configure git to use these shared hooks instead of the default `.git/hooks` directory.

## Available Hooks

### pre-commit

Runs automatically before each commit to ensure code quality:

- **cargo fmt --check**: Verifies code formatting
- **cargo clippy**: Checks for common mistakes and style issues

If any check fails, the commit will be aborted with instructions on how to fix the issues.

## Manual Formatting

To format code manually:

```bash
cargo fmt --all
```

To run clippy manually:

```bash
cargo clippy --all-targets --all-features
```

## Bypassing Hooks (Not Recommended)

In exceptional cases, you can bypass hooks with:

```bash
git commit --no-verify
```

**Note**: This is strongly discouraged as it can lead to CI failures.

## Uninstalling

To stop using shared hooks and revert to default git hooks:

```bash
git config --unset core.hooksPath
```
