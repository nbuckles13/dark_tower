#!/bin/bash
#
# iterate.sh - Telepresence-based local development for Dark Tower services
#
# This script allows you to run a service locally while connected to the kind cluster.
# Traffic that would go to the in-cluster service is routed to your local process.
#
# Usage: ./scripts/dev/iterate.sh <service>
#
# Services:
#   ac  - Auth Controller (port 8082)
#   gc  - Global Controller (port 8080)  [future]
#   mc  - Meeting Controller (port 8081) [future]
#   mh  - Media Handler (port 8083)      [future]
#

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log_info() { echo -e "${GREEN}[INFO]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }
log_step() { echo -e "${BLUE}[STEP]${NC} $1"; }

# Service configuration: key="statefulset:binary:port"
declare -A SERVICES=(
    ["ac"]="ac-service:auth-controller:8082"
    ["gc"]="global-controller:global-controller:8080"
    ["mc"]="meeting-controller:meeting-controller:8081"
    ["mh"]="media-handler:media-handler:8083"
)

NAMESPACE="dark-tower"
ORIGINAL_REPLICAS=2
STATEFULSET=""
BINARY=""
PORT=""
TELEPRESENCE_CONNECTED=false

# Find project root (where Cargo.toml lives)
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

usage() {
    echo "Usage: $0 <service>"
    echo ""
    echo "Run a Dark Tower service locally while connected to the kind cluster."
    echo "Traffic to the in-cluster service is routed to your local process."
    echo ""
    echo "Services:"
    echo "  ac  - Auth Controller (port 8082)"
    echo "  gc  - Global Controller (port 8080)  [skeleton]"
    echo "  mc  - Meeting Controller (port 8081) [skeleton]"
    echo "  mh  - Media Handler (port 8083)      [skeleton]"
    echo ""
    echo "Example:"
    echo "  $0 ac"
    echo ""
    echo "Prerequisites:"
    echo "  1. kind cluster running: ./infra/kind/scripts/setup.sh"
    echo "  2. Telepresence CLI installed: https://www.telepresence.io/docs/latest/install/"
    echo ""
    exit 1
}

cleanup() {
    local exit_code=$?
    echo ""
    log_step "Cleaning up..."

    # Leave telepresence intercept if connected
    if [[ "${TELEPRESENCE_CONNECTED}" == "true" ]]; then
        log_info "Leaving Telepresence intercept..."
        telepresence leave "${STATEFULSET}" 2>/dev/null || true
        telepresence quit 2>/dev/null || true
    fi

    # Scale statefulset back up
    if [[ -n "${STATEFULSET}" ]]; then
        log_info "Scaling ${STATEFULSET} back to ${ORIGINAL_REPLICAS} replicas..."
        kubectl scale statefulset/"${STATEFULSET}" -n "${NAMESPACE}" --replicas="${ORIGINAL_REPLICAS}" 2>/dev/null || true

        # Wait briefly for pods to start
        sleep 2
        log_info "Cluster state restored."
    fi

    if [[ ${exit_code} -eq 0 ]]; then
        log_info "Cleanup complete."
    else
        log_warn "Exited with code ${exit_code}. Cluster state has been restored."
    fi
}

check_prerequisites() {
    log_step "Checking prerequisites..."

    # Check telepresence CLI
    if ! command -v telepresence &> /dev/null; then
        log_error "Telepresence CLI not installed."
        echo ""
        echo "Install Telepresence from: https://www.telepresence.io/docs/latest/install/"
        echo ""
        echo "Quick install (Linux):"
        echo "  curl -fsSL https://app.getambassador.io/download/tel2/linux/amd64/latest/telepresence -o /tmp/telepresence"
        echo "  sudo install -m 755 /tmp/telepresence /usr/local/bin/telepresence"
        echo ""
        exit 1
    fi

    # Check kubectl
    if ! command -v kubectl &> /dev/null; then
        log_error "kubectl not installed."
        exit 1
    fi

    # Check kind cluster is running
    if ! kubectl cluster-info &> /dev/null; then
        log_error "Cannot connect to Kubernetes cluster."
        echo ""
        echo "Make sure the kind cluster is running:"
        echo "  ./infra/kind/scripts/setup.sh"
        echo ""
        exit 1
    fi

    # Check statefulset exists
    if ! kubectl get statefulset/"${STATEFULSET}" -n "${NAMESPACE}" &> /dev/null; then
        log_error "StatefulSet ${STATEFULSET} not found in namespace ${NAMESPACE}."
        echo ""
        echo "Make sure AC service is deployed:"
        echo "  ./infra/kind/scripts/setup.sh"
        echo ""
        exit 1
    fi

    log_info "Prerequisites OK."
}

run_local_service() {
    log_step "Starting local ${BINARY}..."
    echo ""
    log_info "============================================"
    log_info "Telepresence intercept active!"
    log_info "Service: ${BINARY} on port ${PORT}"
    log_info "Press Ctrl+C to stop and restore cluster"
    log_info "============================================"
    echo ""

    # Export environment variables that the service needs
    export DATABASE_URL="postgresql://darktower:dev_password_change_in_production@localhost:5432/dark_tower"
    export AC_MASTER_KEY="AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8="
    export AC_HASH_SECRET="AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8="
    export RUST_LOG="info,${BINARY//-/_}=debug"
    export BIND_ADDRESS="0.0.0.0:${PORT}"
    # Disable drain period for fast local shutdown (production uses 30s)
    export AC_DRAIN_SECONDS=0

    # Note: We use localhost:5432 because port-forward is already set up by setup.sh
    # The in-cluster URL would be postgres.dark-tower.svc.cluster.local but that
    # requires DNS resolution through Telepresence which adds complexity.

    log_info "Environment:"
    log_info "  DATABASE_URL=${DATABASE_URL}"
    log_info "  BIND_ADDRESS=${BIND_ADDRESS}"
    log_info "  RUST_LOG=${RUST_LOG}"
    echo ""

    # Run the service
    cd "${PROJECT_ROOT}"
    cargo run --bin "${BINARY}"
}

main() {
    if [[ $# -lt 1 ]]; then
        usage
    fi

    local SERVICE_KEY="$1"

    # Check if service key is valid
    if [[ ! -v "SERVICES[$SERVICE_KEY]" ]]; then
        log_error "Unknown service: ${SERVICE_KEY}"
        echo ""
        usage
    fi

    # Parse service config
    IFS=':' read -r STATEFULSET BINARY PORT <<< "${SERVICES[$SERVICE_KEY]}"

    echo ""
    log_info "============================================"
    log_info "Dark Tower Local Development"
    log_info "Service: ${BINARY}"
    log_info "============================================"
    echo ""

    # Check prerequisites
    check_prerequisites

    # Get current replica count for restoration
    ORIGINAL_REPLICAS=$(kubectl get statefulset/"${STATEFULSET}" -n "${NAMESPACE}" -o jsonpath='{.spec.replicas}')
    log_info "Current replicas: ${ORIGINAL_REPLICAS}"

    # Set up cleanup trap
    trap cleanup EXIT INT TERM

    # Kill any existing port-forwards on our port
    log_info "Killing any existing processes on port ${PORT}..."
    fuser -k "${PORT}/tcp" 2>/dev/null || true

    # Scale down in-cluster service
    log_step "Scaling down ${STATEFULSET} in cluster..."
    kubectl scale statefulset/"${STATEFULSET}" -n "${NAMESPACE}" --replicas=0

    # Force-delete pods immediately (skip graceful termination for dev speed)
    log_info "Force-deleting pods for faster iteration..."
    kubectl delete pod -l app="${STATEFULSET}" -n "${NAMESPACE}" --grace-period=0 --force 2>/dev/null || true

    # Brief wait to ensure pods are gone
    kubectl wait --for=delete pod -l app="${STATEFULSET}" -n "${NAMESPACE}" --timeout=10s 2>/dev/null || true

    # Connect to cluster via Telepresence
    log_step "Connecting to cluster via Telepresence..."
    telepresence connect
    TELEPRESENCE_CONNECTED=true

    # Run the local service
    run_local_service
}

main "$@"
