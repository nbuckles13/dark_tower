#!/bin/bash
#
# Guard Runner: Execute all simple (pattern-based) guards
#
# Semantic guards are handled by the semantic-guard agent during devloops,
# not by this script. See .claude/agents/semantic-guard.md.
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
#   --verbose         Show detailed output
#   --help            Show this help message
#
# Examples:
#   ./run-guards.sh                              # Run simple guards on entire repo
#   ./run-guards.sh crates/ac-service/src/       # Run on specific directory
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
VERBOSE=false
SEARCH_PATH=""

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --verbose)
            VERBOSE=true
            shift
            ;;
        --help)
            head -25 "$0" | tail -20
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
    # Note: In worktrees/clones, .git may be a file pointing to the main repo
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
# Run Validation Guards (infrastructure and application metrics)
# -----------------------------------------------------------------------------
echo ""
echo -e "${BOLD}Validation Guards${NC}"
echo "================="
echo ""

# Infrastructure metrics validation
INFRA_METRICS_GUARD="$SCRIPT_DIR/validate-infrastructure-metrics.sh"
if [[ -x "$INFRA_METRICS_GUARD" ]]; then
    GUARD_NAME="infrastructure-metrics"
    ((TOTAL_GUARDS++)) || true

    echo -e "${BLUE}Running:${NC} $GUARD_NAME"

    if $VERBOSE; then
        if "$INFRA_METRICS_GUARD"; then
            echo -e "${GREEN}PASSED${NC}: $GUARD_NAME"
            ((PASSED_GUARDS++)) || true
        else
            echo -e "${RED}FAILED${NC}: $GUARD_NAME"
            ((FAILED_GUARDS++)) || true
            FAILED_GUARD_NAMES+=("$GUARD_NAME")
        fi
    else
        if OUTPUT=$("$INFRA_METRICS_GUARD" 2>&1); then
            echo -e "${GREEN}PASSED${NC}: $GUARD_NAME"
            ((PASSED_GUARDS++)) || true
        else
            echo -e "${RED}FAILED${NC}: $GUARD_NAME"
            ((FAILED_GUARDS++)) || true
            FAILED_GUARD_NAMES+=("$GUARD_NAME")
            echo "$OUTPUT"
        fi
    fi
    echo ""
fi

# Application metrics validation
APP_METRICS_GUARD="$SCRIPT_DIR/validate-application-metrics.sh"
if [[ -x "$APP_METRICS_GUARD" ]]; then
    GUARD_NAME="application-metrics"
    ((TOTAL_GUARDS++)) || true

    echo -e "${BLUE}Running:${NC} $GUARD_NAME"

    if $VERBOSE; then
        if "$APP_METRICS_GUARD"; then
            echo -e "${GREEN}PASSED${NC}: $GUARD_NAME"
            ((PASSED_GUARDS++)) || true
        else
            echo -e "${RED}FAILED${NC}: $GUARD_NAME"
            ((FAILED_GUARDS++)) || true
            FAILED_GUARD_NAMES+=("$GUARD_NAME")
        fi
    else
        if OUTPUT=$("$APP_METRICS_GUARD" 2>&1); then
            echo -e "${GREEN}PASSED${NC}: $GUARD_NAME"
            ((PASSED_GUARDS++)) || true
        else
            echo -e "${RED}FAILED${NC}: $GUARD_NAME"
            ((FAILED_GUARDS++)) || true
            FAILED_GUARD_NAMES+=("$GUARD_NAME")
            echo "$OUTPUT"
        fi
    fi
    echo ""
fi

# NOTE: Semantic guards are now handled by the semantic-guard agent,
# spawned during devloop validation (see .claude/agents/semantic-guard.md)

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
