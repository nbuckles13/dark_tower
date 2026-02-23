#!/bin/bash
#
# Simple Guard: No Test Removal
#
# Detects when tests are removed or weakened compared to HEAD.
# This is a WARNING guard - it doesn't block but surfaces test changes for review.
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
#   ./no-test-removal.sh [path]   # Check path against HEAD (default: .)
#

set -euo pipefail

# Source common library
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/../common.sh"

# Default to current directory
SEARCH_PATH="${1:-.}"

# Initialize
init_violations
start_timer

DIFF_BASE=$(get_diff_base)

print_header "Guard: Test Modification Check
Path: $SEARCH_PATH

Compares working tree against ${DIFF_BASE} to detect test weakening."

# Track warnings (separate from violations since this is non-blocking)
WARNINGS=0

# Get changed Rust files using common helpers
MODIFIED_FILES=$(get_modified_files "$SEARCH_PATH" ".rs")
DELETED_FILES=$(get_deleted_files "$SEARCH_PATH" ".rs")

if [[ -z "$MODIFIED_FILES" && -z "$DELETED_FILES" ]]; then
    echo -e "${GREEN}No Rust files modified compared to ${DIFF_BASE}${NC}"
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
for file in $MODIFIED_FILES; do
    # Skip non-existent files (deleted)
    [[ ! -f "$file" ]] && continue

    # Count #[test] and #[sqlx::test] in HEAD vs working tree
    head_count=$(git show "$DIFF_BASE":"$file" 2>/dev/null | grep -cE '#\[(test|sqlx::test|tokio::test)' 2>/dev/null || true)
    head_count=${head_count:-0}
    current_count=$(grep -cE '#\[(test|sqlx::test|tokio::test)' "$file" 2>/dev/null || true)
    current_count=${current_count:-0}

    if [[ "$current_count" -lt "$head_count" ]]; then
        if [[ "$test_changes_found" == false ]]; then
            echo -e "${YELLOW}WARNING: Test functions removed:${NC}"
            test_changes_found=true
        fi
        removed=$((head_count - current_count))
        echo "  $file: $removed test(s) removed (was $head_count, now $current_count)"
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
for file in $MODIFIED_FILES; do
    # Skip non-existent files
    [[ ! -f "$file" ]] && continue

    # Only check files that contain tests
    current_has_tests=$(grep -cE '#\[(test|sqlx::test|tokio::test)' "$file" 2>/dev/null || true)
    current_has_tests=${current_has_tests:-0}
    [[ "$current_has_tests" -eq 0 ]] && continue

    # Count assertions in HEAD vs working tree
    head_count=$(git show "$DIFF_BASE":"$file" 2>/dev/null | grep -cE '\bassert(_eq|_ne|_matches)?!' 2>/dev/null || true)
    head_count=${head_count:-0}
    current_count=$(grep -cE '\bassert(_eq|_ne|_matches)?!' "$file" 2>/dev/null || true)
    current_count=${current_count:-0}

    # Only warn if significant reduction (more than 2 assertions removed)
    if [[ "$current_count" -lt "$head_count" ]]; then
        removed=$((head_count - current_count))
        if [[ "$removed" -gt 2 ]]; then
            if [[ "$assertion_changes_found" == false ]]; then
                echo -e "${YELLOW}WARNING: Assertions removed (>2):${NC}"
                assertion_changes_found=true
            fi
            echo "  $file: $removed assertion(s) removed (was $head_count, now $current_count)"
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
for file in $MODIFIED_FILES; do
    # Skip non-existent files
    [[ ! -f "$file" ]] && continue

    # Count #[ignore] in HEAD vs working tree
    head_count=$(git show "$DIFF_BASE":"$file" 2>/dev/null | grep -cE '#\[ignore' 2>/dev/null || true)
    head_count=${head_count:-0}
    current_count=$(grep -cE '#\[ignore' "$file" 2>/dev/null || true)
    current_count=${current_count:-0}

    if [[ "$current_count" -gt "$head_count" ]]; then
        if [[ "$ignore_changes_found" == false ]]; then
            echo -e "${YELLOW}WARNING: #[ignore] added to tests:${NC}"
            ignore_changes_found=true
        fi
        added=$((current_count - head_count))
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
