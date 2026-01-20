#!/bin/bash
#
# Simple Guard: No #[allow(...)] Attributes
#
# Production code should use #[expect(...)] instead of #[allow(...)].
# The expect attribute causes a compiler warning if the expected lint
# doesn't occur, helping keep code clean as issues are fixed.
#
# Test code may continue to use #[allow(...)] as needed.
#
# Exit codes:
#   0 - No violations found
#   1 - Violations found
#   2 - Script error
#
# Usage:
#   ./no-allow-attributes.sh [path]
#   ./no-allow-attributes.sh crates/ac-service/src/
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

print_header "Guard: No #[allow(...)] Attributes
Path: $SEARCH_PATH

Production code should use #[expect(...)] instead of #[allow(...)].
Test code is exempt from this rule."

# -----------------------------------------------------------------------------
# Check: #[allow(...)] attributes in production code
# -----------------------------------------------------------------------------
print_section "Checking for #[allow(...)] attributes"

# Find all #[allow(...)] patterns (but not #![allow(...)] which is crate-level)
# The pattern matches:
#   #[allow(dead_code)]
#   #[allow(unused_variables)]
#   #[allow(clippy::something)]
#   etc.
allow_violations=$(grep -rn --include="*.rs" -E \
    '#\[allow\([^)]+\)\]' \
    "$SEARCH_PATH" 2>/dev/null | \
    grep -Ev '#!\[allow' | \
    filter_test_code || true)

if [[ -n "$allow_violations" ]]; then
    echo -e "${RED}VIOLATIONS FOUND:${NC}"
    echo "$allow_violations" | while read -r line; do
        echo "  $line"
        increment_violations
    done
    echo ""
else
    print_ok "No #[allow(...)] attributes in production code"
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
    echo "Replace #[allow(...)] with #[expect(...)] in production code."
    echo ""
    echo "The #[expect(...)] attribute:"
    echo "  - Suppresses the lint (same as #[allow(...)])"
    echo "  - Warns if the expected lint doesn't occur"
    echo "  - Helps keep code clean as issues are fixed"
    echo ""
    echo "Example:"
    echo "  Before: #[allow(dead_code)]"
    echo "  After:  #[expect(dead_code)]"
    echo ""
    exit 1
else
    echo -e "${GREEN}No violations found!${NC}"
    exit 0
fi
