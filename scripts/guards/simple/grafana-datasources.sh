#!/bin/bash
#
# Simple Guard: Grafana Datasource Validation
#
# Validates:
# 1. All datasource UIDs referenced in Grafana dashboard JSON files
#    are defined in the datasource provisioning configuration.
# 2. All Loki labels used in dashboard queries are defined in the
#    Promtail configuration (dashboard-Loki label consistency).
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
PROMTAIL_CONFIG="$PROJECT_ROOT/infra/kubernetes/observability/promtail-config.yaml"

# Initialize
init_violations
start_timer

print_header "Guard: Grafana Datasource Validation
Dashboard Dir: $DASHBOARD_DIR
Datasource Config: $DATASOURCE_CONFIG
Promtail Config: $PROMTAIL_CONFIG"

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

print_ok "Dashboard directory and datasource config exist"

if [ ! -f "$PROMTAIL_CONFIG" ]; then
    print_warning "Promtail config not found: $PROMTAIL_CONFIG"
    echo "Loki label validation will be skipped"
    PROMTAIL_EXISTS=false
else
    print_ok "Promtail config exists"
    PROMTAIL_EXISTS=true
fi
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
else
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
fi

# -----------------------------------------------------------------------------
# Check 5: Loki label consistency (dashboard queries use valid Promtail labels)
# -----------------------------------------------------------------------------
print_section "Check 5: Loki label consistency"

if [ "$PROMTAIL_EXISTS" = false ]; then
    print_warning "Skipping - Promtail config not found"
    echo ""
elif ! command -v jq &> /dev/null; then
    print_warning "Skipping - jq not available"
    echo ""
elif ! command -v python3 &> /dev/null; then
    print_warning "Skipping - python3 not available for YAML parsing"
    echo ""
