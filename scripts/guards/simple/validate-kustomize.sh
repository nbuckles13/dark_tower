#!/bin/bash
#
# Kustomize Validation Guard
#
# Validates Kustomize infrastructure for the Dark Tower project:
#   R-15: kustomize build succeeds for all bases and overlays
#   R-16: No orphan manifests in service directories
#   R-17: kubeconform schema validation (optional)
#   R-18: Security contexts preserved in build output
#   R-19: No empty secret values in build output
#   R-20: All dashboard JSONs listed in configMapGenerator (bidirectional)
#
# Exit codes:
#   0 - All validations passed (or skipped due to no infra/ changes)
#   1 - Validation errors found
#   2 - Script error
#
# Usage:
#   ./validate-kustomize.sh
#
# Note: This guard operates on fixed paths and does not accept a path argument.
#

set -euo pipefail

# Source common library
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/../common.sh"

REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"

# Directories
SERVICES_DIR="$REPO_ROOT/infra/services"
OVERLAYS_DIR="$REPO_ROOT/infra/kubernetes/overlays/kind"
OBS_BASE_DIR="$REPO_ROOT/infra/kubernetes/observability"
DASHBOARDS_DIR="$REPO_ROOT/infra/grafana/dashboards"
GRAFANA_KUSTOMIZATION="$REPO_ROOT/infra/grafana/kustomization.yaml"

# Build service lists from CANONICAL_SERVICES (common.sh) + infrastructure services
SERVICE_BASES=()
for prefix in "${!CANONICAL_SERVICES[@]}"; do
    IFS=':' read -r dir _app <<< "${CANONICAL_SERVICES[$prefix]}"
    # Only include services that have infra/services/ directories
    if [[ -d "$SERVICES_DIR/$dir" ]]; then
        SERVICE_BASES+=("$dir")
    fi
done
# Infrastructure services (no crate, not in CANONICAL_SERVICES)
SERVICE_BASES+=(postgres redis)

# Overlays mirror bases
SERVICE_OVERLAYS=("${SERVICE_BASES[@]}")

# Known exclusions for orphan manifest check (R-16)
# These files are intentionally NOT listed in kustomization.yaml resources
ORPHAN_EXCLUSIONS=(kustomization.yaml service-monitor.yaml)

# Temp directory for kustomize build output
TMPDIR_BUILD=""

# Initialize
init_violations
start_timer

DIFF_BASE=$(get_diff_base)

print_header "Guard: Kustomize Validation
Diff base: $DIFF_BASE (only runs if infra/ changed)"

# =============================================================================
# Changed-file scoping: skip if no infra/ changes
# =============================================================================

CHANGED_INFRA=$(get_all_changed_files "infra/" "")

if [[ -z "$CHANGED_INFRA" ]]; then
    echo -e "${GREEN}No infra/ files changed compared to ${DIFF_BASE}${NC}"
    print_elapsed_time
    exit 0
fi

echo "Detected infra/ changes — running kustomize validation"
echo ""

# =============================================================================
# Detect kustomize tool
# =============================================================================

KUSTOMIZE_CMD=()

if command -v kustomize &> /dev/null; then
    KUSTOMIZE_CMD=(kustomize build)
elif command -v kubectl &> /dev/null && kubectl kustomize --help &> /dev/null; then
    KUSTOMIZE_CMD=(kubectl kustomize)
fi

