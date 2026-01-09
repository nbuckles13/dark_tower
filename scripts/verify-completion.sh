#!/bin/bash
#
# Verification Script for Development Loop
#
# Runs layered verification checks and outputs structured failure reports.
# Used by the orchestrator to determine if specialist work is complete.
#
# Exit codes:
#   0 - All checks passed
#   1 - One or more checks failed (see report)
#   2 - Script error
#
# Usage:
#   ./verify-completion.sh [options] [path]
#
# Options:
#   --layer LEVEL    Verification level: quick, standard, full (default: full)
#   --format FORMAT  Output format: text, json (default: text)
#   --verbose        Show detailed output from each check
#   --help           Show this help message
#
# Layers:
#   quick    - cargo check + simple guards (~7s)
#   standard - quick + unit tests (~40s)
#   full     - standard + all tests + clippy (~2-5min)
#
# Examples:
#   ./verify-completion.sh                    # Full verification
#   ./verify-completion.sh --layer quick      # Fast feedback
#   ./verify-completion.sh --format json      # Machine-readable output
#

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Set DATABASE_URL for tests if not already set
export DATABASE_URL="${DATABASE_URL:-postgresql://postgres:postgres@localhost:5433/dark_tower_test}"

# Colors
RED='\033[0;31m'
YELLOW='\033[1;33m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
BOLD='\033[1m'
NC='\033[0m'

# Default options
LAYER="full"
FORMAT="text"
VERBOSE=false
SEARCH_PATH="$REPO_ROOT"

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --layer)
            LAYER="$2"
            shift 2
            ;;
        --format)
            FORMAT="$2"
            shift 2
            ;;
        --verbose)
            VERBOSE=true
            shift
            ;;
        --help)
            head -35 "$0" | tail -30
            exit 0
            ;;
        -*)
            echo "Unknown option: $1" >&2
            exit 2
            ;;
        *)
            SEARCH_PATH="$1"
            shift
            ;;
    esac
done

# Validate layer
if [[ ! "$LAYER" =~ ^(quick|standard|full)$ ]]; then
    echo "Invalid layer: $LAYER (must be quick, standard, or full)" >&2
    exit 2
fi

# Track failures
declare -a FAILURES=()
LAYER_FAILED=""

# Timer
START_TIME=$(date +%s.%N)

# -----------------------------------------------------------------------------
# Helper Functions
# -----------------------------------------------------------------------------

add_failure() {
    local type="$1"
    local name="$2"
    local message="$3"
    local hint="${4:-}"

    FAILURES+=("$type|$name|$message|$hint")
    if [[ -z "$LAYER_FAILED" ]]; then
        LAYER_FAILED="$name"
    fi
}

run_check() {
    local name="$1"
    local command="$2"
    local hint="$3"

    if $VERBOSE; then
        echo -e "${BLUE}Running:${NC} $name"
    fi

    local output
    local exit_code=0
    output=$($command 2>&1) || exit_code=$?

    if [[ $exit_code -ne 0 ]]; then
        add_failure "check" "$name" "$output" "$hint"
        return 1
    fi

    if $VERBOSE; then
        echo -e "${GREEN}PASSED${NC}: $name"
    fi
    return 0
}

# -----------------------------------------------------------------------------
# Verification Layers
# -----------------------------------------------------------------------------

verify_compile() {
    if $VERBOSE; then
        echo -e "\n${BOLD}Layer 1: Compile Check${NC}"
        echo "─────────────────────────"
    fi

    run_check "cargo-check" \
        "cargo check --workspace --quiet" \
        "Fix compilation errors before proceeding"
}

verify_guards() {
    if $VERBOSE; then
        echo -e "\n${BOLD}Layer 2: Simple Guards${NC}"
        echo "─────────────────────────"
    fi

    local guard_output
    local exit_code=0
    guard_output=$("$SCRIPT_DIR/guards/run-guards.sh" --simple-only "$SEARCH_PATH" 2>&1) || exit_code=$?

    if [[ $exit_code -ne 0 ]]; then
        # Extract violation details from guard output
        local violations
        violations=$(echo "$guard_output" | grep -E "(VIOLATION|violation|Failed guards:)" | head -20 || true)
        add_failure "guard" "simple-guards" "$violations" "Review guard output and fix violations"
        return 1
    fi

    if $VERBOSE; then
        echo -e "${GREEN}PASSED${NC}: simple-guards"
    fi
    return 0
}

verify_unit_tests() {
    if $VERBOSE; then
        echo -e "\n${BOLD}Layer 3: Unit Tests${NC}"
        echo "─────────────────────────"
    fi

    run_check "unit-tests" \
        "cargo test --workspace --lib --quiet" \
        "Fix failing unit tests"
}

verify_all_tests() {
    if $VERBOSE; then
        echo -e "\n${BOLD}Layer 4: All Tests${NC}"
        echo "─────────────────────────"
    fi

    run_check "all-tests" \
        "cargo test --workspace --quiet" \
        "Fix failing integration tests"
}

