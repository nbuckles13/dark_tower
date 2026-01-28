#!/bin/bash
#
# Semantic Guard Runner
#
# Orchestrates diff-based semantic analysis using LLM.
# Much faster than per-file analysis - analyzes entire diff in one call.
#
# Exit codes:
#   0 - All checks passed (SAFE)
#   1 - Issues found (UNSAFE)
#   2 - Analysis unclear or no changes
#   3 - Script error
#
# Usage:
#   ./run-semantic-guards.sh                    # Analyze diff against HEAD
#   ./run-semantic-guards.sh --base main        # Analyze diff against main
#   ./run-semantic-guards.sh --check actor-blocking  # Run single check
#   ./run-semantic-guards.sh --verbose          # Show full analysis
#
# Environment:
#   GUARD_SEMANTIC_MODEL - Model to use (default: claude-sonnet-4-20250514)
#   GUARD_MAX_DIFF_SIZE  - Max diff size in bytes before chunking (default: 50000)
#

set -euo pipefail

# Script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"

# Configuration
MAX_DIFF_SIZE="${GUARD_MAX_DIFF_SIZE:-50000}"  # 50KB default

# Default options
BASE_REF="HEAD"
CHECK="all"
VERBOSE=false

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --base)
            BASE_REF="$2"
            shift 2
            ;;
        --check)
            CHECK="$2"
            shift 2
            ;;
        --verbose)
            VERBOSE=true
            shift
            ;;
        --help)
            head -30 "$0" | tail -25
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            exit 3
            ;;
    esac
done

start_timer

print_header "Semantic Guard Analysis
Base: $BASE_REF
Check: $CHECK"

# -----------------------------------------------------------------------------
# Step 1: Generate filtered diff
# -----------------------------------------------------------------------------
echo -e "${BLUE}Generating diff (excluding test files)...${NC}"

DIFF_FILE=$(mktemp)
trap "rm -f $DIFF_FILE" EXIT

"$SCRIPT_DIR/get-non-test-diff.sh" "$BASE_REF" > "$DIFF_FILE"

DIFF_SIZE=$(wc -c < "$DIFF_FILE")
DIFF_LINES=$(wc -l < "$DIFF_FILE")

echo "Diff size: $DIFF_SIZE bytes, $DIFF_LINES lines"
echo ""

# Check if there are any changes
if [[ $DIFF_SIZE -eq 0 ]]; then
    echo -e "${YELLOW}No changes to analyze${NC}"
    echo "Either there are no uncommitted changes, or all changes are in test files."
    exit 2
fi

# -----------------------------------------------------------------------------
# Step 2: Check size and chunk if needed
# -----------------------------------------------------------------------------
if [[ $DIFF_SIZE -gt $MAX_DIFF_SIZE ]]; then
    echo -e "${YELLOW}Diff is large ($DIFF_SIZE bytes > $MAX_DIFF_SIZE). Chunking not yet implemented.${NC}"
    echo "For now, analyzing first $MAX_DIFF_SIZE bytes."
    echo ""
    head -c "$MAX_DIFF_SIZE" "$DIFF_FILE" > "${DIFF_FILE}.chunk"
    mv "${DIFF_FILE}.chunk" "$DIFF_FILE"
fi

# -----------------------------------------------------------------------------
# Step 3: Run semantic analysis
# -----------------------------------------------------------------------------
echo -e "${BLUE}Running semantic analysis...${NC}"
echo ""

# Run the analyzer
RESULT_FILE=$(mktemp)
trap "rm -f $DIFF_FILE $RESULT_FILE" EXIT

if "$SCRIPT_DIR/semantic/analyze-diff.sh" "$DIFF_FILE" --check "$CHECK" > "$RESULT_FILE" 2>&1; then
    ANALYSIS_EXIT=0
else
    ANALYSIS_EXIT=$?
fi

# Show results
if $VERBOSE; then
    cat "$RESULT_FILE"
else
    # Show summary only
    grep -E '(^##|^SAFE|^UNSAFE|^UNCLEAR|FINDING|VERDICT)' "$RESULT_FILE" || cat "$RESULT_FILE"
fi

echo ""

# -----------------------------------------------------------------------------
# Summary
# -----------------------------------------------------------------------------
print_header "Summary"
print_elapsed_time
echo ""

case $ANALYSIS_EXIT in
    0)
        echo -e "${GREEN}All semantic checks passed (SAFE)${NC}"
        exit 0
        ;;
    1)
        echo -e "${RED}Issues found (UNSAFE)${NC}"
        echo "Review the findings above and address them before committing."
        exit 1
        ;;
    2)
        echo -e "${YELLOW}Analysis unclear - manual review recommended${NC}"
        exit 2
        ;;
    *)
        echo -e "${RED}Analysis failed with exit code $ANALYSIS_EXIT${NC}"
        exit 3
        ;;
esac