KUSTOMIZE_AVAILABLE=false
if [[ ${#KUSTOMIZE_CMD[@]} -gt 0 ]]; then
    KUSTOMIZE_AVAILABLE=true
fi

# Create temp directory for build output (cleaned up on exit)
# Combined trap: clean both TMPDIR_BUILD and VIOLATIONS_FILE (from init_violations)
TMPDIR_BUILD=$(mktemp -d)
trap "rm -rf $TMPDIR_BUILD; rm -f $VIOLATIONS_FILE" EXIT

# =============================================================================
# R-15: Validate kustomize build succeeds for all bases and overlays
# =============================================================================

print_section "R-15: Kustomize build validation"

run_kustomize_build() {
    local path="$1"
    local label="$2"
    local output_file="$3"
    local err_file="$TMPDIR_BUILD/stderr-$(basename "$path").txt"

    if "${KUSTOMIZE_CMD[@]}" "$path" > "$output_file" 2>"$err_file"; then
        print_ok "$label"
        return 0
    else
        print_violation "$label — kustomize build failed"
        if [[ -s "$err_file" ]]; then
            head -10 "$err_file" | sed 's/^/    /'
        fi
        increment_violations
        return 1
    fi
}

if [[ "$KUSTOMIZE_AVAILABLE" = false ]]; then
    print_warning "Neither kustomize nor kubectl kustomize available — skipping R-15, R-17, R-18, R-19"
    echo ""
else
    # Build service bases
    for svc in "${SERVICE_BASES[@]}"; do
        run_kustomize_build "$SERVICES_DIR/$svc" \
            "base: infra/services/$svc" \
            "$TMPDIR_BUILD/base-$svc.yaml" || true
    done

    # Build observability base
    run_kustomize_build "$OBS_BASE_DIR" \
        "base: infra/kubernetes/observability" \
        "$TMPDIR_BUILD/base-observability.yaml" || true

    # Build service overlays
    for svc in "${SERVICE_OVERLAYS[@]}"; do
        run_kustomize_build "$OVERLAYS_DIR/services/$svc" \
            "overlay: overlays/kind/services/$svc" \
            "$TMPDIR_BUILD/overlay-$svc.yaml" || true
    done

    # Build observability overlay
    run_kustomize_build "$OVERLAYS_DIR/observability" \
        "overlay: overlays/kind/observability" \
        "$TMPDIR_BUILD/overlay-observability.yaml" || true

    echo ""
fi

# =============================================================================
# R-16: No orphan manifests in service directories
# =============================================================================

print_section "R-16: Orphan manifest detection"

r16_errors=0

for svc in "${SERVICE_BASES[@]}"; do
    svc_dir="$SERVICES_DIR/$svc"
    kustomization_file="$svc_dir/kustomization.yaml"

    if [[ ! -f "$kustomization_file" ]]; then
        print_violation "infra/services/$svc/ missing kustomization.yaml"
        increment_violations
        ((r16_errors++)) || true
        continue
    fi

    # Extract resources from kustomization.yaml (lines matching "  - something.yaml")
    declared_resources=$(grep -E '^\s*-\s+\S+\.yaml' "$kustomization_file" | \
        sed 's/^\s*-\s*//' | sort || true)

    # Find all .yaml files in the service directory
    actual_files=$(find "$svc_dir" -maxdepth 1 -name '*.yaml' -exec basename {} \; | sort)

    # Check each actual file against declared resources and exclusions
    while IFS= read -r filename; do
        [[ -z "$filename" ]] && continue

        # Check if excluded
        is_excluded=false
        for excl in "${ORPHAN_EXCLUSIONS[@]}"; do
            if [[ "$filename" = "$excl" ]]; then
                is_excluded=true
                break
            fi
        done
        $is_excluded && continue

        # Check if declared
        if ! echo "$declared_resources" | grep -qx "$filename"; then
            print_violation "infra/services/$svc/$filename is not listed in kustomization.yaml resources"
            increment_violations
            ((r16_errors++)) || true
        fi
    done <<< "$actual_files"
done

if [[ $r16_errors -eq 0 ]]; then
    print_ok "No orphan manifests found"
fi
echo ""

# =============================================================================
# R-17: Kubeconform schema validation (optional)
# =============================================================================

print_section "R-17: Kubeconform schema validation"

if [[ "$KUSTOMIZE_AVAILABLE" = false ]]; then
    print_warning "Skipped — kustomize not available (no build output to validate)"
    echo ""
elif ! command -v kubeconform &> /dev/null; then
    print_warning "kubeconform not installed — skipping schema validation"
    echo ""
else
    r17_errors=0

    # Combine all build output and validate
    for build_file in "$TMPDIR_BUILD"/*.yaml; do
        [[ ! -s "$build_file" ]] && continue
        label=$(basename "$build_file" .yaml)

        if kubeconform -strict -summary "$build_file" 2>&1; then
            print_ok "kubeconform: $label"
        else
            print_violation "kubeconform: $label — schema validation failed"
            increment_violations
            ((r17_errors++)) || true
        fi
    done

    if [[ $r17_errors -eq 0 ]]; then
        print_ok "All kustomize build output passes schema validation"
    fi
    echo ""
fi

# =============================================================================
# R-18: Security context validation
# =============================================================================

print_section "R-18: Security context validation"

if [[ "$KUSTOMIZE_AVAILABLE" = false ]]; then
    print_warning "Skipped — kustomize not available (no build output to validate)"
    echo ""
else
    r18_errors=0

    # Process each base build output — split into individual YAML documents
    # and check Deployment/StatefulSet resources for security context fields
    check_security_contexts() {
        local build_file="$1"
        local source_label="$2"

        [[ ! -s "$build_file" ]] && return 0

        local doc_dir
        doc_dir=$(mktemp -d "$TMPDIR_BUILD/docs-XXXXXX")

        # Split multi-doc YAML into individual files
        awk -v dir="$doc_dir" '
            BEGIN { doc_num = 0 }
            /^---$/ { doc_num++; next }
            { print >> (dir "/doc-" doc_num ".yaml") }
        ' "$build_file"

        for doc_file in "$doc_dir"/doc-*.yaml; do
            [[ ! -f "$doc_file" ]] && continue

            # Check if this is a Deployment or StatefulSet
            local kind
            kind=$(grep -m1 '^kind:' "$doc_file" | awk '{print $2}' || true)

            if [[ "$kind" != "Deployment" && "$kind" != "StatefulSet" ]]; then
                continue
            fi

            local name
            name=$(grep -m1 '^\s*name:' "$doc_file" | head -1 | awk '{print $2}' || true)
            local resource_label="$kind/$name ($source_label)"

            # Check runAsNonRoot: true
            if ! grep -q 'runAsNonRoot: true' "$doc_file"; then
                print_violation "$resource_label missing runAsNonRoot: true"
                increment_violations
                ((r18_errors++)) || true
            fi

            # Check allowPrivilegeEscalation: false
            if ! grep -q 'allowPrivilegeEscalation: false' "$doc_file"; then
                print_violation "$resource_label missing allowPrivilegeEscalation: false"
                increment_violations
                ((r18_errors++)) || true
            fi

            # Check capabilities.drop includes ALL
            if ! grep -qE '^\s*-\s*"?ALL"?\s*$' "$doc_file" || \
               ! grep -q 'drop:' "$doc_file"; then
                print_violation "$resource_label missing capabilities.drop: [ALL]"
                increment_violations
                ((r18_errors++)) || true
            fi

            # Check readOnlyRootFilesystem: true (skip for workloads needing writable storage)
            if [[ "$name" != *postgres* && "$name" != *prometheus* && "$name" != *loki* && "$name" != *grafana* ]]; then
                if ! grep -q 'readOnlyRootFilesystem: true' "$doc_file"; then
                    print_violation "$resource_label missing readOnlyRootFilesystem: true"
                    increment_violations
                    ((r18_errors++)) || true
                fi
            fi
        done

        rm -rf "$doc_dir"
    }

    # Check all base build outputs
    for svc in "${SERVICE_BASES[@]}"; do
        check_security_contexts "$TMPDIR_BUILD/base-$svc.yaml" "infra/services/$svc"
    done

    # Also check observability base (includes Grafana deployment)
    check_security_contexts "$TMPDIR_BUILD/base-observability.yaml" "infra/kubernetes/observability"

    if [[ $r18_errors -eq 0 ]]; then
        print_ok "All Deployment/StatefulSet resources have required security contexts"
    fi
    echo ""
fi

# =============================================================================
# R-19: No empty secret values
# =============================================================================

print_section "R-19: No empty secret values"

if [[ "$KUSTOMIZE_AVAILABLE" = false ]]; then
    print_warning "Skipped — kustomize not available (no build output to validate)"
    echo ""
else
    r19_errors=0

    check_empty_secrets() {
        local build_file="$1"
        local source_label="$2"

        [[ ! -s "$build_file" ]] && return 0

        local doc_dir
        doc_dir=$(mktemp -d "$TMPDIR_BUILD/secrets-XXXXXX")

        # Split multi-doc YAML into individual files
        awk -v dir="$doc_dir" '
            BEGIN { doc_num = 0 }
            /^---$/ { doc_num++; next }
            { print >> (dir "/doc-" doc_num ".yaml") }
        ' "$build_file"

        for doc_file in "$doc_dir"/doc-*.yaml; do
            [[ ! -f "$doc_file" ]] && continue

            local kind
            kind=$(grep -m1 '^kind:' "$doc_file" | awk '{print $2}' || true)

            if [[ "$kind" != "Secret" ]]; then
                continue
            fi

            local secret_name
            secret_name=$(grep -m1 '^\s*name:' "$doc_file" | head -1 | awk '{print $2}' || true)

            # Check for empty values in data: and stringData: sections
            # Patterns: "key: ''" or 'key: ""' or "key:" with nothing after
            local in_data_section=false
            while IFS= read -r line; do
                # Detect data: or stringData: section start
                if [[ "$line" =~ ^(data|stringData):$ ]]; then
                    in_data_section=true
                    continue
                fi

                # Detect end of data section (non-indented line or new top-level key)
                if [[ "$in_data_section" = true ]] && [[ "$line" =~ ^[a-zA-Z] ]]; then
                    in_data_section=false
                    continue
                fi

                if [[ "$in_data_section" = true ]]; then
                    # Check for empty values: "  key: ''" or "  key: \"\"" or "  key: " (trailing space) or "  key:"
                    if [[ "$line" =~ ^[[:space:]]+([^:]+):[[:space:]]*(\"\"|\'\')?[[:space:]]*$ ]]; then
                        local key_name="${BASH_REMATCH[1]}"
                        # Only report key name, never the value
                        print_violation "Secret '$secret_name' ($source_label) has empty value for key '$key_name'"
                        increment_violations
                        ((r19_errors++)) || true
                    fi
                fi
            done < "$doc_file"
        done

        rm -rf "$doc_dir"
    }

    # Check all base build outputs
    for svc in "${SERVICE_BASES[@]}"; do
        check_empty_secrets "$TMPDIR_BUILD/base-$svc.yaml" "infra/services/$svc"
    done

    # Also check observability base (includes Grafana secret)
    check_empty_secrets "$TMPDIR_BUILD/base-observability.yaml" "infra/kubernetes/observability"

    if [[ $r19_errors -eq 0 ]]; then
        print_ok "No empty secret values found"
    fi
    echo ""
fi

# =============================================================================
# R-20: Dashboard JSON coverage in configMapGenerator (bidirectional)
# =============================================================================

print_section "R-20: Dashboard configMapGenerator coverage"

r20_errors=0

if [[ ! -d "$DASHBOARDS_DIR" ]]; then
    print_warning "Dashboard directory not found: $DASHBOARDS_DIR"
    echo ""
elif [[ ! -f "$GRAFANA_KUSTOMIZATION" ]]; then
    print_violation "Grafana kustomization not found: $GRAFANA_KUSTOMIZATION"
    increment_violations
    ((r20_errors++)) || true
    echo ""
else
    # Extract dashboard filenames from configMapGenerator files: entries
    # Format: "- key.json=../../../grafana/dashboards/filename.json"
    # We extract the basename from the path after '='
    declared_dashboards=$(grep -oE '[^/]+\.json$' "$GRAFANA_KUSTOMIZATION" | sort -u || true)

    # List actual dashboard JSON files. Skip copy-me template stubs
    # (`_template-*.json`) — these are starter files for service specialists
    # and must NOT be shipped into the Grafana ConfigMap.
    actual_dashboards=$(find "$DASHBOARDS_DIR" -maxdepth 1 -name '*.json' ! -name '_template-*.json' -exec basename {} \; | sort)

    # Direction 1: Check all actual dashboards are in configMapGenerator
    while IFS= read -r dashboard; do
        [[ -z "$dashboard" ]] && continue

        if ! echo "$declared_dashboards" | grep -qx "$dashboard"; then
            print_violation "Dashboard $dashboard exists in infra/grafana/dashboards/ but is not in configMapGenerator"
            increment_violations
            ((r20_errors++)) || true
        fi
    done <<< "$actual_dashboards"

    # Direction 2: Check all configMapGenerator references point to existing dashboards
    while IFS= read -r dashboard; do
        [[ -z "$dashboard" ]] && continue

        if [[ ! -f "$DASHBOARDS_DIR/$dashboard" ]]; then
            print_violation "configMapGenerator references $dashboard but file does not exist in infra/grafana/dashboards/"
            increment_violations
            ((r20_errors++)) || true
        fi
    done <<< "$declared_dashboards"

    if [[ $r20_errors -eq 0 ]]; then
        print_ok "All dashboard JSONs are listed in configMapGenerator and vice versa"
    fi
    echo ""
fi

# =============================================================================
# Summary
# =============================================================================

print_header "Summary"

TOTAL_VIOLATIONS=$(get_violations)
print_elapsed_time
echo ""

if [[ $TOTAL_VIOLATIONS -gt 0 ]]; then
    echo -e "${RED}Found $TOTAL_VIOLATIONS violation(s)${NC}"
    echo ""
    echo "Fix violations and re-run the guard."
    echo ""
    exit 1
else
    echo -e "${GREEN}All kustomize validations passed!${NC}"
    exit 0
fi