verify_clippy() {
    if $VERBOSE; then
        echo -e "\n${BOLD}Layer 5: Clippy${NC}"
        echo "─────────────────────────"
    fi

    run_check "clippy" \
        "cargo clippy --workspace --quiet -- -D warnings" \
        "Fix clippy warnings"
}

# -----------------------------------------------------------------------------
# Output Functions
# -----------------------------------------------------------------------------

output_text() {
    local elapsed
    elapsed=$(echo "$(date +%s.%N) - $START_TIME" | bc)

    echo ""
    echo -e "${BOLD}=========================================="
    echo "Verification Report"
    echo "==========================================${NC}"
    echo ""
    echo "Layer: $LAYER"
    printf "Elapsed: %.2f seconds\n" "$elapsed"
    echo ""

    if [[ ${#FAILURES[@]} -eq 0 ]]; then
        echo -e "${GREEN}All checks passed!${NC}"
        echo ""
        return 0
    fi

    echo -e "${RED}Verification Failed${NC}"
    echo ""
    echo "Failed at: $LAYER_FAILED"
    echo ""

    for failure in "${FAILURES[@]}"; do
        IFS='|' read -r type name message hint <<< "$failure"

        echo -e "${RED}### $name${NC}"
        echo ""
        # Truncate long messages
        if [[ ${#message} -gt 2000 ]]; then
            echo "${message:0:2000}..."
            echo "(output truncated)"
        else
            echo "$message"
        fi
        echo ""
        if [[ -n "$hint" ]]; then
            echo -e "${YELLOW}Hint:${NC} $hint"
            echo ""
        fi
    done

    return 1
}

output_json() {
    local elapsed
    elapsed=$(echo "$(date +%s.%N) - $START_TIME" | bc)

    local passed="true"
    if [[ ${#FAILURES[@]} -gt 0 ]]; then
        passed="false"
    fi

    echo "{"
    echo "  \"passed\": $passed,"
    echo "  \"layer\": \"$LAYER\","
    printf "  \"elapsed_seconds\": %.2f,\n" "$elapsed"
    echo "  \"layer_failed\": \"${LAYER_FAILED:-null}\","
    echo "  \"failures\": ["

    local first=true
    for failure in "${FAILURES[@]}"; do
        IFS='|' read -r type name message hint <<< "$failure"

        if [[ "$first" != "true" ]]; then
            echo ","
        fi
        first=false

        # Escape JSON strings
        message="${message//\\/\\\\}"
        message="${message//\"/\\\"}"
        message="${message//$'\n'/\\n}"
        message="${message//$'\r'/}"
        hint="${hint//\\/\\\\}"
        hint="${hint//\"/\\\"}"

        echo "    {"
        echo "      \"type\": \"$type\","
        echo "      \"name\": \"$name\","
        echo "      \"message\": \"${message:0:2000}\","
        echo "      \"hint\": \"$hint\""
        echo -n "    }"
    done

    echo ""
    echo "  ]"
    echo "}"
}

# -----------------------------------------------------------------------------
# Main
# -----------------------------------------------------------------------------

main() {
    if $VERBOSE; then
        echo -e "${BOLD}=========================================="
        echo "Verification: $LAYER layer"
        echo "==========================================${NC}"
        echo ""
        echo "Path: $SEARCH_PATH"
    fi

    # Layer 1: Compile (always)
    verify_compile || true

    # Layer 2: Guards (always)
    if [[ ${#FAILURES[@]} -eq 0 ]]; then
        verify_guards || true
    fi

    # Stop here for quick layer
    if [[ "$LAYER" == "quick" ]]; then
        if [[ "$FORMAT" == "json" ]]; then
            output_json
        else
            output_text
        fi
        [[ ${#FAILURES[@]} -eq 0 ]] && exit 0 || exit 1
    fi

    # Layer 3: Unit tests (standard+)
    if [[ ${#FAILURES[@]} -eq 0 ]]; then
        verify_unit_tests || true
    fi

    # Stop here for standard layer
    if [[ "$LAYER" == "standard" ]]; then
        if [[ "$FORMAT" == "json" ]]; then
            output_json
        else
            output_text
        fi
        [[ ${#FAILURES[@]} -eq 0 ]] && exit 0 || exit 1
    fi

    # Layer 4: All tests (full only)
    if [[ ${#FAILURES[@]} -eq 0 ]]; then
        verify_all_tests || true
    fi

    # Layer 5: Clippy (full only)
    if [[ ${#FAILURES[@]} -eq 0 ]]; then
        verify_clippy || true
    fi

    # Output results
    if [[ "$FORMAT" == "json" ]]; then
        output_json
    else
        output_text
    fi

    [[ ${#FAILURES[@]} -eq 0 ]] && exit 0 || exit 1
}

main
