#!/bin/bash
#
# Environment Variable Configuration Guard
#
# Validates consistency between Rust service config and K8s manifests:
#   1. Required env vars in config.rs are provided in deployment/statefulset manifests
#   2. configMapKeyRef keys in deployment manifests exist in the corresponding configmap
#   3. configmap keys are referenced by at least one deployment env var
#
# Exit codes:
#   0 - All validations passed (or skipped due to no relevant changes)
#   1 - Validation errors found
#   2 - Script error
#
# Usage:
#   ./validate-env-config.sh
#
# Note: This guard operates on fixed paths and does not accept a path argument.
#

set -euo pipefail

# Source common library
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/../common.sh"

REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"

# Directories
CRATES_DIR="$REPO_ROOT/crates"
SERVICES_DIR="$REPO_ROOT/infra/services"

# Initialize
init_violations
start_timer

DIFF_BASE=$(get_diff_base)

print_header "Guard: Environment Variable Configuration
Diff base: $DIFF_BASE (only runs if crates/ or infra/services/ changed)"

# =============================================================================
# Changed-file scoping: skip if no relevant changes
# =============================================================================

CHANGED_CRATES=$(get_all_changed_files "crates/" "")
CHANGED_INFRA=$(get_all_changed_files "infra/services/" "")

if [[ -z "$CHANGED_CRATES" && -z "$CHANGED_INFRA" ]]; then
    echo -e "${GREEN}No crates/ or infra/services/ files changed compared to ${DIFF_BASE}${NC}"
    print_elapsed_time
    exit 0
fi

echo "Detected relevant changes — running env config validation"
echo ""

# =============================================================================
# Build service list from CANONICAL_SERVICES
# =============================================================================

# Collect services that have both a crate config.rs and infra manifests
declare -A SVC_CRATE_DIR   # prefix -> crate directory name
declare -A SVC_INFRA_DIR   # prefix -> infra directory name

for prefix in "${!CANONICAL_SERVICES[@]}"; do
    IFS=':' read -r dir _app <<< "${CANONICAL_SERVICES[$prefix]}"

    config_file="$CRATES_DIR/$dir/src/config.rs"
    infra_dir="$SERVICES_DIR/$dir"

    # Skip services that don't have both a config.rs and infra directory
    if [[ ! -f "$config_file" ]]; then
        continue
    fi
    if [[ ! -d "$infra_dir" ]]; then
        continue
    fi

    SVC_CRATE_DIR[$prefix]="$dir"
    SVC_INFRA_DIR[$prefix]="$dir"
done

