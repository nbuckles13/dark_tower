#!/bin/bash
#
# Simple Guard: No Hardcoded Secrets
#
# Detects hardcoded secrets in source code using grep patterns.
# This catches common patterns like API keys, passwords in code,
# and connection strings with embedded credentials.
#
# Exit codes:
#   0 - No violations found
#   1 - Violations found
#   2 - Script error
#
# Usage:
#   ./no-hardcoded-secrets.sh [path]
#   ./no-hardcoded-secrets.sh crates/ac-service/src/
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

print_header "Guard: No Hardcoded Secrets
Path: $SEARCH_PATH"

# -----------------------------------------------------------------------------
# Check 1: Secret variable assignments with string literals
# -----------------------------------------------------------------------------
print_section "Check 1: Secret variable assignments with literals"

# Pattern: password = "...", secret = "...", api_key = "...", etc.
# Excludes env var lookups, test code
secret_assignment_violations=$(grep -rn --include="*.rs" -Ei \
    '(password|secret|token|api_key|credential|master_key|private_key|client_secret)\s*[=:]\s*"[^"]+' \
    "$SEARCH_PATH" 2>/dev/null | \
    grep -Ev 'std::env|env::var|dotenvy' | \
    filter_test_code || true)

if [[ -n "$secret_assignment_violations" ]]; then
    echo -e "${RED}VIOLATIONS FOUND:${NC}"
    echo "$secret_assignment_violations" | while read -r line; do
        echo "  $line"
        increment_violations
    done
    echo ""
else
    print_ok "No secret variable assignments with literals"
    echo ""
fi

# -----------------------------------------------------------------------------
# Check 2: API key patterns
# -----------------------------------------------------------------------------
print_section "Check 2: API key patterns"

# Common API key prefixes:
# - sk-/pk- (Stripe, OpenAI)
# - AKIA (AWS Access Key ID)
# - ghp_/gho_ (GitHub tokens)
# - xox (Slack tokens)
api_key_violations=$(grep -rn --include="*.rs" -E \
    '"(sk-[a-zA-Z0-9]{20,}|pk-[a-zA-Z0-9]{20,}|AKIA[A-Z0-9]{16}|ghp_[a-zA-Z0-9]{36}|gho_[a-zA-Z0-9]{36}|xox[baprs]-[a-zA-Z0-9-]+)"' \
    "$SEARCH_PATH" 2>/dev/null | \
    filter_test_code || true)

if [[ -n "$api_key_violations" ]]; then
    echo -e "${RED}VIOLATIONS FOUND:${NC}"
    echo "$api_key_violations" | while read -r line; do
        echo "  $line"
        increment_violations
    done
    echo ""
else
    print_ok "No API key patterns found"
    echo ""
fi

# -----------------------------------------------------------------------------
# Check 3: Connection strings with embedded credentials
# -----------------------------------------------------------------------------
print_section "Check 3: Connection strings with credentials"

# Patterns: postgresql://user:password@host, redis://:password@host
# Note: The password part must have actual content (not just a variable)
connection_violations=$(grep -rn --include="*.rs" -E \
    '"(postgresql|mysql|redis|mongodb|amqp)://[^:]+:[^@{$]+@' \
    "$SEARCH_PATH" 2>/dev/null | \
    filter_test_code || true)

if [[ -n "$connection_violations" ]]; then
    echo -e "${RED}VIOLATIONS FOUND:${NC}"
    echo "$connection_violations" | while read -r line; do
        echo "  $line"
        increment_violations
    done
    echo ""
else
    print_ok "No connection strings with embedded credentials"
    echo ""
fi

# -----------------------------------------------------------------------------
# Check 4: Authorization headers with tokens
# -----------------------------------------------------------------------------
print_section "Check 4: Authorization headers with tokens"

# Pattern: "Authorization: Bearer ..." or "X-API-Key: ..."
# Only flag if the token looks like a real value (not a variable reference)
auth_header_violations=$(grep -rn --include="*.rs" -Ei \
    '"(Authorization:\s*(Bearer|Basic)\s+[A-Za-z0-9+/=_.~-]{20,})"' \
    "$SEARCH_PATH" 2>/dev/null | \
    filter_test_code || true)

if [[ -n "$auth_header_violations" ]]; then
    echo -e "${RED}VIOLATIONS FOUND:${NC}"
    echo "$auth_header_violations" | while read -r line; do
        echo "  $line"
        increment_violations
    done
    echo ""
else
    print_ok "No hardcoded authorization headers"
    echo ""
fi

# -----------------------------------------------------------------------------
# Check 5: Base64-encoded secrets (heuristic)
# -----------------------------------------------------------------------------
print_section "Check 5: Long Base64 strings (potential secrets)"

# Long base64 strings (40+ chars) that might be secrets
# This is a heuristic - mark as "review manually"
base64_findings=$(grep -rn --include="*.rs" -E \
    '"[A-Za-z0-9+/]{40,}={0,2}"' \
    "$SEARCH_PATH" 2>/dev/null | \
    filter_test_code || true)

if [[ -n "$base64_findings" ]]; then
    echo -e "${YELLOW}POTENTIAL VIOLATIONS (review manually):${NC}"
    echo "$base64_findings" | while read -r line; do
        echo "  $line"
        # Don't increment for "review manually" - these are warnings
    done
    echo ""
else
    print_ok "No suspicious base64 strings"
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
    echo "  1. Move secret to environment variable"
    echo "  2. Move secret to configuration file (not committed)"
    echo "  3. Use SecretString for runtime secrets"
    echo "  4. Mark as false positive with // guard:ignore comment"
    echo ""
    exit 1
else
    echo -e "${GREEN}No violations found!${NC}"
    exit 0
fi
