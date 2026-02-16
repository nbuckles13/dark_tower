#!/bin/bash
#
# Application Metrics Validation Guard
#
# Validates that:
# 1. All services with metrics.rs are in the canonical mapping
# 2. Metrics use the correct prefix for their service
# 3. Dashboard application metrics exist in source code
# 4. All defined metrics are monitored (optional warning)
#
# Source of truth: crates/*/src/observability/metrics.rs files
#
# Exit codes:
#   0 - All validations passed
#   1 - Validation errors found
#   2 - Script error

set -euo pipefail

# Script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

# Source common library
source "$SCRIPT_DIR/common.sh"

# Configuration
CRATES_DIR="$REPO_ROOT/crates"
DASHBOARDS_DIR="$REPO_ROOT/infra/grafana/dashboards"

# Colors
RED='\033[0;31m'
YELLOW='\033[1;33m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m'

# ============================================================================
# CANONICAL SERVICE MAPPING
#
# This is the source of truth for service structure.
# When adding a new service, you MUST add it here.
#
# Format: [prefix]="directory:app-label"
# - prefix: Metric prefix (e.g., "ac" for ac_*)
# - directory: Crate directory name (e.g., "ac-service")
# - app-label: Kubernetes app label (e.g., "ac-service")
# ============================================================================

declare -A CANONICAL_SERVICES=(
    [ac]="ac-service:ac-service"
    [gc]="gc-service:gc-service"
    [mc]="mc-service:mc-service"
    [mh]="mh-service:mh-service"
)

# Parse into separate arrays
declare -A SERVICE_DIRS
declare -A SERVICE_APPS
declare -A SERVICE_METRICS

for prefix in "${!CANONICAL_SERVICES[@]}"; do
    IFS=':' read -r dir app <<< "${CANONICAL_SERVICES[$prefix]}"
    SERVICE_DIRS[$prefix]="$dir"
    SERVICE_APPS[$prefix]="$app"
done

# ============================================================================
# Step 1: Auto-discover services and validate against canonical mapping
# ============================================================================

validate_service_registration() {
    echo -e "${BLUE}Validating service registration...${NC}"

    local errors=0

    # Find all metrics.rs files in crates directory only
    local found_services=()
    while IFS= read -r metrics_file; do
        # Extract directory name: crates/gc-service/... → gc-service
        # Make path relative to repo root first
        metrics_file="${metrics_file#$REPO_ROOT/}"
        local dir_name
        dir_name=$(echo "$metrics_file" | cut -d'/' -f2)
        found_services+=("$dir_name")

        # Check if this directory is in our canonical mapping
        local found=false
        for prefix in "${!SERVICE_DIRS[@]}"; do
            if [[ "${SERVICE_DIRS[$prefix]}" == "$dir_name" ]]; then
                found=true
                break
            fi
        done

        if ! $found; then
            echo -e "${RED}❌ ERROR: Found service directory '$dir_name' with src/observability/metrics.rs${NC}"
            echo "   but it's not in the canonical mapping table!"
            echo ""
            echo "   Add to CANONICAL_SERVICES in $(basename "$0"):"
            echo "   [??]=\"$dir_name:app-label-here\""
            echo ""
            echo "   where [??] is the metric prefix (e.g., 'mh' for mh_*)"
            echo ""
            ((errors++))
        fi
    done < <(find "$CRATES_DIR" -path "*/src/observability/metrics.rs" 2>/dev/null)

    if [[ $errors -gt 0 ]]; then
        return 1
    fi

    echo -e "${GREEN}✓ All services properly registered${NC}"
    return 0
}

# ============================================================================
# Step 2: Extract metrics from source code and validate prefix correctness
# ============================================================================

