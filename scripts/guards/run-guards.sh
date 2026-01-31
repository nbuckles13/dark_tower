#!/bin/bash
#
# Guard Runner: Execute all guards in the pipeline
#
# Runs simple guards first (fast), then optionally semantic guards (slower).
# Designed for CI integration and local development.
#
# Exit codes:
#   0 - All guards passed
#   1 - One or more guards failed
#   2 - Script error
#
# Usage:
#   ./run-guards.sh [options] [path]
#
# Options:
#   --simple-only     Only run simple (grep-based) guards
#   --semantic        Also run semantic (LLM-based) guards
#   --verbose         Show detailed output
#   --help            Show this help message
#
# Examples:
#   ./run-guards.sh                              # Run simple guards on entire repo
#   ./run-guards.sh crates/ac-service/src/       # Run on specific directory
#   ./run-guards.sh --semantic src/auth.rs       # Run with semantic analysis
#

set -euo pipefail

# Script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Source common library for helper functions
source "$SCRIPT_DIR/common.sh"

# Colors (already defined in common.sh, but keep for standalone use)
RED='\033[0;31m'
YELLOW='\033[1;33m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
BOLD='\033[1m'
NC='\033[0m'

# Default options
RUN_SEMANTIC=false
VERBOSE=false
SEARCH_PATH=""

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --simple-only)
            RUN_SEMANTIC=false
            shift
            ;;
        --semantic)
            RUN_SEMANTIC=true
            shift
            ;;
        --verbose)
            VERBOSE=true
            shift
            ;;
        --help)
            head -30 "$0" | tail -25
            exit 0
            ;;
        -*)
            echo "Unknown option: $1"
            exit 2
            ;;
        *)
            SEARCH_PATH="$1"
            shift
            ;;
    esac
done

# Default to repository root
if [[ -z "$SEARCH_PATH" ]]; then
    # Find repository root (go up until we find .git directory or file)
    # Note: In worktrees, .git is a file pointing to the main repo
    REPO_ROOT="$SCRIPT_DIR"
    while [[ ! -d "$REPO_ROOT/.git" ]] && [[ ! -f "$REPO_ROOT/.git" ]] && [[ "$REPO_ROOT" != "/" ]]; do
        REPO_ROOT="$(dirname "$REPO_ROOT")"
    done
    if [[ "$REPO_ROOT" == "/" ]]; then
        echo "Error: Could not find repository root (.git directory or file)"
        exit 2
    fi
    SEARCH_PATH="$REPO_ROOT"
fi

echo -e "${BOLD}=========================================="
echo "Guard Pipeline Runner"
echo "==========================================${NC}"
echo ""
echo "Path: $SEARCH_PATH"
echo "Semantic: $RUN_SEMANTIC"
echo ""

# Track results
TOTAL_GUARDS=0
PASSED_GUARDS=0
FAILED_GUARDS=0
declare -a FAILED_GUARD_NAMES

# Timer
START_TIME=$(date +%s.%N)

# -----------------------------------------------------------------------------
# Run Simple Guards
# -----------------------------------------------------------------------------
echo -e "${BOLD}Simple Guards${NC}"
echo "============="
echo ""

