#!/bin/bash
# Validate Grafana dashboard datasource references
#
# This script ensures all datasource UIDs referenced in dashboard JSON files
# are defined in the datasource provisioning configuration.
#
# Added as part of PRR-0001 follow-up to prevent dashboard/datasource mismatches.

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

DASHBOARD_DIR="$PROJECT_ROOT/infra/grafana/dashboards"
DATASOURCE_CONFIG="$PROJECT_ROOT/infra/grafana/provisioning/datasources/datasources.yaml"

# Check required files exist
if [ ! -d "$DASHBOARD_DIR" ]; then
    echo "Dashboard directory not found: $DASHBOARD_DIR"
    exit 1
fi

if [ ! -f "$DATASOURCE_CONFIG" ]; then
    echo "Datasource config not found: $DATASOURCE_CONFIG"
    exit 1
fi

# Extract defined datasource UIDs from config
defined_uids=$(grep -E "^\s+uid:" "$DATASOURCE_CONFIG" | awk '{print $2}' | sort -u)

if [ -z "$defined_uids" ]; then
    echo "WARNING: No datasource UIDs defined in $DATASOURCE_CONFIG"
    echo "Consider adding explicit 'uid:' fields to datasource definitions"
fi

# Check if there are any dashboard files
dashboard_count=$(find "$DASHBOARD_DIR" -name "*.json" 2>/dev/null | wc -l)
if [ "$dashboard_count" -eq 0 ]; then
    echo "No dashboard files found in $DASHBOARD_DIR"
    exit 0
fi

# Extract referenced UIDs from dashboards (excluding built-in "-- Grafana --")
referenced_uids=$(grep -rho '"uid": "[^"]*"' "$DASHBOARD_DIR"/*.json 2>/dev/null | \
    sed 's/"uid": "//g; s/"//g' | \
    grep -v "^-- Grafana --$" | \
    grep -v "^ac-service-dashboard$" | \
    sort -u)

# Check each referenced UID exists in defined UIDs
errors=0
for uid in $referenced_uids; do
    if ! echo "$defined_uids" | grep -q "^${uid}$"; then
        echo "ERROR: Dashboard references undefined datasource UID: $uid"
        # Find which dashboard file contains this reference
        grep -rl "\"uid\": \"$uid\"" "$DASHBOARD_DIR"/*.json 2>/dev/null | while read -r file; do
            echo "  -> Found in: $(basename "$file")"
        done
        errors=$((errors + 1))
    fi
done

if [ $errors -gt 0 ]; then
    echo ""
    echo "Found $errors undefined datasource UID reference(s)"
    echo ""
    echo "To fix: Add the missing UID(s) to $DATASOURCE_CONFIG"
    echo "Example:"
    echo "  - name: Prometheus"
    echo "    type: prometheus"
    echo "    uid: prometheus    # <-- Add this line"
    exit 1
fi

echo "✓ All dashboard datasource UIDs are valid ($dashboard_count dashboard(s) checked)"