extract_and_validate_metrics() {
    echo -e "${BLUE}Extracting and validating metrics from source code...${NC}"

    local errors=0

    for prefix in "${!SERVICE_DIRS[@]}"; do
        local dir_name="${SERVICE_DIRS[$prefix]}"
        local metrics_file="$CRATES_DIR/${dir_name}/src/observability/metrics.rs"

        if [[ ! -f "$metrics_file" ]]; then
            echo -e "${YELLOW}⚠️  WARNING: Expected metrics file for '${prefix}' service${NC}"
            echo "   at $metrics_file but it doesn't exist"
            echo ""
            continue
        fi

        # Extract metrics using grep
        # Looking for: counter!("metric_name", ...)
        #              histogram!("metric_name", ...)
        #              gauge!("metric_name", ...)
        local metrics
        metrics=$(grep -oP '(?:counter|histogram|gauge)!\s*\(\s*"([^"]+)"' "$metrics_file" | grep -oP '"[^"]+"' | tr -d '"' | sort -u || true)

        if [[ -z "$metrics" ]]; then
            echo -e "${YELLOW}⚠️  WARNING: No metrics found in ${dir_name}/src/observability/metrics.rs${NC}"
            echo ""
            continue
        fi

        # Validate each metric uses the correct prefix
        while IFS= read -r metric; do
            [[ -z "$metric" ]] && continue

            local metric_prefix
            metric_prefix=$(echo "$metric" | cut -d'_' -f1)

            if [[ "$metric_prefix" != "$prefix" ]]; then
                echo -e "${RED}❌ ERROR: Metric '$metric' in $metrics_file${NC}"
                echo "   uses prefix '${metric_prefix}_' but should use '${prefix}_'"
                echo "   (based on directory: crates/${dir_name}/)"
                echo ""
                ((errors++))
            fi
        done <<< "$metrics"

        # Store metrics for this service
        SERVICE_METRICS[$prefix]="$metrics"
    done

    if [[ $errors -gt 0 ]]; then
        return 1
    fi

    echo -e "${GREEN}✓ All metrics use correct prefixes${NC}"
    return 0
}

# ============================================================================
# Step 3: Validate dashboard queries against source code metrics
# ============================================================================

