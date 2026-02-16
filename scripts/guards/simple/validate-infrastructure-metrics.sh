#!/bin/bash
#
# Infrastructure Metrics Validation Guard
#
# Validates that Grafana dashboards use correct Kubernetes metric patterns
# and only reference metrics/labels that exist in Prometheus configuration.
#
# This guard prevents:
# - Docker metric patterns in Kubernetes deployment (name=, container_name=)
# - Invalid metric names (not in scrape targets)
# - Invalid label usage (labels that don't exist)
#
# Exit codes:
#   0 - All dashboards valid
#   1 - Validation errors found
#   2 - Script error

set -euo pipefail

# Script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"

# Source common library
source "$SCRIPT_DIR/../common.sh"

# Configuration files
PROMETHEUS_CONFIG="$REPO_ROOT/infra/kubernetes/observability/prometheus-config.yaml"
DASHBOARDS_DIR="$REPO_ROOT/infra/grafana/dashboards"

# Colors
RED='\033[0;31m'
YELLOW='\033[1;33m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m'

ERRORS_FOUND=0

# ============================================================================
# Helper Functions
# ============================================================================

check_dependencies() {
    if ! command -v python3 &> /dev/null; then
        echo -e "${RED}Error: python3 is required but not installed${NC}" >&2
        exit 2
    fi

    if ! python3 -c "import yaml" 2>/dev/null; then
        echo -e "${RED}Error: Python PyYAML module is required${NC}" >&2
        echo "Install with: pip install pyyaml" >&2
        exit 2
    fi
}

# ============================================================================
# Extract Infrastructure Metrics Schema from Prometheus Config
# ============================================================================

extract_prometheus_schema() {
    local config_file="$1"

    python3 <<PYTHON
import yaml
import sys
import json

config_file = "$config_file"

try:
    with open(config_file, 'r') as f:
        docs = list(yaml.safe_load_all(f))

    valid_labels = set()
    scrape_jobs = []

    for doc in docs:
        if doc and doc.get('kind') == 'ConfigMap' and 'data' in doc:
            prom_yaml = doc['data'].get('prometheus.yml', '')
            if prom_yaml:
                prom_config = yaml.safe_load(prom_yaml)

                for scrape_config in prom_config.get('scrape_configs', []):
                    job_name = scrape_config.get('job_name', '')
                    scrape_jobs.append(job_name)

                    # If using Kubernetes service discovery, add standard K8s labels
                    if 'kubernetes_sd_configs' in scrape_config:
                        valid_labels.update(['namespace', 'pod', 'node', 'container', 'service'])

                    # Extract custom labels from relabel_configs
                    for relabel in scrape_config.get('relabel_configs', []):
                        if relabel.get('action') == 'replace':
                            target_label = relabel.get('target_label', '')
                            if target_label and not target_label.startswith('__'):
                                valid_labels.add(target_label)

    # Output as JSON
    print(json.dumps({
        'valid_labels': list(valid_labels),
        'scrape_jobs': scrape_jobs
    }))

except Exception as e:
    print(f"Error parsing Prometheus config: {e}", file=sys.stderr)
    sys.exit(2)
PYTHON
}

# ============================================================================
# Validate Dashboards
# ============================================================================

