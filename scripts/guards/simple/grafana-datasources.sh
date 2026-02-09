#!/bin/bash
#
# Simple Guard: Grafana Datasource Validation
#
# Validates that all datasource UIDs referenced in Grafana dashboard JSON files
# are defined in the datasource provisioning configuration.
#
# This prevents dashboard/datasource mismatches that cause "no data" errors
# in Grafana. Added as part of PRR-0001 follow-up.
#
# Exit codes:
#   0 - No violations found
#   1 - Violations found
#   2 - Script error
#
# Usage:
#   ./grafana-datasources.sh
#
# Note: This guard operates on fixed paths and does not accept a path argument.
#

set -euo pipefail

# Source common library
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/../common.sh"

PROJECT_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
DASHBOARD_DIR="$PROJECT_ROOT/infra/grafana/dashboards"
DATASOURCE_CONFIG="$PROJECT_ROOT/infra/grafana/provisioning/datasources/datasources.yaml"

# Initialize
init_violations
start_timer

print_header "Guard: Grafana Datasource Validation
Dashboard Dir: $DASHBOARD_DIR
Datasource Config: $DATASOURCE_CONFIG"

# -----------------------------------------------------------------------------
# Check 1: Required paths exist
# -----------------------------------------------------------------------------
print_section "Check 1: Required paths exist"

if [ ! -d "$DASHBOARD_DIR" ]; then
    print_warning "Dashboard directory not found: $DASHBOARD_DIR"
    echo "Skipping guard - no dashboards to validate"
    echo ""
    exit 0
fi

if [ ! -f "$DATASOURCE_CONFIG" ]; then
    print_warning "Datasource config not found: $DATASOURCE_CONFIG"
    echo "Skipping guard - no datasource config to validate against"
    echo ""
    exit 0
fi

print_ok "Both dashboard directory and datasource config exist"
echo ""

# -----------------------------------------------------------------------------
# Check 2: Datasource config has UIDs defined
# -----------------------------------------------------------------------------
print_section "Check 2: Datasource config has UIDs defined"

defined_uids=$(grep -E "^\s+uid:" "$DATASOURCE_CONFIG" | awk '{print $2}' | sort -u || true)

if [ -z "$defined_uids" ]; then
    print_warning "No datasource UIDs defined in $DATASOURCE_CONFIG"
    echo "Consider adding explicit 'uid:' fields to datasource definitions"
    echo ""
else
    uid_count=$(echo "$defined_uids" | wc -l | tr -d ' ')
    print_ok "Found $uid_count datasource UID(s) defined"
    echo ""
fi

# -----------------------------------------------------------------------------
# Check 3: Dashboard files exist
# -----------------------------------------------------------------------------
print_section "Check 3: Dashboard files exist"

dashboard_count=$(find "$DASHBOARD_DIR" -name "*.json" 2>/dev/null | wc -l | tr -d ' ')
if [ "$dashboard_count" -eq 0 ]; then
    print_ok "No dashboard files found in $DASHBOARD_DIR - nothing to validate"
    echo ""
    print_header "Summary"
    print_elapsed_time
    echo ""
    echo -e "${GREEN}No violations found!${NC}"
    exit 0
fi

print_ok "Found $dashboard_count dashboard file(s) to validate"
echo ""

# -----------------------------------------------------------------------------
# Check 4: All referenced UIDs are defined
# -----------------------------------------------------------------------------
print_section "Check 4: All referenced datasource UIDs are defined"

# Check if jq is available for proper JSON parsing
if ! command -v jq &> /dev/null; then
    print_warning "jq not found - skipping datasource UID validation"
    echo "Install jq for proper Grafana dashboard validation"
    echo ""
    print_header "Summary"
    print_elapsed_time
    echo ""
    echo -e "${GREEN}Skipped - jq not available${NC}"
    exit 0
fi

# Extract datasource UIDs from panels using jq (proper JSON parsing)
# This extracts only UIDs from "datasource": {"uid": "..."} contexts,
# excluding dashboard UIDs at root level and other non-datasource UIDs
referenced_uids=$(find "$DASHBOARD_DIR" -name "*.json" -exec jq -r '
    .. |
    objects |
    select(has("datasource")) |
    .datasource |
    select(type == "object") |
    .uid // empty
' {} \; 2>/dev/null | \
    grep -v "^-- Grafana --$" | \
    sort -u || true)

if [ -z "$referenced_uids" ]; then
    print_ok "No datasource UIDs referenced in dashboards"
    echo ""
else
    # Check each referenced UID exists in defined UIDs
    for uid in $referenced_uids; do
        if ! echo "$defined_uids" | grep -q "^${uid}$"; then
            echo -e "${RED}VIOLATION:${NC} Dashboard references undefined datasource UID: $uid"
            # Find which dashboard file contains this reference
            grep -rl "\"uid\": \"$uid\"" "$DASHBOARD_DIR"/*.json 2>/dev/null | while read -r file; do
                echo "  -> Found in: $(basename "$file")"
            done
            increment_violations
        fi
    done

    if [ "$(get_violations)" -eq 0 ]; then
        print_ok "All referenced datasource UIDs are defined"
    fi
    echo ""
fi

# -----------------------------------------------------------------------------
# Summary
# -----------------------------------------------------------------------------
print_header "Summary"

TOTAL_VIOLATIONS=$(get_violations)
print_elapsed_time
echo ""

if [ $TOTAL_VIOLATIONS -gt 0 ]; then
    echo -e "${RED}Found $TOTAL_VIOLATIONS violation(s)${NC}"
    echo ""
    echo "To fix: Add the missing UID(s) to $DATASOURCE_CONFIG"
    echo "Example:"
    echo "  - name: Prometheus"
    echo "    type: prometheus"
    echo "    uid: prometheus    # <-- Add this line"
    echo ""
    exit 1
else
    echo -e "${GREEN}No violations found!${NC}"
    echo "Validated $dashboard_count dashboard(s) against datasource config"
    exit 0
fi
