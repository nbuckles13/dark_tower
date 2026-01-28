#!/bin/bash
#
# Simple Guard: No Error Hiding
#
# Detects patterns where error context is discarded, such as:
#   .map_err(|_| SomeError)
#   .map_err(|_e| SomeError)  // unused binding
#
# Preserving error context is critical for debugging. Instead use:
#   .map_err(|e| SomeError::Internal(e.to_string()))
#   .map_err(SomeError::from)
#
# Exit codes:
#   0 - No violations found
#   1 - Violations found
#   2 - Script error
#
# Usage:
#   ./no-error-hiding.sh [path]
#   ./no-error-hiding.sh crates/ac-service/src/
#

set -euo pipefail

# Source common library
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/../common.sh"

# Default to current directory if no path provided
SEARCH_PATH="${1:-.}"

# Initialize
check_nightly_required
init_violations
start_timer

print_header "Guard: No Error Hiding
Path: $SEARCH_PATH"

# -----------------------------------------------------------------------------
# Check 1: map_err with underscore (discards error)
# -----------------------------------------------------------------------------
print_section "Check 1: map_err with underscore pattern"

# Pattern: .map_err(|_| ...) or .map_err(|_e| ...) or .map_err(|_err| ...)
# The underscore (with or without name) means the error is being discarded
error_hiding_violations=$(grep -rn --include="*.rs" \
    '\.map_err(|_' \
    "$SEARCH_PATH" 2>/dev/null | \
    filter_test_code || true)

if [[ -n "$error_hiding_violations" ]]; then
    echo -e "${RED}VIOLATIONS FOUND:${NC}"
    echo "$error_hiding_violations" | while read -r line; do
        echo "  $line"
        increment_violations
    done
    echo ""
else
    print_ok "No map_err with underscore patterns"
    echo ""
fi

# -----------------------------------------------------------------------------
# Check 2: ok_or with generic error (less severe, but worth noting)
# -----------------------------------------------------------------------------
print_section "Check 2: ok_or patterns (informational)"

# Pattern: .ok_or(SomeError::...) without context
# This is less severe than map_err(|_|) because there's no error to preserve,
# but it's worth noting when the error message is generic
ok_or_generic=$(grep -rn --include="*.rs" \
    '\.ok_or(.*Internal\s*[,)]' \
    "$SEARCH_PATH" 2>/dev/null | \
    filter_test_code || true)

if [[ -n "$ok_or_generic" ]]; then
    echo -e "${YELLOW}INFORMATIONAL (consider adding context):${NC}"
    echo "$ok_or_generic" | while read -r line; do
        echo "  $line"
        # Don't increment - this is informational only
    done
    echo ""
else
    print_ok "No generic ok_or patterns"
    echo ""
fi

# -----------------------------------------------------------------------------
# Summary
# -----------------------------------------------------------------------------
print_header "Summary"

TOTAL_VIOLATIONS=$(get_violations)
print_elapsed_time
echo ""

if [[ $TOTAL_VIOLATIONS -gt 0 ]]; then
    echo -e "${RED}Found $TOTAL_VIOLATIONS violation(s)${NC}"
    echo ""
    echo "Error context is critical for debugging. Instead of:"
    echo "  .map_err(|_| SomeError::Internal)"
    echo ""
    echo "Use one of these patterns to preserve context:"
    echo "  .map_err(|e| SomeError::Internal(e.to_string()))"
    echo "  .map_err(|e| SomeError::Internal(format!(\"context: {}\", e)))"
    echo "  .map_err(SomeError::from)  // if From is implemented"
    echo ""
    exit 1
else
    echo -e "${GREEN}No violations found!${NC}"
    exit 0
fi