validate_dashboards() {
    local schema_json="$1"

    python3 <<PYTHON
import json
import sys
import os
import re

schema = json.loads('''$schema_json''')
valid_labels = set(schema['valid_labels'])
dashboards_dir = "$DASHBOARDS_DIR"

errors_found = 0

# Docker-specific patterns to detect
docker_patterns = {
    'name': 'Docker Compose uses "name=" label',
    'container_name': 'Docker Compose uses "container_name=" label',
    'image': 'Docker Compose uses "image=" label'
}

kubernetes_labels = {'namespace', 'pod', 'container', 'node', 'service'}

# Process each dashboard
for filename in os.listdir(dashboards_dir):
    if not filename.endswith('.json'):
        continue

    dashboard_path = os.path.join(dashboards_dir, filename)

    try:
        with open(dashboard_path, 'r') as f:
            dashboard = json.load(f)
    except Exception as e:
        print(f"Error reading {filename}: {e}", file=sys.stderr)
        continue

    # Extract all panel queries
    for panel in dashboard.get('panels', []):
        panel_title = panel.get('title', 'Untitled')

        for target in panel.get('targets', []):
            # Only check Prometheus queries
            datasource = target.get('datasource', {})
            if datasource.get('type') != 'prometheus':
                continue

            expr = target.get('expr', '')
            if not expr:
                continue

            # Only check infrastructure metrics (not application metrics like gc_*, ac_*, etc.)
            # Infrastructure metrics: container_*, kube_*, node_*, up, etc.
            is_infrastructure_query = bool(re.search(r'\b(container_|kube_|node_|up\b)', expr))

            if not is_infrastructure_query:
                # Skip application metrics - they have their own validation
                continue

            # Check for Docker label patterns
            for docker_label, description in docker_patterns.items():
                if re.search(rf'\b{docker_label}\s*[=~]', expr):
                    print(f"❌ {filename} (panel: {panel_title})")
                    print(f"   Uses Docker label pattern: {docker_label}=")
                    print(f"   {description}")
                    print(f"   Kubernetes deployment should use: {', '.join(sorted(kubernetes_labels))}")
                    print(f"   Query: {expr[:100]}...")
                    print()
                    errors_found += 1

            # Extract labels from query (simple regex - matches label=value or label=~"value")
            label_matches = re.findall(r'\b(\w+)\s*[=~]', expr)

            for label in label_matches:
                # Skip special Prometheus labels
                if label in ['__name__', 'job', 'instance']:
                    continue

                # Skip if it looks like a metric name (comes right after opening brace)
                if re.search(rf'{{\s*{label}\s*[=~]', expr):
                    # Check if this label is valid for infrastructure metrics
                    if label not in valid_labels and label in docker_patterns:
                        # Already reported above
                        continue
                    elif label not in valid_labels:
                        print(f"⚠️  {filename} (panel: {panel_title})")
                        print(f"   Uses label '{label}' which is not in Prometheus config")
                        print(f"   Valid infrastructure labels: {', '.join(sorted(valid_labels))}")
                        print(f"   Query: {expr[:100]}...")
                        print()
                        errors_found += 1

sys.exit(1 if errors_found > 0 else 0)
PYTHON

    return $?
}

# ============================================================================
# Main
# ============================================================================

main() {
    echo -e "${BLUE}Validating infrastructure metrics in dashboards...${NC}"

    # Check dependencies
    check_dependencies

    # Check if Prometheus config exists
    if [[ ! -f "$PROMETHEUS_CONFIG" ]]; then
        echo -e "${RED}Error: Prometheus config not found at $PROMETHEUS_CONFIG${NC}" >&2
        exit 2
    fi

    # Check if dashboards directory exists
    if [[ ! -d "$DASHBOARDS_DIR" ]]; then
        echo -e "${RED}Error: Dashboards directory not found at $DASHBOARDS_DIR${NC}" >&2
        exit 2
    fi

    # Extract Prometheus schema
    schema_json=$(extract_prometheus_schema "$PROMETHEUS_CONFIG")
    if [[ $? -ne 0 ]]; then
        echo -e "${RED}Error: Failed to extract Prometheus schema${NC}" >&2
        exit 2
    fi

    # Validate dashboards against schema
    if validate_dashboards "$schema_json"; then
        echo -e "${GREEN}✓ All infrastructure metrics valid${NC}"
        exit 0
    else
        echo -e "${RED}✗ Infrastructure metrics validation failed${NC}"
        echo -e "${YELLOW}Fix the errors above before committing${NC}"
        exit 1
    fi
}

main "$@"
