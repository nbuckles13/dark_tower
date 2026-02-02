#!/bin/bash
#
# Simple Guard: API Route Pattern Check
#
# Enforces consistent API route patterns across all services.
#
# Route patterns:
#   - /api/v{N}/* - Versioned API endpoints (REQUIRED for all API routes)
#   - /health, /ready, /metrics - Operational endpoints (NO version prefix)
#   - /.well-known/* - RFC-defined paths (NO version prefix)
#   - /internal/* - Internal-only endpoints (NO version prefix)
#
# What it catches:
#   - API routes without /api/v{N}/ prefix (e.g., /v1/users is WRONG)
#   - Operational endpoints with version prefix (e.g., /v1/health is WRONG)
#
# Exit codes:
#   0 - No violations found
#   1 - Violations found
#   2 - Script error
#
# Usage:
#   ./api-version-check.sh [path]
#   ./api-version-check.sh crates/ac-service/src/
#

set -euo pipefail

# Source common library
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/../common.sh"

# Default to current directory if no path provided
SEARCH_PATH="${1:-.}"

# Initialize
init_violations
start_timer

print_header "Guard: API Version Check
Path: $SEARCH_PATH"

# -----------------------------------------------------------------------------
# Check 1: API routes must use /api/v{N}/ prefix
# -----------------------------------------------------------------------------
print_section "Check 1: API routes without /api/v{N}/ prefix"

# Find .route( calls
route_definitions=$(grep -rn --include="*.rs" '\.route\s*(' "$SEARCH_PATH" 2>/dev/null | \
    grep -E '\.route\s*\(\s*"/' | \
    filter_test_code || true)

if [[ -z "$route_definitions" ]]; then
    print_ok "No route definitions found"
    echo ""
else
    # Check each route for correct pattern
    violations=""
    while IFS= read -r line; do
        # Extract the path from the route call
        path=$(echo "$line" | grep -oE '\.route\s*\(\s*"[^"]*"' | grep -oE '"[^"]*"' | tr -d '"')

        if [[ -z "$path" ]]; then
            continue
        fi

        # Allowed unversioned paths (operational/standard endpoints)
        if [[ "$path" =~ ^/.well-known ]]; then
            continue
        fi
        if [[ "$path" =~ ^/health$ || "$path" =~ ^/ready$ || "$path" =~ ^/metrics$ || "$path" =~ ^/metrics/ ]]; then
            continue
        fi
        if [[ "$path" =~ ^/internal ]]; then
            continue
        fi

        # API routes MUST use /api/v{N}/ prefix
        if [[ "$path" =~ ^/api/v[0-9]+/ ]]; then
            continue
        fi

        # This is a violation - either:
        # - Uses /v{N}/ without /api prefix (wrong pattern)
        # - Has no version at all (missing version)
        violations="$violations$line\n"
    done <<< "$route_definitions"

    if [[ -n "$violations" ]]; then
        echo -e "${RED}VIOLATIONS FOUND:${NC}"
        echo -e "$violations" | while read -r line; do
            if [[ -n "$line" ]]; then
                echo "  $line"
                increment_violations
            fi
        done
        echo ""
    else
        print_ok "All API routes use /api/v{N}/ prefix"
        echo ""
    fi
fi

# -----------------------------------------------------------------------------
# Check 2: Operational endpoints must NOT have version prefix
# -----------------------------------------------------------------------------
print_section "Check 2: Versioned operational endpoints"

# Look for versioned health/ready/metrics endpoints (wrong pattern)
# Matches: /v1/health, /api/v1/health, /v2/ready, etc.
# Does NOT match: /health, /ready, /metrics (correct unversioned)
versioned_ops=$(grep -rn --include="*.rs" '\.route\s*(' "$SEARCH_PATH" 2>/dev/null | \
    grep -E '\.route\s*\(\s*"/(api/)?v[0-9]+/(health|ready|metrics)' | \
    filter_test_code || true)

if [[ -n "$versioned_ops" ]]; then
    echo -e "${RED}VIOLATIONS FOUND:${NC}"
    echo "  Operational endpoints should not have version prefix."
    echo "  Use /health, /ready, /metrics (not /v1/health, /api/v1/health, etc.)"
    echo ""
    echo "$versioned_ops" | while read -r line; do
        echo "  $line"
        increment_violations
    done
    echo ""
else
    print_ok "Operational endpoints are unversioned"
    echo ""
fi

# -----------------------------------------------------------------------------
# Check 3: Format string routes (manual review)
# -----------------------------------------------------------------------------
print_section "Check 3: Dynamic format routes"

# Look for format! or format_args! used in route paths
format_routes=$(grep -rn --include="*.rs" -E '\.route\s*\(\s*&?format!' "$SEARCH_PATH" 2>/dev/null | \
    filter_test_code || true)

if [[ -n "$format_routes" ]]; then
    echo -e "${YELLOW}REVIEW MANUALLY (dynamic routes):${NC}"
    echo "$format_routes" | while read -r line; do
        echo "  $line"
    done
    echo ""
    echo "  Dynamic routes cannot be statically checked."
    echo "  Please verify manually that API routes use /api/v{N}/ prefix."
    echo ""
else
    print_ok "No dynamic format routes found"
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
    echo "Route pattern rules:"
    echo ""
    echo "  API routes (versioned):"
    echo "    CORRECT: /api/v1/users, /api/v1/meetings/:id"
    echo "    WRONG:   /v1/users, /users"
    echo ""
    echo "  Operational endpoints (unversioned):"
    echo "    CORRECT: /health, /ready, /metrics"
    echo "    WRONG:   /v1/health, /api/v1/health"
    echo ""
    echo "  Other unversioned (allowed):"
    echo "    /.well-known/* (RFC-defined)"
    echo "    /internal/* (internal-only)"
    echo ""
    echo "See docs/principles/api-design.md for details."
    echo ""
    exit 1
else
    echo -e "${GREEN}No violations found!${NC}"
    exit 0
fi
