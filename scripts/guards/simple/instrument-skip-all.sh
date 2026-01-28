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
# 3. Don't have skip_all
#
# This requires looking at consecutive lines, which is harder in bash.
# For now, we look for instrument attributes and check if they contain skip_all
# when on functions that might have sensitive params.

# Get all instrument attributes without skip_all
instruments_without_skip_all=$(grep -rn --include="*.rs" \
    '#\[instrument' \
    "$SEARCH_PATH" 2>/dev/null | \
    grep -v 'skip_all' | \
    filter_test_code || true)

if [[ -n "$instruments_without_skip_all" ]]; then
    # Check if the next few lines contain sensitive parameter names
    sensitive_findings=""
    while IFS= read -r line; do
        file=$(echo "$line" | cut -d: -f1)
        line_num=$(echo "$line" | cut -d: -f2)

        # Read a few lines after the instrument attribute to find the fn signature
        # Using sed to get lines N through N+5
        fn_context=$(sed -n "${line_num},$((line_num + 5))p" "$file" 2>/dev/null || true)

        # Check if context contains sensitive parameter names
        if echo "$fn_context" | grep -qEi '(password|secret|token|credential|private_key|client_secret|auth_code)[^a-z_]'; then
            sensitive_findings="${sensitive_findings}${line}\n"
        fi
    done <<< "$instruments_without_skip_all"

    if [[ -n "$sensitive_findings" ]]; then
        echo -e "${YELLOW}POTENTIAL VIOLATIONS (functions with sensitive params but no skip_all):${NC}"
        echo -e "$sensitive_findings" | grep -v '^$' | while read -r line; do
            echo "  $line"
            # Increment as violation since this is a security concern
            increment_violations
        done
        echo ""
    else
        print_ok "No sensitive parameter exposure risks"
        echo ""
    fi
else
    print_ok "All instrument attributes use skip_all or have no parameters"
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
