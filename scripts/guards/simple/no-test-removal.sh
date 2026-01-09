#!/bin/bash
#
# Simple Guard: No Test Removal
#
# Detects when tests are removed or weakened in staged changes.
# This is a WARNING guard - it doesn't block commits but surfaces
# test changes for review.
#
# What it detects:
#   - Test functions removed (fewer #[test] attributes)
#   - Assertions removed (fewer assert! calls)
#   - #[ignore] added to tests
#   - Test files deleted
#
# Exit codes:
#   0 - No warnings (or no test changes)
#   0 - Warnings found (non-blocking, just informational)
#   2 - Script error
#
# Usage:
#   ./no-test-removal.sh              # Check staged changes (pre-commit)
#   ./no-test-removal.sh [path]       # Skip if path provided (not applicable)
#
# Note: This guard only makes sense for pre-commit checks. When run with
# a path argument (e.g., from run-guards.sh), it exits successfully.
#

set -euo pipefail

# Source common library
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/../common.sh"

# If a path argument is provided, skip this guard (it only works with git staging)
if [[ $# -gt 0 ]]; then
    echo "Skipping no-test-removal guard (only runs on staged changes, not path scans)"
    exit 0
fi

# Initialize
init_violations
start_timer

print_header "Guard: Test Modification Check"

# Track warnings (separate from violations since this is non-blocking)
WARNINGS=0

# Get list of staged .rs files
STAGED_FILES=$(git diff --cached --name-only --diff-filter=M 2>/dev/null | grep '\.rs$' || true)
DELETED_FILES=$(git diff --cached --name-only --diff-filter=D 2>/dev/null | grep '\.rs$' || true)

if [[ -z "$STAGED_FILES" && -z "$DELETED_FILES" ]]; then
    echo -e "${GREEN}No Rust files modified or deleted in staged changes${NC}"
    print_elapsed_time
    exit 0
fi

# -----------------------------------------------------------------------------
# Check 1: Deleted test files
# -----------------------------------------------------------------------------
print_section "Check 1: Deleted test files"

deleted_test_files=""
for file in $DELETED_FILES; do
    # Check if deleted file was a test file
    if [[ "$file" == *"_test.rs" || "$file" == *"/tests/"* || "$file" == *"test_"* ]]; then
        deleted_test_files="$deleted_test_files$file\n"
    fi
done

if [[ -n "$deleted_test_files" ]]; then
    echo -e "${YELLOW}WARNING: Test files deleted:${NC}"
    echo -e "$deleted_test_files" | while read -r line; do
        [[ -n "$line" ]] && echo "  - $line"
    done
    ((WARNINGS++)) || true
    echo ""
else
    print_ok "No test files deleted"
    echo ""
fi

# -----------------------------------------------------------------------------
# Check 2: Test function count changes
# -----------------------------------------------------------------------------
print_section "Check 2: Test function count changes"

test_changes_found=false
for file in $STAGED_FILES; do
    # Skip non-existent files (deleted)
    [[ ! -f "$file" ]] && continue

    # Count #[test] and #[sqlx::test] in HEAD vs staged
    head_count=$(git show HEAD:"$file" 2>/dev/null | grep -cE '#\[(test|sqlx::test|tokio::test)' 2>/dev/null || true)
    head_count=${head_count:-0}
    staged_count=$(git show :"$file" 2>/dev/null | grep -cE '#\[(test|sqlx::test|tokio::test)' 2>/dev/null || true)
    staged_count=${staged_count:-0}

    if [[ "$staged_count" -lt "$head_count" ]]; then
        if [[ "$test_changes_found" == false ]]; then
            echo -e "${YELLOW}WARNING: Test functions removed:${NC}"
            test_changes_found=true
        fi
        removed=$((head_count - staged_count))
        echo "  $file: $removed test(s) removed (was $head_count, now $staged_count)"
        ((WARNINGS++)) || true
    fi
done

if [[ "$test_changes_found" == false ]]; then
    print_ok "No test functions removed"
fi
echo ""

# -----------------------------------------------------------------------------
# Check 3: Assertion count changes
# -----------------------------------------------------------------------------
print_section "Check 3: Assertion count changes"

assertion_changes_found=false
for file in $STAGED_FILES; do
    # Skip non-existent files
    [[ ! -f "$file" ]] && continue

    # Only check files that contain tests
    staged_has_tests=$(git show :"$file" 2>/dev/null | grep -cE '#\[(test|sqlx::test|tokio::test)' 2>/dev/null || true)
    staged_has_tests=${staged_has_tests:-0}
    [[ "$staged_has_tests" -eq 0 ]] && continue

    # Count assertions in HEAD vs staged
    head_count=$(git show HEAD:"$file" 2>/dev/null | grep -cE '\bassert(_eq|_ne|_matches)?!' 2>/dev/null || true)
    head_count=${head_count:-0}
    staged_count=$(git show :"$file" 2>/dev/null | grep -cE '\bassert(_eq|_ne|_matches)?!' 2>/dev/null || true)
    staged_count=${staged_count:-0}

    # Only warn if significant reduction (more than 2 assertions removed)
    if [[ "$staged_count" -lt "$head_count" ]]; then
        removed=$((head_count - staged_count))
        if [[ "$removed" -gt 2 ]]; then
            if [[ "$assertion_changes_found" == false ]]; then
                echo -e "${YELLOW}WARNING: Assertions removed (>2):${NC}"
                assertion_changes_found=true
            fi
            echo "  $file: $removed assertion(s) removed (was $head_count, now $staged_count)"
            ((WARNINGS++)) || true
        fi
    fi
done

if [[ "$assertion_changes_found" == false ]]; then
    print_ok "No significant assertion reductions"
fi
echo ""

# -----------------------------------------------------------------------------
# Check 4: #[ignore] additions
# -----------------------------------------------------------------------------
print_section "Check 4: #[ignore] additions"

ignore_changes_found=false
for file in $STAGED_FILES; do
    # Skip non-existent files
    [[ ! -f "$file" ]] && continue

    # Count #[ignore] in HEAD vs staged
    head_count=$(git show HEAD:"$file" 2>/dev/null | grep -cE '#\[ignore' 2>/dev/null || true)
    head_count=${head_count:-0}
    staged_count=$(git show :"$file" 2>/dev/null | grep -cE '#\[ignore' 2>/dev/null || true)
    staged_count=${staged_count:-0}

    if [[ "$staged_count" -gt "$head_count" ]]; then
        if [[ "$ignore_changes_found" == false ]]; then
            echo -e "${YELLOW}WARNING: #[ignore] added to tests:${NC}"
            ignore_changes_found=true
        fi
        added=$((staged_count - head_count))
        echo "  $file: $added #[ignore] attribute(s) added"
        ((WARNINGS++)) || true
    fi
done

if [[ "$ignore_changes_found" == false ]]; then
    print_ok "No #[ignore] attributes added"
fi
echo ""

# -----------------------------------------------------------------------------
# Summary
# -----------------------------------------------------------------------------
print_header "Summary"

print_elapsed_time
echo ""

if [[ $WARNINGS -gt 0 ]]; then
    echo -e "${YELLOW}Found $WARNINGS warning(s) about test modifications${NC}"
    echo ""
    echo "────────────────────────────────────────────────────────────────"
    echo "If these changes are intentional (e.g., removing obsolete tests,"
    echo "consolidating assertions), proceed with commit."
    echo ""
    echo "If unintentional, please restore the removed tests."
    echo "────────────────────────────────────────────────────────────────"
    echo ""
    # Exit 0 - this is a warning, not a blocker
    exit 0
else
    echo -e "${GREEN}No test modification warnings${NC}"
    exit 0
fi