validate_dashboard_metrics() {
    echo -e "${BLUE}Validating dashboard metrics against source code...${NC}"

    local errors=0

    # Build a lookup map of all metrics
    declare -A all_metrics
    for prefix in "${!SERVICE_METRICS[@]}"; do
        while IFS= read -r metric; do
            [[ -z "$metric" ]] && continue
            all_metrics[$metric]="$prefix"
        done <<< "${SERVICE_METRICS[$prefix]}"
    done

    # Check each dashboard
    for dashboard_file in "$DASHBOARDS_DIR"/*.json; do
        [[ ! -f "$dashboard_file" ]] && continue

        local dashboard_name
        dashboard_name=$(basename "$dashboard_file")

        # Extract application metric queries (ac_*, gc_*, mc_*, mh_*)
        # Using jq if available, otherwise grep
        local queries
        if command -v jq &> /dev/null; then
            queries=$(jq -r '.. | .expr? // empty' "$dashboard_file" 2>/dev/null | grep -oP '\b(ac|gc|mc|mh)_[a-z_]+' | sort -u || true)
        else
            queries=$(grep -oP '\b(ac|gc|mc|mh)_[a-z_]+' "$dashboard_file" | sort -u || true)
        fi

        while IFS= read -r metric; do
            [[ -z "$metric" ]] && continue

            # Check if metric exists in any service
            # OR if it's a histogram suffix (_bucket, _count, _sum) of an existing histogram
            metric_exists=false

            if [[ -v all_metrics[$metric] ]]; then
                metric_exists=true
            else
                # Check if this is a histogram-generated metric (_bucket, _count, _sum)
                for suffix in "_bucket" "_count" "_sum"; do
                    if [[ "$metric" =~ $suffix$ ]]; then
                        # Strip suffix and check if base metric exists
                        base_metric="${metric%$suffix}"
                        if [[ -v all_metrics[$base_metric] ]]; then
                            metric_exists=true
                            break
                        fi
                    fi
                done
            fi

            if ! $metric_exists; then
                local prefix
                prefix=$(echo "$metric" | cut -d'_' -f1)

                echo -e "${RED}❌ ERROR: Dashboard '$dashboard_name' uses metric '$metric'${NC}"
                echo "   which is not defined in crates/${SERVICE_DIRS[$prefix]}/src/observability/metrics.rs"
                echo ""

                # Find similar metrics (fuzzy matching)
                local similar=""
                if [[ -v SERVICE_METRICS[$prefix] ]] && [[ -n "${SERVICE_METRICS[$prefix]}" ]]; then
                    # Strip histogram suffixes for matching
                    search_pattern=$(echo "$metric" | sed -E 's/_(bucket|count|sum)$//' | sed 's/_[^_]*$//')
                    similar=$(echo "${SERVICE_METRICS[$prefix]}" | tr ' ' '\n' | grep -i "$search_pattern" | head -3 || true)
                fi
                if [[ -n "$similar" ]]; then
                    echo "   Similar metrics found:"
                    while IFS= read -r sim; do
                        [[ -z "$sim" ]] && continue
                        echo "   - $sim"
                    done <<< "$similar"
                    echo ""
                fi

                ((errors++))
            fi
        done <<< "$queries"
    done

    if [[ $errors -gt 0 ]]; then
        return 1
    fi

    echo -e "${GREEN}✓ All dashboard metrics exist in source code${NC}"
    return 0
}

# ============================================================================
# Step 4: Check metric coverage (optional warning)
# ============================================================================

check_metric_coverage() {
    echo -e "${BLUE}Checking metric coverage in dashboards...${NC}"

    local warnings=0

    # Build list of all metrics used in dashboards
    declare -A dashboard_metrics
    for dashboard_file in "$DASHBOARDS_DIR"/*.json; do
        [[ ! -f "$dashboard_file" ]] && continue

        local queries
        if command -v jq &> /dev/null; then
            queries=$(jq -r '.. | .expr? // empty' "$dashboard_file" 2>/dev/null | grep -oP '\b(ac|gc|mc|mh)_[a-z_]+' | sort -u || true)
        else
            queries=$(grep -oP '\b(ac|gc|mc|mh)_[a-z_]+' "$dashboard_file" | sort -u || true)
        fi

        while IFS= read -r metric; do
            [[ -z "$metric" ]] && continue
            dashboard_metrics[$metric]=1
        done <<< "$queries"
    done

    # Check each defined metric
    for prefix in "${!SERVICE_METRICS[@]}"; do
        if [[ ! -v SERVICE_METRICS[$prefix] ]] || [[ -z "${SERVICE_METRICS[$prefix]}" ]]; then
            continue
        fi

        while IFS= read -r metric; do
            [[ -z "$metric" ]] && continue

            if [[ ! -v dashboard_metrics[$metric] ]]; then
                echo -e "${YELLOW}⚠️  Metric '$metric' defined in ${SERVICE_DIRS[$prefix]}/src/observability/metrics.rs${NC}"
                echo "   but not used in any dashboard"
                echo "   Consider: adding to dashboard or marking as internal-only"
                echo ""
                ((warnings++)) || true
            fi
        done <<< "${SERVICE_METRICS[$prefix]}"
    done

    if [[ $warnings -gt 0 ]]; then
        echo -e "${YELLOW}Found $warnings metric(s) without dashboard coverage${NC}"
    else
        echo -e "${GREEN}✓ All metrics are monitored in dashboards${NC}"
    fi

    return 0  # Warnings don't fail the guard
}

# ============================================================================
# Main
# ============================================================================

main() {
    echo ""
    echo "========================================="
    echo "Application Metrics Validation"
    echo "========================================="
    echo ""

    local exit_code=0

    # Step 1: Validate service registration
    if ! validate_service_registration; then
        exit_code=1
    fi

    echo ""

    # Step 2: Extract and validate metrics
    if ! extract_and_validate_metrics; then
        exit_code=1
    fi

    echo ""

    # Step 3: Validate dashboard metrics
    if ! validate_dashboard_metrics; then
        exit_code=1
    fi

    echo ""

    # Step 4: Check coverage (warnings only)
    check_metric_coverage

    echo ""
    echo "========================================="

    if [[ $exit_code -eq 0 ]]; then
        echo -e "${GREEN}✓ Application metrics validation passed${NC}"
    else
        echo -e "${RED}✗ Application metrics validation failed${NC}"
    fi

    echo "========================================="
    echo ""

    exit $exit_code
}

main "$@"