# Find all simple guards
SIMPLE_GUARDS_DIR="$SCRIPT_DIR/simple"
if [[ -d "$SIMPLE_GUARDS_DIR" ]]; then
    for guard in "$SIMPLE_GUARDS_DIR"/*.sh; do
        if [[ -x "$guard" ]]; then
            GUARD_NAME=$(basename "$guard" .sh)
            ((TOTAL_GUARDS++)) || true

            echo -e "${BLUE}Running:${NC} $GUARD_NAME"

            if $VERBOSE; then
                if "$guard" "$SEARCH_PATH"; then
                    echo -e "${GREEN}PASSED${NC}: $GUARD_NAME"
                    ((PASSED_GUARDS++)) || true
                else
                    echo -e "${RED}FAILED${NC}: $GUARD_NAME"
                    ((FAILED_GUARDS++)) || true
                    FAILED_GUARD_NAMES+=("$GUARD_NAME")
                fi
            else
                # Capture output for non-verbose mode
                if OUTPUT=$("$guard" "$SEARCH_PATH" 2>&1); then
                    echo -e "${GREEN}PASSED${NC}: $GUARD_NAME"
                    ((PASSED_GUARDS++)) || true
                else
                    echo -e "${RED}FAILED${NC}: $GUARD_NAME"
                    ((FAILED_GUARDS++)) || true
                    FAILED_GUARD_NAMES+=("$GUARD_NAME")
                    # Show failure details
                    echo "$OUTPUT" | grep -E "(VIOLATION|violation)" | head -5
                fi
            fi
            echo ""
        fi
    done
else
    echo "No simple guards found in $SIMPLE_GUARDS_DIR"
fi

# -----------------------------------------------------------------------------
# Run Semantic Guards (if enabled)
# -----------------------------------------------------------------------------
if $RUN_SEMANTIC; then
    echo -e "${BOLD}Semantic Guards${NC}"
    echo "==============="
    echo ""

    SEMANTIC_RUNNER="$SCRIPT_DIR/run-semantic-guards.sh"
    if [[ -x "$SEMANTIC_RUNNER" ]]; then
        ((TOTAL_GUARDS++)) || true

        echo -e "${BLUE}Running:${NC} semantic-analysis (diff-based)"

        if $VERBOSE; then
            if "$SEMANTIC_RUNNER" --verbose; then
                echo -e "${GREEN}PASSED${NC}: semantic-analysis"
                ((PASSED_GUARDS++)) || true
            else
                SEMANTIC_EXIT=$?
                if [[ $SEMANTIC_EXIT -eq 2 ]]; then
                    # UNCLEAR - treat as warning, not failure
                    echo -e "${YELLOW}UNCLEAR${NC}: semantic-analysis (manual review recommended)"
                    ((PASSED_GUARDS++)) || true
                else
                    echo -e "${RED}FAILED${NC}: semantic-analysis"
                    ((FAILED_GUARDS++)) || true
                    FAILED_GUARD_NAMES+=("semantic-analysis")
                fi
            fi
        else
            if OUTPUT=$("$SEMANTIC_RUNNER" 2>&1); then
                echo -e "${GREEN}PASSED${NC}: semantic-analysis"
                ((PASSED_GUARDS++)) || true
            else
                SEMANTIC_EXIT=$?
                if [[ $SEMANTIC_EXIT -eq 2 ]]; then
                    echo -e "${YELLOW}UNCLEAR${NC}: semantic-analysis (manual review recommended)"
                    ((PASSED_GUARDS++)) || true
                else
                    echo -e "${RED}FAILED${NC}: semantic-analysis"
                    ((FAILED_GUARDS++)) || true
                    FAILED_GUARD_NAMES+=("semantic-analysis")
                    # Show failure details
                    echo "$OUTPUT" | grep -E "(FINDING|UNSAFE|VERDICT)" | head -10
                fi
            fi
        fi
        echo ""
    else
        echo -e "${YELLOW}Semantic runner not found at $SEMANTIC_RUNNER${NC}"
        echo ""
    fi
fi

# -----------------------------------------------------------------------------
# Summary
# -----------------------------------------------------------------------------
END_TIME=$(date +%s.%N)
ELAPSED=$(echo "$END_TIME - $START_TIME" | bc)

echo -e "${BOLD}=========================================="
echo "Summary"
echo "==========================================${NC}"
echo ""
echo "Total guards run: $TOTAL_GUARDS"
echo -e "Passed: ${GREEN}$PASSED_GUARDS${NC}"
echo -e "Failed: ${RED}$FAILED_GUARDS${NC}"
printf "Elapsed time: %.2f seconds\n" "$ELAPSED"
echo ""

if [[ $FAILED_GUARDS -gt 0 ]]; then
    echo -e "${RED}Failed guards:${NC}"
    for failed in "${FAILED_GUARD_NAMES[@]}"; do
        echo "  - $failed"
    done
    echo ""
    echo "Run with --verbose for detailed output"
    exit 1
else
    echo -e "${GREEN}All guards passed!${NC}"
    exit 0
fi
