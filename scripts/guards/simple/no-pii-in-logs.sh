#!/bin/bash
#
# Simple Guard: No PII in Logs
#
# Detects PII (Personally Identifiable Information) in logging/tracing statements.
# Enforces observability.md principle: "NEVER log UNSAFE fields in plaintext"
#
# UNSAFE PII fields: email, phone, name, ip_address, user_agent
#
# Exit codes:
#   0 - No violations found
#   1 - Violations found
#   2 - Script error
#
# Usage:
#   ./no-pii-in-logs.sh [path]
#   ./no-pii-in-logs.sh crates/ac-service/src/
#

set -euo pipefail

# Source common library
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/../common.sh"

# Default to current directory if no path provided
SEARCH_PATH="${1:-.}"

# PII field patterns (case-insensitive)
# Note: 'name' is tricky - we use specific patterns to reduce false positives
PII_PATTERNS="email|phone|phone_number|ip_address|ip_addr|user_agent|full_name|user_name|first_name|last_name|real_name"

# Initialize
check_nightly_required
init_violations
start_timer

print_header "Guard: No PII in Logs
Path: $SEARCH_PATH"

# -----------------------------------------------------------------------------
# Check 1: PII variables in log macros
# -----------------------------------------------------------------------------
print_section "Check 1: PII variables in log macros"

# Look for info!, debug!, warn!, error!, trace! containing PII patterns
log_violations=$(grep -rn --include="*.rs" -E '\b(info|debug|warn|error|trace)!\s*\(' "$SEARCH_PATH" 2>/dev/null | \
    grep -Ei "(${PII_PATTERNS})" | \
    grep -v '//.*\|REDACTED\|\[REDACTED\]\|masked\|hashed\|h:' | \
    filter_test_code || true)

if [[ -n "$log_violations" ]]; then
    echo -e "${RED}VIOLATIONS FOUND:${NC}"
    echo "$log_violations" | while read -r line; do
        echo "  $line"
        increment_violations
    done
    echo ""
else
    print_ok "No PII variables in log macros"
    echo ""
fi

# -----------------------------------------------------------------------------
# Check 2: Named tracing fields with PII names
# -----------------------------------------------------------------------------
print_section "Check 2: Named tracing fields with PII names"

# Look for patterns like: email = %, ip_address = ?, user_agent =
named_field_violations=$(grep -rn --include="*.rs" -E "#\[instrument|tracing::(info|debug|warn|error|trace)!" "$SEARCH_PATH" 2>/dev/null | \
    grep -Ei "(${PII_PATTERNS})\s*=\s*[%?]" | \
    grep -Ev "skip\s*\(|REDACTED" | \
    filter_test_code || true)

if [[ -n "$named_field_violations" ]]; then
    echo -e "${RED}VIOLATIONS FOUND:${NC}"
    echo "$named_field_violations" | while read -r line; do
        echo "  $line"
        increment_violations
    done
    echo ""
else
    print_ok "No PII in named tracing fields"
    echo ""
fi

# -----------------------------------------------------------------------------
# Check 3: PII in #[instrument] fields() without skip
# -----------------------------------------------------------------------------
print_section "Check 3: PII in #[instrument] without skip"

# Find #[instrument] with fields() containing PII but not skipped
instrument_violations=$(grep -rn --include="*.rs" '#\[instrument' "$SEARCH_PATH" 2>/dev/null | \
    grep -i 'fields\s*(' | \
    grep -Ei "(${PII_PATTERNS})" | \
    grep -v 'skip\s*(' | \
    filter_test_code || true)

if [[ -n "$instrument_violations" ]]; then
    echo -e "${RED}VIOLATIONS FOUND:${NC}"
    echo "$instrument_violations" | while read -r line; do
        echo "  $line"
        increment_violations
    done
    echo ""
else
    print_ok "No PII in #[instrument] fields"
    echo ""
fi

# -----------------------------------------------------------------------------
# Check 4: PII in error/anyhow messages
# -----------------------------------------------------------------------------
print_section "Check 4: PII in error messages"

error_violations=$(grep -rn --include="*.rs" -E '(Err\(|anyhow!|bail!|context\()' "$SEARCH_PATH" 2>/dev/null | \
    grep -Ei "\{[^}]*(${PII_PATTERNS})[^}]*\}" | \
    grep -v '//.*\|REDACTED' | \
    filter_test_code || true)

if [[ -n "$error_violations" ]]; then
    echo -e "${YELLOW}POTENTIAL VIOLATIONS (review manually):${NC}"
    echo "$error_violations" | while read -r line; do
        echo "  $line"
        # Don't increment for "review manually" - these are warnings
    done
    echo ""
else
    print_ok "No PII in error messages"
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
    echo "PII must not be logged in plaintext. Options:"
    echo "  1. Remove PII from log statement"
    echo "  2. Use masked format: [REDACTED] or ****"
    echo "  3. Use hashed format: h:abc123 (for correlation)"
    echo "  4. Add skip(field) to #[instrument]"
    echo ""
    echo "See docs/principles/observability.md for field classifications."
    echo ""
    exit 1
else
    echo -e "${GREEN}No violations found!${NC}"
    exit 0
fi
