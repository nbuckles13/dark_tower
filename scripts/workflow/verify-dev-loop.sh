#!/usr/bin/env bash
# Dev-loop verification script
# Extracted from .claude/workflows/development-loop.md

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Default values
OUTPUT_DIR=""
VERBOSE=false
FORMAT="text"

usage() {
    echo "Usage: $0 [OPTIONS]"
    echo ""
    echo "Options:"
    echo "  --output-dir DIR   Path to dev-loop output directory"
    echo "  --verbose          Show detailed output"
    echo "  --format FORMAT    Output format: text (default) or json"
    echo "  -h, --help         Show this help message"
    echo ""
    echo "Examples:"
    echo "  $0 --output-dir docs/dev-loop-outputs/2026-01-15-gc-meeting-api"
    echo "  $0 --output-dir docs/dev-loop-outputs/2026-01-15-gc-meeting-api --verbose"
}

log_pass() {
    if [ "$FORMAT" = "json" ]; then
        echo "{\"check\": \"$1\", \"status\": \"pass\"}"
    else
        echo -e "${GREEN}✓${NC} $1"
    fi
}

log_fail() {
    if [ "$FORMAT" = "json" ]; then
        echo "{\"check\": \"$1\", \"status\": \"fail\", \"message\": \"$2\"}"
    else
        echo -e "${RED}✗${NC} $1: $2"
    fi
}

log_warn() {
    if [ "$FORMAT" = "json" ]; then
        echo "{\"check\": \"$1\", \"status\": \"warn\", \"message\": \"$2\"}"
    else
        echo -e "${YELLOW}!${NC} $1: $2"
    fi
}

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --output-dir)
            OUTPUT_DIR="$2"
            shift 2
            ;;
        --verbose)
            VERBOSE=true
            shift
            ;;
        --format)
            FORMAT="$2"
            shift 2
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            usage
            exit 1
            ;;
    esac
done

if [ -z "$OUTPUT_DIR" ]; then
    echo "Error: --output-dir is required"
    usage
    exit 1
fi

MAIN_FILE="$OUTPUT_DIR/main.md"
ERRORS=0

# Check output directory exists
if [ -d "$OUTPUT_DIR" ]; then
    log_pass "Output directory exists"
else
    log_fail "Output directory exists" "Directory not found: $OUTPUT_DIR"
    exit 1
fi

# Check main.md exists
if [ -f "$MAIN_FILE" ]; then
    log_pass "main.md exists"
else
    log_fail "main.md exists" "File not found: $MAIN_FILE"
    exit 1
fi

# Check required sections in main.md
REQUIRED_SECTIONS=(
    "## Task Overview"
    "## Implementation Summary"
    "## Dev-Loop Verification Steps"
)

for section in "${REQUIRED_SECTIONS[@]}"; do
    if grep -q "$section" "$MAIN_FILE"; then
        log_pass "Section present: $section"
    else
        log_fail "Section present: $section" "Missing section"
        ((ERRORS++))
    fi
done

# Check for placeholder content
PLACEHOLDERS=("TBD" "TODO" "PLACEHOLDER")
for placeholder in "${PLACEHOLDERS[@]}"; do
    count=$(grep -c "$placeholder" "$MAIN_FILE" 2>/dev/null || true)
    if [ "$count" -gt 0 ]; then
        log_warn "Placeholder check" "Found $count instances of '$placeholder'"
        if [ "$VERBOSE" = true ]; then
            grep -n "$placeholder" "$MAIN_FILE" | head -5
        fi
    fi
done

# Check Implementation Summary has content (not just header)
if awk '/## Implementation Summary/{found=1} found && /^\|/{has_content=1} /^##/ && found && !/## Implementation Summary/{exit} END{exit !has_content}' "$MAIN_FILE"; then
    log_pass "Implementation Summary has content"
else
    log_fail "Implementation Summary has content" "Section appears empty (no tables/lists)"
    ((ERRORS++))
fi

# Check Loop State exists and is populated
if grep -q "## Loop State" "$MAIN_FILE"; then
    log_pass "Loop State section exists"

    # Check Current Step is not TBD
    if grep -A10 "## Loop State" "$MAIN_FILE" | grep -q "Current Step.*complete"; then
        log_pass "Loop State shows complete"
    elif grep -A10 "## Loop State" "$MAIN_FILE" | grep -q "Current Step.*TBD"; then
        log_fail "Loop State populated" "Current Step is TBD"
        ((ERRORS++))
    fi
else
    log_warn "Loop State section exists" "Section not found (may be older format)"
fi

# Check for specialist checkpoint file
SPECIALIST_FILES=$(find "$OUTPUT_DIR" -name "*.md" ! -name "main.md" 2>/dev/null | wc -l)
if [ "$SPECIALIST_FILES" -gt 0 ]; then
    log_pass "Specialist checkpoint files exist ($SPECIALIST_FILES found)"
else
    log_warn "Specialist checkpoint files" "No specialist checkpoint files found"
fi

# Summary
echo ""
if [ "$ERRORS" -gt 0 ]; then
    echo -e "${RED}Verification failed with $ERRORS error(s)${NC}"
    exit 1
else
    echo -e "${GREEN}Verification passed${NC}"
    exit 0
fi
