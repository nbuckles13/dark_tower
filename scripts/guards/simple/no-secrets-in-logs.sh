#!/bin/bash
#
# Simple Guard: No Secrets in Logs
#
# Detects potential credential leaks in logging statements using grep patterns.
# This is a fast, first-pass check. For complex cases, use the semantic guard.
#
# Exit codes:
#   0 - No violations found
#   1 - Violations found
#   2 - Script error
#
# Usage:
#   ./no-secrets-in-logs.sh [path]
#   ./no-secrets-in-logs.sh crates/ac-service/src/
#

set -euo pipefail

# Source common library
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/../common.sh"

# Default to current directory if no path provided
SEARCH_PATH="${1:-.}"

# Secret variable patterns (case-insensitive matching)
SECRET_PATTERNS="password|passwd|pwd|secret|token|key|credential|cred|bearer|api_key|master_key|private_key|client_secret|access_token|refresh_token"

# Initialize
check_nightly_required
init_violations
start_timer

print_header "Guard: No Secrets in Logs
Path: $SEARCH_PATH"

# -----------------------------------------------------------------------------
# Check 1: #[instrument] without skip for secret parameters
# -----------------------------------------------------------------------------
print_section "Check 1: #[instrument] without skip for secret parameters"

# Find functions with #[instrument] that have secret-sounding parameters
instrument_violations=$(grep -rn --include="*.rs" -B 3 'fn\s\+\w\+.*\b\(password\|secret\|token\|key\|credential\)\b.*\s*)' "$SEARCH_PATH" 2>/dev/null | \
    grep -B 3 'fn\s' | \
    grep '#\[instrument' | \
    grep -v 'skip.*password\|skip.*secret\|skip.*token\|skip.*key\|skip.*credential\|skip_all' | \
    filter_test_code || true)

if [[ -n "$instrument_violations" ]]; then
    echo -e "${RED}VIOLATIONS FOUND:${NC}"
    echo "$instrument_violations" | while read -r line; do
        echo "  $line"
        increment_violations
    done
    echo ""
else
    print_ok "No #[instrument] violations found"
    echo ""
fi

# -----------------------------------------------------------------------------
# Check 2: Direct logging of secret variables in log macros
# -----------------------------------------------------------------------------
print_section "Check 2: Secret variables in log macros"

# Look for info!, debug!, warn!, error!, trace! containing secret patterns
log_violations=$(grep -rn --include="*.rs" -E '\b(info|debug|warn|error|trace)!\s*\(' "$SEARCH_PATH" 2>/dev/null | \
    grep -Ei "\{[^}]*(${SECRET_PATTERNS})[^}]*\}|\%\s*(${SECRET_PATTERNS})\b|,\s*(${SECRET_PATTERNS})\s*[,\)]" | \
    grep -v '//.*\|REDACTED\|\[REDACTED\]\|skip(' | \
    filter_test_code || true)

if [[ -n "$log_violations" ]]; then
    echo -e "${RED}VIOLATIONS FOUND:${NC}"
    echo "$log_violations" | while read -r line; do
        echo "  $line"
        increment_violations
    done
    echo ""
else
    print_ok "No secret variables in log macros"
    echo ""
fi

# -----------------------------------------------------------------------------
# Check 3: Named tracing fields with secret names
# -----------------------------------------------------------------------------
print_section "Check 3: Named tracing fields with secret names"

# Look for patterns like: password = %, token = ?, secret =
named_field_violations=$(grep -rn --include="*.rs" -E "tracing::(info|debug|warn|error|trace)!\s*\(" "$SEARCH_PATH" 2>/dev/null | \
    grep -Ei "(${SECRET_PATTERNS})\s*=\s*[%?]" | \
    grep -v 'REDACTED\|\[REDACTED\]\|skip(' | \
    filter_test_code || true)

if [[ -n "$named_field_violations" ]]; then
    echo -e "${RED}VIOLATIONS FOUND:${NC}"
    echo "$named_field_violations" | while read -r line; do
        echo "  $line"
        increment_violations
    done
    echo ""
else
    print_ok "No named secret fields in tracing"
    echo ""
fi

# -----------------------------------------------------------------------------
# Check 4: Secrets in error/anyhow messages
# -----------------------------------------------------------------------------
print_section "Check 4: Secrets in error messages"

# Look for Err(), anyhow!(), bail!() containing secret variables
error_violations=$(grep -rn --include="*.rs" -E '(Err\(|anyhow!|bail!|context\()' "$SEARCH_PATH" 2>/dev/null | \
    grep -Ei "\{[^}]*(${SECRET_PATTERNS})[^}]*\}" | \
    grep -v '//.*\|REDACTED' | \
    filter_test_code || true)

if [[ -n "$error_violations" ]]; then
    echo -e "${YELLOW}POTENTIAL VIOLATIONS (review manually):${NC}"
    echo "$error_violations" | while read -r line; do
        echo "  $line"
        increment_violations
    done
    echo ""
else
    print_ok "No secrets in error messages"
    echo ""
fi

# -----------------------------------------------------------------------------
# Check 5: Debug formatting of structs that might contain secrets
# -----------------------------------------------------------------------------
print_section "Check 5: Debug formatting with {:?} on request/response objects"

# This is a heuristic - flag {:?} on common struct names that often contain secrets
debug_violations=$(grep -rn --include="*.rs" -E '\{:\?\}.*\b(request|req|response|res|body|payload|credentials|auth|login)\b|\b(request|req|response|res|body|payload|credentials|auth|login)\b.*\{:\?\}' "$SEARCH_PATH" 2>/dev/null | \
    grep -v '//.*\|#\[derive' | \
    filter_test_code || true)

if [[ -n "$debug_violations" ]]; then
    echo -e "${YELLOW}POTENTIAL VIOLATIONS (review manually):${NC}"
    echo "$debug_violations" | while read -r line; do
        echo "  $line"
        # Don't increment for "review manually" - these are warnings not failures
    done
    echo ""
else
    print_ok "No suspicious debug formatting"
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
    echo "Review each violation and either:"
    echo "  1. Add skip(...) to #[instrument] attributes"
    echo "  2. Remove secret from log message"
    echo "  3. Use [REDACTED] placeholder"
    echo "  4. Mark as false positive with // guard:ignore comment"
    echo ""
    exit 1
else
    echo -e "${GREEN}No violations found!${NC}"
    exit 0
fi