else
    # Extract valid Loki labels from Promtail config using Python
    # These are the target_label values from relabel_configs with action: replace
    valid_loki_labels=$(python3 -c "
import yaml
import sys

config_file = '$PROMTAIL_CONFIG'

try:
    with open(config_file, 'r') as f:
        content = f.read()

    # Parse all YAML documents (separated by ---)
    docs = list(yaml.safe_load_all(content))

    labels = set()

    for doc in docs:
        if doc is None:
            continue

        # Look for ConfigMap with promtail.yaml data
        if doc.get('kind') == 'ConfigMap' and 'data' in doc:
            promtail_yaml = doc['data'].get('promtail.yaml', '')
            if promtail_yaml:
                promtail_config = yaml.safe_load(promtail_yaml)
                if promtail_config and 'scrape_configs' in promtail_config:
                    for scrape_config in promtail_config['scrape_configs']:
                        relabel_configs = scrape_config.get('relabel_configs', [])
                        for relabel in relabel_configs:
                            # Get target_label from relabel configs
                            # Default action is 'replace' if not specified
                            action = relabel.get('action', 'replace')
                            target_label = relabel.get('target_label', '')
                            if action == 'replace' and target_label and not target_label.startswith('__'):
                                labels.add(target_label)

                        # Extract labels from pipeline_stages (json expressions + labels)
                        pipeline_stages = scrape_config.get('pipeline_stages', [])
                        for stage in pipeline_stages:
                            # Check for labels stage (promotes fields to Loki labels)
                            if isinstance(stage, dict) and 'labels' in stage:
                                label_map = stage['labels']
                                if isinstance(label_map, dict):
                                    # Labels can be mapped: {level: extracted_field}
                                    # We care about the label key (what appears in Loki)
                                    for label_key in label_map.keys():
                                        labels.add(label_key)

    # Print labels one per line
    for label in sorted(labels):
        print(label)

except Exception as e:
    print(f'Error: {e}', file=sys.stderr)
    sys.exit(1)
" 2>/dev/null) || valid_loki_labels=""

    if [ -z "$valid_loki_labels" ]; then
        print_warning "Could not extract Loki labels from Promtail config"
        echo ""
    else
        label_count=$(echo "$valid_loki_labels" | wc -l | tr -d ' ')
        echo "Valid Loki labels from Promtail: $valid_loki_labels" | tr '\n' ', ' | sed 's/,$/\n/'
        echo ""

        # Extract Loki queries from dashboards and check for invalid labels
        # We look for expr fields in targets with Loki datasource
        invalid_labels_found=false

        for dashboard in "$DASHBOARD_DIR"/*.json; do
            [ -f "$dashboard" ] || continue
            dashboard_name=$(basename "$dashboard")

            # Extract all Loki query expressions from this dashboard
            # Look for targets where datasource.type == "loki" and extract expr
            loki_exprs=$(jq -r '
                .. |
                objects |
                select(.datasource?.type == "loki" or .datasource?.uid == "loki") |
                select(has("expr")) |
                .expr // empty
            ' "$dashboard" 2>/dev/null || true)

            # Also check template variable queries
            template_exprs=$(jq -r '
                .templating.list[]? |
                select(.datasource?.type == "loki" or .datasource?.uid == "loki") |
                .query.stream // empty
            ' "$dashboard" 2>/dev/null || true)

            all_exprs=$(printf "%s\n%s" "$loki_exprs" "$template_exprs")

            if [ -n "$all_exprs" ]; then
                # Extract label names from LogQL queries
                # Labels appear in format: {label="value"} or {label=~"regex"}
                # Use grep to find label= patterns and extract label names
                used_labels=$(echo "$all_exprs" | \
                    grep -oE '\{[^}]+\}' | \
                    grep -oE '[a-zA-Z_][a-zA-Z0-9_]*=' | \
                    sed 's/=$//' | \
                    sort -u || true)

                if [ -n "$used_labels" ]; then
                    for label in $used_labels; do
                        # Check if this label is in valid_loki_labels
                        if ! echo "$valid_loki_labels" | grep -q "^${label}$"; then
                            # Special case: 'level' is extracted by pipeline stages (regex + labels)
                            # not by relabel_configs, so it won't appear in the relabel_configs list
                            # This is a standard pattern for log level filtering in Loki
                            if [ "$label" != "level" ]; then
                                echo -e "${RED}VIOLATION:${NC} Dashboard uses invalid Loki label: '$label'"
                                echo "  -> Dashboard: $dashboard_name"
                                # Find the panel title that uses this label
                                panel_titles=$(jq -r --arg label "$label" '
                                    .. |
                                    objects |
                                    select(.datasource?.type == "loki" or .datasource?.uid == "loki") |
                                    select(.expr // "" | contains($label + "=")) |
                                    .title // "Unknown panel"
                                ' "$dashboard" 2>/dev/null | sort -u || true)
                                if [ -n "$panel_titles" ]; then
                                    echo "$panel_titles" | while read -r title; do
                                        echo "  -> Panel: $title"
                                    done
                                fi
                                echo "  -> Valid labels: $(echo "$valid_loki_labels" | tr '\n' ', ' | sed 's/,$//')"
                                increment_violations
                                invalid_labels_found=true
                            fi
                        fi
                    done
                fi
            fi
        done

        if [ "$invalid_labels_found" = false ] && [ "$(get_violations)" -eq 0 ]; then
            print_ok "All dashboard Loki queries use valid labels"
        fi
        echo ""
    fi
fi

# -----------------------------------------------------------------------------
# Check 6: Dashboard variable consistency (variables match queried labels)
# -----------------------------------------------------------------------------
print_section "Check 6: Dashboard variable consistency"

if ! command -v jq &> /dev/null; then
    print_warning "Skipping - jq not available"
    echo ""
elif [ "$PROMTAIL_EXISTS" = false ]; then
    print_warning "Skipping - Promtail config not found"
    echo ""
else
    # Validate that dashboard variables:
    # 1. Variable name matches the label it queries
    # 2. Variable queries a valid Loki label (from Promtail config)

    variable_issues_found=false

    for dashboard in "$DASHBOARD_DIR"/*.json; do
        [ -f "$dashboard" ] || continue
        dashboard_name=$(basename "$dashboard")

        # Extract Loki template variables
        loki_variables=$(jq -r '
            .templating.list[]? |
            select(.datasource?.type == "loki" or .datasource?.uid == "loki") |
            select(has("query")) |
            @json
        ' "$dashboard" 2>/dev/null || true)

        if [ -n "$loki_variables" ]; then
            echo "$loki_variables" | while IFS= read -r variable_json; do
                var_name=$(echo "$variable_json" | jq -r '.name')
                queried_label=$(echo "$variable_json" | jq -r '.query.label // empty')

                if [ -z "$queried_label" ]; then
                    continue
                fi

                # Check 1: Variable name should match queried label (best practice)
                if [ "$var_name" != "$queried_label" ]; then
                    echo -e "${RED}VIOLATION:${NC} Variable name mismatch in $dashboard_name"
                    echo "  -> Variable name: '$var_name' queries label: '$queried_label'"
                    echo "  -> Best practice: Variable name should match the label it queries"
                    echo "  -> Fix: Either rename variable to '$queried_label' or change query to use '$var_name' label"
                    increment_violations
                    variable_issues_found=true
                fi

                # Check 2: Queried label must exist in valid Loki labels
                if [ -n "$valid_loki_labels" ]; then
                    if ! echo "$valid_loki_labels" | grep -q "^${queried_label}$"; then
                        # Special case: 'level' is extracted by pipeline stages
                        if [ "$queried_label" != "level" ]; then
                            echo -e "${RED}VIOLATION:${NC} Variable queries invalid Loki label in $dashboard_name"
                            echo "  -> Variable: '$var_name' queries label: '$queried_label'"
                            echo "  -> Valid labels: $(echo "$valid_loki_labels" | tr '\n' ', ' | sed 's/,$//')"
                            increment_violations
                            variable_issues_found=true
                        fi
                    fi
                fi
            done
        fi
    done

    if [ "$variable_issues_found" = false ] && [ "$(get_violations)" -eq 0 ]; then
        print_ok "All dashboard variables are consistent"
    fi
    echo ""
fi

# -----------------------------------------------------------------------------
# Check 7: Prometheus query validation (labels and patterns)
# -----------------------------------------------------------------------------
print_section "Check 7: Prometheus query validation"

PROMETHEUS_CONFIG="$PROJECT_ROOT/infra/kubernetes/observability/prometheus-config.yaml"

if ! command -v jq &> /dev/null; then
    print_warning "Skipping - jq not available"
    echo ""
elif ! command -v python3 &> /dev/null; then
    print_warning "Skipping - python3 not available"
    echo ""
elif [ ! -f "$PROMETHEUS_CONFIG" ]; then
    print_warning "Skipping - Prometheus config not found"
    echo ""
else
    # Extract valid Prometheus labels from Prometheus config using Python
    # Includes standard K8s labels when using kubernetes_sd_configs
    # and custom labels from relabel_configs
    valid_prometheus_labels=$(python3 -c "
import yaml
import sys

config_file = '$PROMETHEUS_CONFIG'

try:
    with open(config_file, 'r') as f:
        content = f.read()

    # Parse all YAML documents (separated by ---)
    docs = list(yaml.safe_load_all(content))

    labels = set()

    for doc in docs:
        if doc is None:
            continue

        # Look for ConfigMap with prometheus.yml data
        if doc.get('kind') == 'ConfigMap' and 'data' in doc:
            prom_yaml = doc['data'].get('prometheus.yml', '')
            if prom_yaml:
                prom_config = yaml.safe_load(prom_yaml)
                if prom_config and 'scrape_configs' in prom_config:
                    for scrape_config in prom_config['scrape_configs']:
                        # If using Kubernetes SD, add standard K8s labels
                        if 'kubernetes_sd_configs' in scrape_config:
                            # Standard Kubernetes labels available with kubernetes_sd_configs
                            labels.update(['namespace', 'pod', 'node', 'container', 'service', 'endpoint'])

                        # Extract custom labels from relabel_configs
                        relabel_configs = scrape_config.get('relabel_configs', [])
                        for relabel in relabel_configs:
                            # Get target_label from relabel configs
                            action = relabel.get('action', 'replace')
                            target_label = relabel.get('target_label', '')
                            if action == 'replace' and target_label and not target_label.startswith('__'):
                                labels.add(target_label)

    # Always allow 'job' and 'instance' - these are standard Prometheus labels
    labels.update(['job', 'instance'])

    # Print labels one per line
    for label in sorted(labels):
        print(label)

except Exception as e:
    print(f'Error: {e}', file=sys.stderr)
    sys.exit(1)
" 2>/dev/null) || valid_prometheus_labels=""

    if [ -z "$valid_prometheus_labels" ]; then
        print_warning "Could not extract Prometheus labels from config"
        echo ""
    else
        echo "Valid Prometheus labels from config: $(echo "$valid_prometheus_labels" | tr '\n' ', ' | sed 's/,$//')"
        echo ""

        # Validate Prometheus queries in dashboards
        prometheus_issues_found=false

        for dashboard in "$DASHBOARD_DIR"/*.json; do
            [ -f "$dashboard" ] || continue
            dashboard_name=$(basename "$dashboard")

            # Extract all Prometheus query expressions from this dashboard
            # Look for targets where datasource.type == "prometheus" or datasource.uid == "prometheus"
            prometheus_exprs=$(jq -r '
                .. |
                objects |
                select(.datasource?.type == "prometheus" or .datasource?.uid == "prometheus") |
                select(has("expr")) |
                .expr // empty
            ' "$dashboard" 2>/dev/null || true)

            if [ -n "$prometheus_exprs" ]; then
                # Check for Docker patterns (invalid in Kubernetes environment)
                # Note: Currently informational only - will be enforced in future iterations
                if echo "$prometheus_exprs" | grep -qE '\bname\s*=~?"'; then
                    echo -e "${YELLOW}INFO:${NC} Dashboard uses Docker 'name' label pattern (will not work in Kubernetes)"
                    echo "  -> Dashboard: $dashboard_name"
                    echo "  -> Docker pattern detected: {name=~\"...\"}"
                    echo "  -> Kubernetes equivalent: Use {namespace=\"...\", pod=~\"...\"}"
                    echo "  -> Note: This is informational only - not blocking guards yet"
                    # Don't increment violations yet - this is informational
                    # increment_violations
                    prometheus_issues_found=true
                fi

                # Extract label names from PromQL queries
                # Labels appear in format: {label="value"} or {label=~"regex"}
                # Use grep to find label= patterns
                used_labels=$(echo "$prometheus_exprs" | \
                    grep -oE '\{[^}]+\}' | \
                    grep -oE '[a-zA-Z_][a-zA-Z0-9_]*\s*[=!]' | \
                    sed 's/[=!].*$//' | \
                    sed 's/\s*$//' | \
                    sort -u || true)

                if [ -n "$used_labels" ]; then
                    for label in $used_labels; do
                        # Only validate infrastructure labels (Kubernetes/Docker labels)
                        # Don't validate application-specific metric labels like 'status', 'error_type', etc.

                        # Known problematic labels to check (infrastructure-related)
                        case "$label" in
                            # Docker-specific labels (invalid in Kubernetes)
                            name|container_name|image)
                                if ! echo "$valid_prometheus_labels" | grep -q "^${label}$"; then
                                    echo -e "${YELLOW}INFO:${NC} Dashboard uses Docker label: '$label' (will not work in Kubernetes)"
                                    echo "  -> Dashboard: $dashboard_name"
                                    echo "  -> Docker label detected - use Kubernetes equivalent"
                                    if [ "$label" = "name" ]; then
                                        echo "  -> Replace 'name' with 'pod' for Kubernetes"
                                    fi
                                    echo "  -> Valid Kubernetes labels: $(echo "$valid_prometheus_labels" | tr '\n' ', ' | sed 's/,$//')"
                                    echo "  -> Note: This is informational only - not blocking guards yet"
                                    # Don't increment violations yet - this is informational
                                    # increment_violations
                                    prometheus_issues_found=true
                                fi
                                ;;
                            # Only warn about core Kubernetes labels if they're used but not configured
                            namespace|pod|node|container|service|endpoint)
                                if ! echo "$valid_prometheus_labels" | grep -q "^${label}$"; then
                                    echo -e "${YELLOW}WARNING:${NC} Dashboard uses Kubernetes label '$label' but Prometheus config may not expose it"
                                    echo "  -> Dashboard: $dashboard_name"
                                    echo "  -> Verify this label is available from your scrape configs"
                                fi
                                ;;
                            # Ignore application-specific labels (status, error_type, etc.) - these come from metrics themselves
                            *)
                                # Skip - application labels are valid
                                ;;
                        esac
                    done
                fi
            fi
        done

        if [ "$prometheus_issues_found" = false ]; then
            print_ok "All dashboard Prometheus queries use valid labels"
        else
            echo -e "${YELLOW}INFO:${NC} Some dashboards use Docker patterns (informational only, not blocking)"
        fi
        echo ""
    fi
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
    echo "To fix datasource UID issues:"
    echo "  Add the missing UID(s) to $DATASOURCE_CONFIG"
    echo "  Example:"
    echo "    - name: Prometheus"
    echo "      type: prometheus"
    echo "      uid: prometheus    # <-- Add this line"
    echo ""
    echo "To fix Loki label issues:"
    echo "  Update dashboard queries to use valid labels from Promtail config."
    echo "  Valid labels are defined in: $PROMTAIL_CONFIG"
    echo "  Common fix: Replace 'job=\"service-name\"' with 'app=\"service-name\"'"
    echo ""
    echo "To fix variable consistency issues:"
    echo "  Ensure variable name matches the Loki label it queries."
    echo "  Example: Variable 'pod' should query label 'pod', not 'container'"
    echo "  Update the query.label field in templating.list to match variable name."
    echo ""
    echo "To fix Prometheus query issues:"
    echo "  Replace Docker patterns with Kubernetes patterns."
    echo "  Docker: {name=~\"dark_tower_ac.*\"}"
    echo "  Kubernetes: {namespace=\"dark-tower\", pod=~\"ac-service.*\"}"
    echo "  Use labels from Prometheus config: $PROMETHEUS_CONFIG"
    echo ""
    exit 1
else
    echo -e "${GREEN}No violations found!${NC}"
    echo "Validated $dashboard_count dashboard(s) against datasource and Promtail configs"
    exit 0
fi
