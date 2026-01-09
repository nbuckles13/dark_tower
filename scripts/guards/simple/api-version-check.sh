#!/bin/bash
#
# Simple Guard: API Version Check
#
# Detects API route definitions without version prefix.
# Enforces api-design.md principle: "Include version in all API paths"
#
# What it checks:
#   - Axum .route() calls must have /api/v{N}/ or /v{N}/ prefix
#
# Allowed exceptions:
#   - /.well-known/* (RFC-defined paths)
#   - /health, /ready, /metrics (operational endpoints)
#   - /internal/* (internal-only endpoints)
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
# Check 1: Route definitions without version prefix
# -----------------------------------------------------------------------------
print_section "Check 1: Unversioned API routes"

# Find .route( calls and check for version prefix
# We look for .route("/something" where the path doesn't start with:
# - /api/v followed by digit
# - /v followed by digit
# - /.well-known
# - /health, /ready, /metrics
# - /internal

# First, find all route definitions
route_definitions=$(grep -rn --include="*.rs" '\.route\s*(' "$SEARCH_PATH" 2>/dev/null | \
    grep -E '\.route\s*\(\s*"/' | \
    filter_test_code || true)

if [[ -z "$route_definitions" ]]; then
    print_ok "No route definitions found"
    echo ""
else
    # Check each route for version prefix
    violations=""
    while IFS= read -r line; do
        # Extract the path from the route call
        # Match patterns like: .route("/path", ...)
        path=$(echo "$line" | grep -oE '\.route\s*\(\s*"[^"]*"' | grep -oE '"[^"]*"' | tr -d '"')

        if [[ -z "$path" ]]; then
            continue
        fi

        # Check if path is an allowed exception
        if [[ "$path" =~ ^/.well-known ]]; then
            continue
        fi
        if [[ "$path" =~ ^/health$ || "$path" =~ ^/ready$ || "$path" =~ ^/metrics ]]; then
            continue
        fi
        if [[ "$path" =~ ^/internal ]]; then
            continue
        fi

        # Check if path has version prefix
        if [[ "$path" =~ ^/api/v[0-9]+ || "$path" =~ ^/v[0-9]+ ]]; then
            continue
        fi

        # This is a violation
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
        print_ok "All routes have version prefix or are allowed exceptions"
        echo ""
    fi
fi

# -----------------------------------------------------------------------------
# Check 2: Format string routes without version
# -----------------------------------------------------------------------------
print_section "Check 2: Format string routes"

# Look for format! or format_args! used in route paths
format_routes=$(grep -rn --include="*.rs" -E '\.route\s*\(\s*&?format!' "$SEARCH_PATH" 2>/dev/null | \
    filter_test_code || true)

if [[ -n "$format_routes" ]]; then
    echo -e "${YELLOW}REVIEW MANUALLY (dynamic routes):${NC}"
    echo "$format_routes" | while read -r line; do
        echo "  $line"
    done
    echo ""
    echo "  Dynamic routes cannot be statically checked for version prefix."
    echo "  Please verify manually that these include /api/v{N}/ or /v{N}/."
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
    echo "All API routes must include a version prefix. Options:"
    echo "  1. Use /api/v1/endpoint for external APIs"
    echo "  2. Use /v1/endpoint for simpler versioning"
    echo ""
    echo "Allowed exceptions (no version required):"
    echo "  - /.well-known/* (RFC-defined paths)"
    echo "  - /health, /ready, /metrics (operational)"
    echo "  - /internal/* (internal-only endpoints)"
    echo ""
    echo "See docs/principles/api-design.md for versioning guidelines."
    echo ""
    exit 1
else
    echo -e "${GREEN}No violations found!${NC}"
    exit 0
fi
