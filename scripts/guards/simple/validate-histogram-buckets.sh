#!/bin/bash
#
# Histogram Bucket Configuration Guard
#
# Validates that every histogram!() defined in a service's metrics.rs has a
# matching set_buckets_for_metric() call IN THE SAME FILE.
#
# Co-location is enforced: bucket config must live next to the metric
# definitions so they stay in sync. See ADR-0011.
#
# Without explicit bucket configuration, histograms use the metrics crate's
# default buckets which are NOT aligned with SLO targets.
#
# Exit codes:
#   0 - All histograms have bucket configuration
#   1 - Unconfigured histograms found
#   2 - Script error

set -euo pipefail

# Script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"

# Source common library (CANONICAL_SERVICES)
source "$SCRIPT_DIR/../common.sh"

# Configuration
CRATES_DIR="$REPO_ROOT/crates"

# Colors
RED='\033[0;31m'
YELLOW='\033[1;33m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m'

# ============================================================================
# Main
# ============================================================================

main() {
    echo ""
    echo "========================================="
    echo "Histogram Bucket Configuration Validation"
    echo "========================================="
    echo ""

    local exit_code=0
    local total_histograms=0
    local unconfigured_histograms=0

    for prefix in "${!CANONICAL_SERVICES[@]}"; do
        IFS=':' read -r dir_name app_label <<< "${CANONICAL_SERVICES[$prefix]}"
        local crate_dir="$CRATES_DIR/$dir_name"
        local metrics_file="$crate_dir/src/observability/metrics.rs"

        if [[ ! -f "$metrics_file" ]]; then
            continue
        fi

        # Extract histogram metric names
        local histogram_names
        histogram_names=$(grep -oP 'histogram!\s*\(\s*"([^"]+)"' "$metrics_file" | grep -oP '"[^"]+"' | tr -d '"' | sort -u || true)

        if [[ -z "$histogram_names" ]]; then
            continue
        fi

        # Extract configured bucket prefixes from the SAME metrics.rs file
        local bucket_prefixes
        bucket_prefixes=$(grep -oP 'Matcher::Prefix\(\s*"([^"]+)"' "$metrics_file" | grep -oP '"[^"]+"' | tr -d '"' | sort -u || true)

        echo -e "${BLUE}Service: ${prefix} (${dir_name})${NC}"

        while IFS= read -r metric_name; do
            [[ -z "$metric_name" ]] && continue
            ((total_histograms++)) || true

            # Check if any bucket prefix matches this metric name
            local matched=false
            if [[ -n "$bucket_prefixes" ]]; then
                while IFS= read -r bp; do
                    [[ -z "$bp" ]] && continue
                    if [[ "$metric_name" == "$bp"* ]]; then
                        matched=true
                        break
                    fi
                done <<< "$bucket_prefixes"
            fi

            if $matched; then
                echo -e "  ${GREEN}✓${NC} $metric_name"
            else
                echo -e "  ${RED}✗${NC} $metric_name — no set_buckets_for_metric() configured"
                ((unconfigured_histograms++)) || true
                exit_code=1
            fi
        done <<< "$histogram_names"

        echo ""
    done

    echo "========================================="
    echo "Total histograms:       $total_histograms"
    echo "Configured:             $((total_histograms - unconfigured_histograms))"
    echo -e "Unconfigured:           ${unconfigured_histograms}"

    if [[ $exit_code -eq 0 ]]; then
        echo -e "${GREEN}✓ All histograms have bucket configuration${NC}"
    else
        echo -e "${RED}✗ $unconfigured_histograms histogram(s) missing bucket configuration${NC}"
        echo ""
        echo "Fix: Add set_buckets_for_metric(Matcher::Prefix(\"<prefix>\"), ...)"
        echo "     in the SAME metrics.rs file where the histogram is defined."
    fi

    echo "========================================="
    echo ""

    exit $exit_code
}

main "$@"
