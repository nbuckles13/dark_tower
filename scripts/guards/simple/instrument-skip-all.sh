#!/bin/bash
#
# Simple Guard: Instrument Skip-All
#
# Detects #[instrument] attributes using denylist approach (skip) instead
# of allowlist approach (skip_all).
#
# BAD (denylist - new fields leak by default):
#   #[instrument(skip(self, password))]
#   #[instrument(level = "debug", skip(token))]
#
# GOOD (allowlist - new fields hidden by default):
#   #[instrument(skip_all, fields(user_id = %user_id))]
#   #[instrument(skip_all)]
#
# The allowlist approach ensures new function parameters don't accidentally
# leak sensitive data into traces.
#
# Exit codes:
#   0 - No violations found
#   1 - Violations found
#   2 - Script error
#
# Usage:
#   ./instrument-skip-all.sh [path]
#   ./instrument-skip-all.sh crates/ac-service/src/
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

print_header "Guard: Instrument Skip-All
Path: $SEARCH_PATH"

# -----------------------------------------------------------------------------
# Check 1: #[instrument(skip(...))] without skip_all
# -----------------------------------------------------------------------------
print_section "Check 1: instrument with skip() but not skip_all"

# Find #[instrument(...)] that uses skip() but not skip_all
# Strategy:
# 1. Find all lines with #[instrument( that contain skip(
# 2. Exclude lines that have skip_all
#
# Note: This handles multi-line attributes poorly, but covers most cases.
# For complex multi-line cases, semantic analysis would be needed.

denylist_violations=$(grep -rn --include="*.rs" \
    '#\[instrument(' \
    "$SEARCH_PATH" 2>/dev/null | \
    grep 'skip(' | \
    grep -v 'skip_all' | \
    filter_test_code || true)

if [[ -n "$denylist_violations" ]]; then
    echo -e "${RED}VIOLATIONS FOUND:${NC}"
    echo "$denylist_violations" | while read -r line; do
        echo "  $line"
        increment_violations
    done
    echo ""
else
    print_ok "No denylist skip() patterns found"
    echo ""
fi

# -----------------------------------------------------------------------------
# Check 2: #[instrument] without skip_all on functions with sensitive params
# -----------------------------------------------------------------------------
print_section "Check 2: instrument on functions with sensitive parameters"

# This is a heuristic check. We look for functions that:
# 1. Have #[instrument] (or #[instrument(...)])
# 2. Have parameters named password, secret, token, key, credential, etc.
# 3. Don't have skip_all (checking within 3 lines to handle multi-line attributes)
#
# Strategy: For each #[instrument, check the next 3 lines for skip_all.
# This handles multi-line attributes formatted by cargo fmt.

# Use a simpler approach: collect violations to temp file to avoid subshell issues
temp_file=$(mktemp)
trap "rm -f $temp_file" EXIT

find "$SEARCH_PATH" -type f -name "*.rs" ! -path "*/test*" 2>/dev/null | while IFS= read -r file; do
    # For each #[instrument in this file
    grep -n '#\[instrument' "$file" 2>/dev/null | cut -d: -f1 | while read -r line_num; do
        # Get 4 lines starting from the instrument line
        context=$(sed -n "${line_num},$((line_num + 3))p" "$file" 2>/dev/null)

        # If skip_all is found in those 4 lines, skip to next
        echo "$context" | grep -q 'skip_all' && continue

        # Otherwise, check for sensitive params in next few lines
        extended=$(sed -n "${line_num},$((line_num + 8))p" "$file" 2>/dev/null)
        if echo "$extended" | grep -qEi '(password|secret|token|credential|private_key|client_secret|auth_code)[^a-z_]'; then
            echo "${file}:${line_num}:#[instrument(" >> "$temp_file"
        fi
    done
done || true

# Now process the results outside the subshell
if [[ -s "$temp_file" ]]; then
    echo -e "${YELLOW}POTENTIAL VIOLATIONS (functions with sensitive params but no skip_all):${NC}"
    while IFS= read -r line; do
        echo "  $line"
        increment_violations
    done < "$temp_file"
    echo ""
else
    print_ok "All instrument attributes use skip_all or have no sensitive parameters"
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
    echo "The #[instrument] attribute should use skip_all (allowlist) instead"
    echo "of skip() (denylist) to prevent accidental data leaks."
    echo ""
    echo "Instead of:"
    echo "  #[instrument(skip(self, password))]"
    echo ""
    echo "Use:"
    echo "  #[instrument(skip_all, fields(user_id = %user_id))]"
    echo ""
    echo "This ensures new parameters don't accidentally leak into traces."
    echo ""
    exit 1
else
    echo -e "${GREEN}No violations found!${NC}"
    exit 0
fi
