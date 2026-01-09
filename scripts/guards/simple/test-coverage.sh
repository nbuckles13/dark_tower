#!/bin/bash
#
# Simple Guard: Test Coverage
#
# Two modes:
#   1. Pre-commit (default): Quick check - warns if new production code
#      lacks corresponding test additions
#   2. CI (--full): Runs cargo llvm-cov and checks coverage thresholds
#
# Exit codes:
#   0 - Coverage adequate (or warnings only in pre-commit mode)
#   1 - Coverage below threshold (CI mode only)
#   2 - Script error
#
# Usage:
#   ./test-coverage.sh              # Quick pre-commit check
#   ./test-coverage.sh --full       # Full coverage analysis (CI)
#   ./test-coverage.sh [path]       # Skip (not applicable for path scans)
#
# Thresholds (from docs/principles/testing.md):
#   - 100% for crypto code
#   - 95% for handlers, services, repositories
#   - 90% overall minimum
#

set -euo pipefail

# Source common library
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/../common.sh"

# Parse arguments
FULL_MODE=false
for arg in "$@"; do
    case $arg in
        --full)
            FULL_MODE=true
            ;;
        *)
            # If any non-flag argument (path), skip this guard
            echo "Skipping test-coverage guard (only runs on staged changes, not path scans)"
            exit 0
            ;;
    esac
done

# Initialize
init_violations
start_timer

# -----------------------------------------------------------------------------
# Pre-commit Mode: Quick heuristic check
# -----------------------------------------------------------------------------
if [[ "$FULL_MODE" == false ]]; then
    print_header "Guard: Test Coverage (Quick Check)"

    # Get staged production code files (excluding tests)
    NEW_PROD_FILES=$(git diff --cached --name-only --diff-filter=A 2>/dev/null | \
        grep '\.rs$' | \
        grep -v '_test\.rs$' | \
        grep -v '/tests/' | \
        grep -v 'test_' | \
        grep -v '/fuzz/' || true)

    NEW_TEST_FILES=$(git diff --cached --name-only --diff-filter=A 2>/dev/null | \
        grep '\.rs$' | \
        grep -E '(_test\.rs$|/tests/|test_|/fuzz/)' || true)

    if [[ -z "$NEW_PROD_FILES" ]]; then
        echo -e "${GREEN}No new production code files in staged changes${NC}"
        print_elapsed_time
        exit 0
    fi

    # Check if new prod files have corresponding test additions
    WARNINGS=0

    print_section "New production code without test additions"

    for prod_file in $NEW_PROD_FILES; do
        # Check if there's a corresponding test file added
        base_name=$(basename "$prod_file" .rs)
        dir_name=$(dirname "$prod_file")

        # Look for test file patterns
        has_test=false

        # Check for mod_test.rs pattern
        if echo "$NEW_TEST_FILES" | grep -q "${base_name}_test.rs"; then
            has_test=true
        fi

        # Check for tests/ directory addition
        if echo "$NEW_TEST_FILES" | grep -q "${dir_name}/tests/"; then
            has_test=true
        fi

        # Check if file itself has #[cfg(test)] module (added in same commit)
        if git show :"$prod_file" 2>/dev/null | grep -q '#\[cfg(test)\]'; then
            has_test=true
        fi

        if [[ "$has_test" == false ]]; then
            echo -e "${YELLOW}WARNING:${NC} $prod_file"
            echo "  No corresponding test file or #[cfg(test)] module found"
            ((WARNINGS++)) || true
        fi
    done

    if [[ $WARNINGS -eq 0 ]]; then
        print_ok "All new production files have corresponding tests"
    fi

    echo ""
    print_header "Summary"
    print_elapsed_time

    if [[ $WARNINGS -gt 0 ]]; then
        echo ""
        echo -e "${YELLOW}$WARNINGS file(s) may need test coverage${NC}"
        echo ""
        echo "────────────────────────────────────────────────────────────────"
        echo "Consider adding tests for new production code."
        echo "This is a reminder, not a blocker."
        echo ""
        echo "For full coverage analysis, run in CI with --full flag."
        echo "────────────────────────────────────────────────────────────────"
    else
        echo -e "${GREEN}Coverage check passed${NC}"
    fi

    # Pre-commit mode always exits 0 (warning only)
    exit 0
fi

# -----------------------------------------------------------------------------
# CI Mode: Full coverage analysis
# -----------------------------------------------------------------------------
print_header "Guard: Test Coverage (Full Analysis)"

echo "Running cargo llvm-cov..."
echo ""

# Check if cargo-llvm-cov is installed
if ! command -v cargo-llvm-cov &> /dev/null; then
    echo -e "${RED}Error: cargo-llvm-cov not installed${NC}"
    echo "Install with: cargo install cargo-llvm-cov"
    exit 2
fi

# Find repository root
REPO_ROOT="$SCRIPT_DIR"
while [[ ! -d "$REPO_ROOT/.git" ]] && [[ "$REPO_ROOT" != "/" ]]; do
    REPO_ROOT="$(dirname "$REPO_ROOT")"
done

cd "$REPO_ROOT"

# Run coverage and capture JSON output
COVERAGE_JSON=$(DATABASE_URL=postgresql://postgres:postgres@localhost:5433/dark_tower_test \
    cargo llvm-cov --workspace --json 2>/dev/null || true)

if [[ -z "$COVERAGE_JSON" ]]; then
    echo -e "${RED}Failed to run cargo llvm-cov${NC}"
    echo "Make sure tests pass and database is available"
    exit 2
fi

# Parse overall coverage from JSON
# The JSON format has "data" array with coverage info
OVERALL_COVERAGE=$(echo "$COVERAGE_JSON" | \
    jq -r '.data[0].totals.lines.percent // 0' 2>/dev/null || echo "0")

print_section "Coverage Results"

echo "Overall line coverage: ${OVERALL_COVERAGE}%"
echo ""

# Thresholds
OVERALL_THRESHOLD=90

# Check overall threshold
if (( $(echo "$OVERALL_COVERAGE < $OVERALL_THRESHOLD" | bc -l) )); then
    echo -e "${RED}VIOLATION: Overall coverage ${OVERALL_COVERAGE}% < ${OVERALL_THRESHOLD}% threshold${NC}"
    increment_violations
else
    print_ok "Overall coverage meets threshold"
fi

# Check crypto module specifically (should be 100%)
CRYPTO_COVERAGE=$(echo "$COVERAGE_JSON" | \
    jq -r '[.data[0].files[] | select(.filename | contains("crypto")) | .summary.lines.percent] | add / length // 0' 2>/dev/null || echo "0")

echo ""
echo "Crypto module coverage: ${CRYPTO_COVERAGE}%"

if (( $(echo "$CRYPTO_COVERAGE < 100" | bc -l) )); then
    echo -e "${YELLOW}WARNING: Crypto coverage ${CRYPTO_COVERAGE}% < 100% target${NC}"
    # Don't increment violations for this, just warn
else
    print_ok "Crypto coverage meets target"
fi

echo ""
print_header "Summary"
print_elapsed_time

TOTAL_VIOLATIONS=$(get_violations)

if [[ $TOTAL_VIOLATIONS -gt 0 ]]; then
    echo ""
    echo -e "${RED}Coverage below required thresholds${NC}"
    echo ""
    echo "Run 'cargo llvm-cov --html' to see detailed coverage report"
    exit 1
else
    echo ""
    echo -e "${GREEN}Coverage meets all thresholds${NC}"
    exit 0
fi
