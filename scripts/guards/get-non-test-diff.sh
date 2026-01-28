#!/bin/bash
#
# Get Non-Test Diff
#
# Generates a unified diff of Rust changes excluding test files.
# Used by semantic guards to analyze only production code changes.
#
# Output: Unified diff format on stdout
#
# Exit codes:
#   0 - Success (diff generated, may be empty)
#   1 - Error
#
# Usage:
#   ./get-non-test-diff.sh              # Diff against HEAD
#   ./get-non-test-diff.sh main         # Diff against main branch
#   ./get-non-test-diff.sh HEAD~3       # Diff against 3 commits ago
#

set -euo pipefail

# Base commit to diff against (default: HEAD for staged/unstaged changes)
BASE_REF="${1:-HEAD}"

# Exclusion patterns for test files
# Uses git pathspec magic to exclude these patterns
EXCLUDE_PATTERNS=(
    ':!**/tests/**'
    ':!**/*_test.rs'
    ':!**/test_*.rs'
    ':!**/*-test-utils/**'
    ':!**/test-utils/**'
    ':!**/testutils/**'
    ':!**/testing/**'
    ':!**/benches/**'
    ':!**/examples/**'
    ':!**/fuzz/**'
)

# Generate the diff
# For HEAD, we want both staged and unstaged changes
if [[ "$BASE_REF" == "HEAD" ]]; then
    # Show staged + unstaged changes to tracked .rs files
    git diff HEAD -- '*.rs' "${EXCLUDE_PATTERNS[@]}" 2>/dev/null || true
else
    # Show diff between base ref and HEAD
    git diff "$BASE_REF"...HEAD -- '*.rs' "${EXCLUDE_PATTERNS[@]}" 2>/dev/null || true
fi