if [[ ${#SVC_CRATE_DIR[@]} -eq 0 ]]; then
    echo "No services with both config.rs and infra manifests found"
    print_elapsed_time
    exit 0
fi

# =============================================================================
# Helper: find the workload manifest (deployment.yaml or statefulset.yaml)
# =============================================================================

find_workload_manifest() {
    local infra_dir="$1"
    if [[ -f "$infra_dir/deployment.yaml" ]]; then
        echo "$infra_dir/deployment.yaml"
    elif [[ -f "$infra_dir/statefulset.yaml" ]]; then
        echo "$infra_dir/statefulset.yaml"
    else
        echo ""
    fi
}

# =============================================================================
# Check 1: Required env vars in config.rs are provided in workload manifest
# =============================================================================

print_section "Check 1: Required env vars provided in K8s manifests"

c1_errors=0

for prefix in "${!SVC_CRATE_DIR[@]}"; do
    dir="${SVC_CRATE_DIR[$prefix]}"
    config_file="$CRATES_DIR/$dir/src/config.rs"
    infra_dir="$SERVICES_DIR/${SVC_INFRA_DIR[$prefix]}"
    workload=$(find_workload_manifest "$infra_dir")

    if [[ -z "$workload" ]]; then
        print_warning "$dir: no deployment.yaml or statefulset.yaml found — skipping"
        continue
    fi

    # Extract required env var names from MissingEnvVar("VAR_NAME")
    required_vars=$(grep -oP 'MissingEnvVar\("\K[A-Z_]+' "$config_file" | sort -u || true)

    if [[ -z "$required_vars" ]]; then
        continue
    fi

    # Extract env var names declared in the workload manifest
    # Matches lines like "        - name: VAR_NAME"
    declared_vars=$(grep -oP '^\s+-\s+name:\s+\K[A-Z_]+' "$workload" | sort -u || true)

    while IFS= read -r var; do
        [[ -z "$var" ]] && continue

        if ! echo "$declared_vars" | grep -qx "$var"; then
            print_violation "$dir: config.rs requires $var but it is not in $(basename "$workload")"
            increment_violations
            ((c1_errors++)) || true
        fi
    done <<< "$required_vars"
done

if [[ $c1_errors -eq 0 ]]; then
    print_ok "All required env vars are provided in K8s manifests"
fi
echo ""

# =============================================================================
# Check 2: configMapKeyRef keys exist in the corresponding configmap
# =============================================================================

print_section "Check 2: configMapKeyRef keys exist in configmap"

c2_errors=0

for prefix in "${!SVC_INFRA_DIR[@]}"; do
    dir="${SVC_INFRA_DIR[$prefix]}"
    infra_dir="$SERVICES_DIR/$dir"
    workload=$(find_workload_manifest "$infra_dir")
    configmap="$infra_dir/configmap.yaml"

    if [[ -z "$workload" ]]; then
        continue
    fi

    if [[ ! -f "$configmap" ]]; then
        # Check if any configMapKeyRef exists — if so, the configmap is missing
        if grep -q 'configMapKeyRef' "$workload"; then
            print_violation "$dir: workload references configMapKeyRef but no configmap.yaml exists"
            increment_violations
            ((c2_errors++)) || true
        fi
        continue
    fi

    # Extract configMapKeyRef key values from the workload manifest
    # Pattern: configMapKeyRef followed by name + key lines
    referenced_keys=$(grep -A2 'configMapKeyRef' "$workload" | \
        grep -oP '^\s+key:\s+\K\S+' | sort -u || true)

    if [[ -z "$referenced_keys" ]]; then
        continue
    fi

    # Extract keys defined in the configmap data section
    configmap_keys=$(awk '/^data:/{found=1; next} /^[a-zA-Z]/{found=0} found && /^\s+[A-Z_]+:/{print $1}' "$configmap" | \
        sed 's/://' | sort -u || true)

    while IFS= read -r key; do
        [[ -z "$key" ]] && continue

        if ! echo "$configmap_keys" | grep -qx "$key"; then
            print_violation "$dir: $(basename "$workload") references configMapKeyRef key '$key' but it is not in configmap.yaml"
            increment_violations
            ((c2_errors++)) || true
        fi
    done <<< "$referenced_keys"
done

if [[ $c2_errors -eq 0 ]]; then
    print_ok "All configMapKeyRef keys exist in their configmaps"
fi
echo ""

# =============================================================================
# Check 3: configmap keys are referenced by the workload
# =============================================================================

print_section "Check 3: No orphan configmap keys"

c3_errors=0

for prefix in "${!SVC_INFRA_DIR[@]}"; do
    dir="${SVC_INFRA_DIR[$prefix]}"
    infra_dir="$SERVICES_DIR/$dir"
    workload=$(find_workload_manifest "$infra_dir")
    configmap="$infra_dir/configmap.yaml"

    if [[ -z "$workload" || ! -f "$configmap" ]]; then
        continue
    fi

    # Extract keys defined in the configmap data section
    configmap_keys=$(awk '/^data:/{found=1; next} /^[a-zA-Z]/{found=0} found && /^\s+[A-Z_]+:/{print $1}' "$configmap" | \
        sed 's/://' | sort -u || true)

    if [[ -z "$configmap_keys" ]]; then
        continue
    fi

    # Extract configMapKeyRef key values from the workload manifest
    referenced_keys=$(grep -A2 'configMapKeyRef' "$workload" | \
        grep -oP '^\s+key:\s+\K\S+' | sort -u || true)

    while IFS= read -r key; do
        [[ -z "$key" ]] && continue

        if ! echo "$referenced_keys" | grep -qx "$key"; then
            print_violation "$dir: configmap.yaml defines '$key' but it is not referenced in $(basename "$workload")"
            increment_violations
            ((c3_errors++)) || true
        fi
    done <<< "$configmap_keys"
done

if [[ $c3_errors -eq 0 ]]; then
    print_ok "All configmap keys are referenced by their workloads"
fi
echo ""

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
    echo -e "${GREEN}All environment variable configuration checks passed!${NC}"
    exit 0
fi
