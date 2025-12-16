#!/usr/bin/env bash
#
# Teardown script for Dark Tower local development environment
#
# This script removes the kind cluster and cleans up resources.
#
# Usage:
#   ./infra/kind/scripts/teardown.sh
#
# See ADR-0013 for the single-tier development environment strategy.

set -euo pipefail

CLUSTER_NAME="dark-tower"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Main
main() {
    log_info "Tearing down Dark Tower kind environment..."

    if ! command -v kind &> /dev/null; then
        log_error "kind is not installed."
        exit 1
    fi

    if kind get clusters 2>/dev/null | grep -q "^${CLUSTER_NAME}$"; then
        log_info "Deleting kind cluster '${CLUSTER_NAME}'..."
        kind delete cluster --name "${CLUSTER_NAME}"
        log_info "Cluster deleted successfully."
    else
        log_warn "Cluster '${CLUSTER_NAME}' does not exist."
    fi

    # Clean up any orphaned port-forward processes
    log_info "Cleaning up port-forward processes..."
    pkill -f "kubectl port-forward.*dark-tower" 2>/dev/null || true

    log_info "Teardown complete."
    echo ""
    echo "To recreate the cluster:"
    echo "  ./infra/kind/scripts/setup.sh"
    echo ""
}

main "$@"
