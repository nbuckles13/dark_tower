#!/usr/bin/env bash
#
# Setup script for Dark Tower local development environment
#
# This script creates a kind cluster with full production parity:
# - Calico CNI for NetworkPolicy enforcement
# - PostgreSQL and Redis
# - Full observability stack (Prometheus, Grafana, Loki)
# - Database migrations
# - Port-forwarding
#
# Prerequisites:
#   - kind: https://kind.sigs.k8s.io/docs/user/quick-start/#installation
#   - kubectl: https://kubernetes.io/docs/tasks/tools/
#   - podman (preferred) or docker
#
# Environment variables:
#   DT_CLUSTER_NAME  Cluster name (default: dark-tower)
#   DT_PORT_MAP      Path to shell-sourceable port variable file
#
# Usage:
#   ./infra/kind/scripts/setup.sh [OPTIONS]
#
# Options:
#   --yes          Auto-answer yes to all interactive prompts
#   --only <svc>   Only rebuild+redeploy one service (ac, gc, mc, mh)
#   --skip-build   Skip image builds, only apply manifests
#   --help         Show this help message
#
# See ADR-0013 for the single-tier development environment strategy.
# See ADR-0030 for multi-cluster parameterization.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
CLUSTER_NAME="${DT_CLUSTER_NAME:-dark-tower}"
KIND_CONFIG="${PROJECT_ROOT}/infra/kind/kind-config.yaml"
CALICO_VERSION="v3.27.0"

# --- Cluster name validation ---
if [[ ! "${CLUSTER_NAME}" =~ ^[a-z0-9]([a-z0-9-]*[a-z0-9])?$ ]] || [[ ${#CLUSTER_NAME} -gt 63 ]]; then
    echo "ERROR: Invalid cluster name '${CLUSTER_NAME}': must be lowercase alphanumeric/hyphens, start and end with alphanumeric, max 63 chars" >&2
    exit 1
fi

# --- DT_PORT_MAP sourcing ---
if [[ -n "${DT_PORT_MAP:-}" ]]; then
    if [[ ! -f "${DT_PORT_MAP}" ]]; then
        echo "ERROR: DT_PORT_MAP file not found: ${DT_PORT_MAP}" >&2
        exit 1
    fi
    # Validate every line is a safe variable assignment (VAR_NAME=digits)
    while IFS= read -r line || [[ -n "$line" ]]; do
        # Skip empty lines and comments
        [[ -z "$line" || "$line" =~ ^[[:space:]]*# ]] && continue
        if [[ ! "$line" =~ ^[A-Z_][A-Z0-9_]*=[0-9]+$ ]]; then
            echo "ERROR: DT_PORT_MAP contains invalid line: ${line}" >&2
            exit 1
        fi
    done < "${DT_PORT_MAP}"
    source "${DT_PORT_MAP}"
fi

# --- kubectl with explicit context for multi-cluster support ---
KUBECTL="kubectl --context kind-${CLUSTER_NAME}"

# --- Argument parsing ---
AUTO_YES=false
ONLY_SERVICE=""
SKIP_BUILD=false

# Auto-yes when stdin is not a TTY (automated callers)
if [[ ! -t 0 ]]; then
    AUTO_YES=true
fi

print_usage() {
    sed -n '2,/^$/{ s/^# \?//; p }' "${BASH_SOURCE[0]}"
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --yes)
            AUTO_YES=true
            shift
            ;;
        --only)
            if [[ -z "${2:-}" ]]; then
                echo "ERROR: --only requires a service name (ac, gc, mc, mh)" >&2
                exit 1
            fi
            case "$2" in
                ac|gc|mc|mh) ONLY_SERVICE="$2" ;;
                *)
                    echo "ERROR: Unknown service '$2'. Valid: ac, gc, mc, mh" >&2
                    exit 1
                    ;;
            esac
            shift 2
            ;;
        --skip-build)
            SKIP_BUILD=true
            shift
            ;;
        --help)
            print_usage
            exit 0
            ;;
        *)
            echo "ERROR: Unknown option '$1'" >&2
            print_usage >&2
            exit 1
            ;;
    esac
done

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

_ts() { date '+%H:%M:%S'; }

log_info() {
    echo -e "${GREEN}[$(_ts) INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[$(_ts) WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[$(_ts) ERROR]${NC} $1"
}

log_step() {
    echo -e "${BLUE}[$(_ts) STEP]${NC} $1"
}

# Load a container image into the Kind cluster.
# Handles podman save/load workaround vs docker direct load.
# Usage: load_image_to_kind <image-tag>
load_image_to_kind() {
    local TAG="$1"
    if [[ "${KIND_EXPERIMENTAL_PROVIDER:-}" == "podman" ]]; then
        local TMPFILE
        TMPFILE=$(mktemp /tmp/kind-image.XXXXXX.tar)
        podman save "$TAG" -o "${TMPFILE}"
        kind load image-archive "${TMPFILE}" --name "${CLUSTER_NAME}"
        rm -f "${TMPFILE}"
    else
        kind load docker-image "$TAG" --name "${CLUSTER_NAME}"
    fi
}

# Detect container runtime (prefer Podman)
detect_container_runtime() {
    if command -v podman &> /dev/null; then
        log_info "Using Podman as container runtime"
        export KIND_EXPERIMENTAL_PROVIDER=podman
    elif command -v docker &> /dev/null; then
        log_info "Using Docker as container runtime"
        export KIND_EXPERIMENTAL_PROVIDER=docker
    else
        log_error "Neither Podman nor Docker found. Please install one of them."
        exit 1
    fi
}

# Check prerequisites
check_prerequisites() {
    log_step "Checking prerequisites..."

    if ! command -v kind &> /dev/null; then
        log_error "kind is not installed. Install from: https://kind.sigs.k8s.io/"
        exit 1
    fi

    if ! command -v kubectl &> /dev/null; then
        log_error "kubectl is not installed. Install from: https://kubernetes.io/docs/tasks/tools/"
        exit 1
    fi

    detect_container_runtime

    log_info "All prerequisites satisfied."
}

# Create kind cluster
create_cluster() {
    if kind get clusters 2>/dev/null | grep -q "^${CLUSTER_NAME}$"; then
        log_warn "Cluster '${CLUSTER_NAME}' already exists."
        if [[ "${AUTO_YES}" == "true" ]]; then
            log_info "Cluster exists, reusing (auto-yes defaults to non-destructive)."
            return 0
        fi
        read -p "Delete and recreate? [y/N] " -n 1 -r
        echo
        if [[ $REPLY =~ ^[Yy]$ ]]; then
            log_info "Deleting existing cluster..."
            kind delete cluster --name "${CLUSTER_NAME}"
        else
            log_info "Using existing cluster."
            return 0
        fi
    fi

    log_step "Creating kind cluster '${CLUSTER_NAME}' with Calico CNI..."
    kind create cluster --config="${KIND_CONFIG}" --name="${CLUSTER_NAME}"

    # NOTE: Don't wait for nodes here - they won't be Ready until Calico is installed
    # (because we set disableDefaultCNI: true in kind-config.yaml)
    log_info "Cluster created. Installing CNI before nodes can become Ready..."
}

# Install Calico CNI
install_calico() {
    log_step "Installing Calico CNI for NetworkPolicy enforcement..."

    ${KUBECTL} create -f "https://raw.githubusercontent.com/projectcalico/calico/${CALICO_VERSION}/manifests/calico.yaml"

    log_info "Waiting for Calico pods to be created..."
    # Wait for calico-node pods to exist before trying to wait for Ready
    local max_attempts=30
    local attempt=0
    while [[ $attempt -lt $max_attempts ]]; do
        if ${KUBECTL} get pods -n kube-system -l k8s-app=calico-node 2>/dev/null | grep -q calico-node; then
            break
        fi
        attempt=$((attempt + 1))
        sleep 2
    done

    if [[ $attempt -eq $max_attempts ]]; then
        log_error "Calico pods were not created in time"
        exit 1
    fi

    log_info "Waiting for Calico to be ready..."
    ${KUBECTL} wait --for=condition=Ready pods -l k8s-app=calico-node -n kube-system --timeout=180s
    ${KUBECTL} wait --for=condition=Ready pods -l k8s-app=calico-kube-controllers -n kube-system --timeout=180s

    log_info "Calico CNI installed successfully."

    # Now wait for nodes to be Ready (requires CNI to be installed first)
    log_info "Waiting for nodes to be ready..."
    ${KUBECTL} wait --for=condition=Ready nodes --all --timeout=120s
}

# Create namespaces
create_namespaces() {
    log_step "Creating namespaces..."

    ${KUBECTL} create namespace dark-tower --dry-run=client -o yaml | ${KUBECTL} apply -f -
    ${KUBECTL} create namespace dark-tower-observability --dry-run=client -o yaml | ${KUBECTL} apply -f -

    log_info "Namespaces created."
}

# Build a container image, removing the old image if it was replaced.
# Usage: build_image <tag> <dockerfile> <context-dir>
build_image() {
    local TAG="$1" DOCKERFILE="$2" CONTEXT="$3"
    local CONTAINER_CMD
    if [[ "${KIND_EXPERIMENTAL_PROVIDER:-}" == "podman" ]]; then
        CONTAINER_CMD="podman"
    else
        CONTAINER_CMD="docker"
    fi
    local OLD_IMAGE_ID
    OLD_IMAGE_ID=$(${CONTAINER_CMD} images -q "$TAG" 2>/dev/null || true)
    ${CONTAINER_CMD} build -t "$TAG" -f "$DOCKERFILE" "$CONTEXT"
    if [ -n "$OLD_IMAGE_ID" ] && [ "$OLD_IMAGE_ID" != "$(${CONTAINER_CMD} images -q "$TAG")" ]; then
        ${CONTAINER_CMD} rmi "$OLD_IMAGE_ID" 2>/dev/null || true
    fi
}

# Pre-load third-party images into the Kind cluster.
# Derives the image list from Kustomize manifests (single source of truth),
# pulls to host cache if not present, then loads into Kind.
preload_third_party_images() {
    log_step "Pre-loading third-party images into Kind cluster..."

    local CONTAINER_CMD
    if [[ "${KIND_EXPERIMENTAL_PROVIDER:-}" == "podman" ]]; then
        CONTAINER_CMD="podman"
    else
        CONTAINER_CMD="docker"
    fi

    # Extract third-party images from rendered Kustomize manifests,
    # qualifying Docker Hub short names for podman compatibility.
    local IMAGES
    IMAGES=$(${KUBECTL} kustomize "${PROJECT_ROOT}/infra/kubernetes/overlays/kind/" \
        | grep -oP 'image:\s+\K\S+' \
        | grep -v '^localhost/' \
        | sort -u)

    if [ -z "$IMAGES" ]; then
        log_info "No third-party images found in manifests, skipping."
        return 0
    fi

    # Podman requires fully-qualified names; prefix docker.io/ for Docker Hub images
    if [[ "$CONTAINER_CMD" == "podman" ]]; then
        local qualified=""
        local img
        for img in $IMAGES; do
            if [[ "$img" != *"."*"/"* ]]; then
                qualified+="docker.io/$img"$'\n'
            else
                qualified+="$img"$'\n'
            fi
        done
        IMAGES=$(echo "$qualified" | sed '/^$/d')
    fi

    # Pull to host cache if not already present
    local img
    for img in $IMAGES; do
        if ${CONTAINER_CMD} image inspect "$img" >/dev/null 2>&1; then
            log_info "Cached: $img"
        else
            log_info "Pulling: $img"
            ${CONTAINER_CMD} pull "$img"
        fi
    done

    # Load into Kind one at a time — batching multiple images into a single
    # podman save archive corrupts tag-to-image mapping in kind load.
    for img in $IMAGES; do
        log_info "Loading: $img"
        load_image_to_kind "$img"
    done

    log_info "Third-party images pre-loaded."
}

# Deploy PostgreSQL
deploy_postgres() {
    log_step "Deploying PostgreSQL..."

    ${KUBECTL} apply -k "${PROJECT_ROOT}/infra/kubernetes/overlays/kind/services/postgres/"

    log_info "Waiting for PostgreSQL to be ready..."
    ${KUBECTL} wait --for=condition=Ready pod -l app=postgres -n dark-tower --timeout=120s
    log_info "PostgreSQL deployed successfully."
}

# Deploy Redis
deploy_redis() {
    log_step "Deploying Redis..."

    ${KUBECTL} apply -k "${PROJECT_ROOT}/infra/kubernetes/overlays/kind/services/redis/"

    log_info "Waiting for Redis to be ready..."
    ${KUBECTL} wait --for=condition=Ready pod -l app=redis -n dark-tower --timeout=120s
    log_info "Redis deployed successfully."
}

# Deploy observability stack (Prometheus, Loki, Promtail, kube-state-metrics, node-exporter, Grafana)
deploy_observability() {
    log_step "Deploying observability stack..."

    # Clean up legacy monolithic ConfigMap from older versions of this script
    ${KUBECTL} delete configmap grafana-dashboards -n dark-tower-observability --ignore-not-found

    # Apply entire observability stack via Kustomize overlay
    # (includes Grafana RBAC, deployment, service, and dashboard ConfigMaps via configMapGenerator)
    ${KUBECTL} apply -k "${PROJECT_ROOT}/infra/kubernetes/overlays/kind/observability/"

    log_info "Waiting for kube-state-metrics to be ready..."
    ${KUBECTL} wait --for=condition=available --timeout=120s \
        deployment/kube-state-metrics -n dark-tower-observability

    log_info "Waiting for node-exporter to be ready..."
    ${KUBECTL} rollout status daemonset/node-exporter -n dark-tower-observability --timeout=60s

    log_info "Waiting for Prometheus to be ready..."
    ${KUBECTL} wait --for=condition=Ready pod -l app=prometheus -n dark-tower-observability --timeout=120s

    log_info "Waiting for Loki to be ready..."
    ${KUBECTL} wait --for=condition=Ready pod -l app=loki -n dark-tower-observability --timeout=120s

    log_info "Waiting for Promtail to be ready..."
    ${KUBECTL} wait --for=condition=Ready pod -l app=promtail -n dark-tower-observability --timeout=120s

    log_info "Waiting for Grafana to be ready..."
    ${KUBECTL} wait --for=condition=Ready pod -l app=grafana -n dark-tower-observability --timeout=300s

    log_info "Observability stack deployed successfully."
}

# Run database migrations
run_migrations() {
    log_step "Running database migrations..."

    # Start port-forward in background
    ${KUBECTL} port-forward -n dark-tower svc/postgres 5432:5432 &
    PF_PID=$!

    # Give it a moment to establish
    sleep 3

    export DATABASE_URL="postgresql://darktower:dev_password_change_in_production@localhost:5432/dark_tower"

    # Check if sqlx is available
    if command -v sqlx &> /dev/null; then
        (cd "${PROJECT_ROOT}" && sqlx migrate run)
        log_info "Migrations completed successfully."
    else
        log_warn "sqlx-cli not installed. Run migrations manually:"
        log_warn "  cargo install sqlx-cli --no-default-features --features postgres"
        log_warn "  export DATABASE_URL=\"postgresql://darktower:dev_password_change_in_production@localhost:5432/dark_tower\""
        log_warn "  sqlx migrate run"
    fi

    # Kill port-forward
    kill $PF_PID 2>/dev/null || true
}

# Seed test data (service credentials for development)
seed_test_data() {
    log_step "Seeding test service credentials..."

    # Pre-computed bcrypt hashes (cost factor 12) for development credentials
    # Generated with: python3 -c "import bcrypt; print(bcrypt.hashpw(b'PASSWORD', bcrypt.gensalt(rounds=12)).decode())"
    # Hashes are inlined below with escaped $ to avoid shell interpretation
    #
    # Credentials:
    #   global-controller / global-controller-secret-dev-001
    #   meeting-controller / meeting-controller-secret-dev-002
    #   media-handler / media-handler-secret-dev-003
    #   test-client / test-client-secret-dev-999

    # Insert credentials using idempotent ON CONFLICT DO UPDATE
    ${KUBECTL} exec -n dark-tower postgres-0 -- psql -U darktower -d dark_tower -c "
INSERT INTO service_credentials (client_id, client_secret_hash, service_type, region, scopes, is_active)
VALUES
    ('global-controller', '\$2b\$12\$Gcm3fKCVQzVeCKBkVumWeu9MpAqayxTo08p4aS7xScQTCK8Fi6nBu', 'global-controller', 'us-west-2', ARRAY['meeting:create', 'meeting:read', 'meeting:update', 'internal:meeting-token'], true),
    ('meeting-controller', '\$2b\$12\$BX5OkdvGLfsj6eTM89qkGe/mPpU2nf2aAXDK7v5sedsndrwUmG6dm', 'meeting-controller', 'us-west-2', ARRAY['media:forward', 'session:manage'], true),
    ('media-handler', '\$2b\$12\$DpQDslp37I3UFi.IBC24NOCnMWcPKkdiDO96FEACLVoXqVyYEhyZa', 'media-handler', 'us-west-2', ARRAY['media:receive', 'media:send'], true),
    ('test-client', '\$2b\$12\$DpBLvWIsdO2j3a8dhx0VwOd8kLdZ4/szjsuZVm.TX.z4fxjlWzOny', 'global-controller', NULL, ARRAY['test:all'], true)
ON CONFLICT (client_id) DO UPDATE SET
    client_secret_hash = EXCLUDED.client_secret_hash,
    service_type = EXCLUDED.service_type,
    region = EXCLUDED.region,
    scopes = EXCLUDED.scopes,
    is_active = EXCLUDED.is_active,
    updated_at = NOW();
"

    if [ $? -eq 0 ]; then
        log_info "Test credentials seeded successfully."
    else
        log_error "Failed to seed test credentials."
    fi

    # Seed test organization for env-tests (required by user registration flow)
    # TODO: Replace with AC org provisioning API (see docs/TODO.md)
    log_step "Seeding test organization..."
    ${KUBECTL} exec -n dark-tower postgres-0 -- psql -U darktower -d dark_tower -c "
INSERT INTO organizations (subdomain, display_name, plan_tier, max_concurrent_meetings)
VALUES ('devtest', 'Development Test Organization', 'enterprise', 1000)
ON CONFLICT (subdomain) DO UPDATE SET max_concurrent_meetings = 1000;
"

    if [ $? -eq 0 ]; then
        log_info "Test organization seeded successfully."
    else
        log_error "Failed to seed test organization."
    fi
}

# Create AC service secrets
create_ac_secrets() {
    log_step "Creating AC service secrets..."

    # Use consistent dev master key (same as local dev scripts)
    local MASTER_KEY="AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8="
    local DB_URL="postgresql://darktower:dev_password_change_in_production@postgres.dark-tower.svc.cluster.local:5432/dark_tower"

    ${KUBECTL} create secret generic ac-service-secrets \
        --from-literal=DATABASE_URL="${DB_URL}" \
        --from-literal=AC_MASTER_KEY="${MASTER_KEY}" \
        -n dark-tower \
        --dry-run=client -o yaml | ${KUBECTL} apply -f -

    log_info "AC service secrets created."
}

# Build and deploy AC service
deploy_ac_service() {
    if [[ "${SKIP_BUILD}" != "true" ]]; then
        log_step "Building AC service container image..."
        build_image localhost/ac-service:latest infra/docker/ac-service/Dockerfile "${PROJECT_ROOT}"

        log_step "Loading image into kind cluster..."
        load_image_to_kind localhost/ac-service:latest
    else
        log_warn "Skipping AC image build (--skip-build). Ensure image is already loaded."
    fi

    log_step "Deploying AC service to cluster..."
    ${KUBECTL} apply -k "${PROJECT_ROOT}/infra/kubernetes/overlays/kind/services/ac-service/"

    log_info "Waiting for AC service to be ready..."
    ${KUBECTL} rollout status statefulset/ac-service -n dark-tower --timeout=180s

    log_info "AC service deployed successfully."
}

# Build and deploy Global Controller service
deploy_gc_service() {
    if [[ "${SKIP_BUILD}" != "true" ]]; then
        log_step "Building Global Controller container image..."
        build_image localhost/gc-service:latest infra/docker/gc-service/Dockerfile "${PROJECT_ROOT}"

        log_step "Loading image into kind cluster..."
        load_image_to_kind localhost/gc-service:latest
    else
        log_warn "Skipping GC image build (--skip-build). Ensure image is already loaded."
    fi

    log_step "Deploying Global Controller to cluster..."
    ${KUBECTL} apply -k "${PROJECT_ROOT}/infra/kubernetes/overlays/kind/services/gc-service/"

    log_info "Waiting for Global Controller to be ready..."
    ${KUBECTL} rollout status deployment/gc-service -n dark-tower --timeout=180s

    log_info "Global Controller deployed successfully."
}

# Generate TLS certificates and create MC TLS secret
create_mc_tls_secret() {
    log_step "Generating TLS certificates for MC WebTransport..."

    # Generate dev certs (idempotent — reuses CA if it already exists)
    "${PROJECT_ROOT}/scripts/generate-dev-certs.sh"

    log_step "Creating mc-service-tls Secret from generated certs..."
    ${KUBECTL} create secret tls mc-service-tls \
        --cert="${PROJECT_ROOT}/infra/docker/certs/mc-webtransport.crt" \
        --key="${PROJECT_ROOT}/infra/docker/certs/mc-webtransport.key" \
        -n dark-tower \
        --dry-run=client -o yaml | ${KUBECTL} apply -f -

    log_info "MC TLS secret created successfully."
}

# Build and deploy Meeting Controller service
deploy_mc_service() {
    if [[ "${SKIP_BUILD}" != "true" ]]; then
        log_step "Building Meeting Controller container image..."
        build_image localhost/mc-service:latest infra/docker/mc-service/Dockerfile "${PROJECT_ROOT}"

        log_step "Loading image into kind cluster..."
        load_image_to_kind localhost/mc-service:latest
    else
        log_warn "Skipping MC image build (--skip-build). Ensure image is already loaded."
    fi

    log_step "Deploying Meeting Controller to cluster..."
    ${KUBECTL} apply -k "${PROJECT_ROOT}/infra/kubernetes/overlays/kind/services/mc-service/"

    log_info "Waiting for Meeting Controller to be ready..."
    ${KUBECTL} rollout status deployment/mc-0 -n dark-tower --timeout=180s
    ${KUBECTL} rollout status deployment/mc-1 -n dark-tower --timeout=180s

    log_info "Meeting Controller deployed successfully."
}

# Create MH service secrets
create_mh_secrets() {
    log_step "Creating MH service secrets..."

    ${KUBECTL} create secret generic mh-service-secrets \
        --from-literal=MH_CLIENT_SECRET="media-handler-secret-dev-003" \
        -n dark-tower \
        --dry-run=client -o yaml | ${KUBECTL} apply -f -

    log_info "MH service secrets created."
}

# Generate TLS certificates and create MH TLS secret
create_mh_tls_secret() {
    log_step "Generating TLS certificates for MH WebTransport..."

    # Generate dev certs (idempotent — reuses CA if it already exists)
    "${PROJECT_ROOT}/scripts/generate-dev-certs.sh"

    log_step "Creating mh-service-tls Secret from generated certs..."
    ${KUBECTL} create secret tls mh-service-tls \
        --cert="${PROJECT_ROOT}/infra/docker/certs/mh-webtransport.crt" \
        --key="${PROJECT_ROOT}/infra/docker/certs/mh-webtransport.key" \
        -n dark-tower \
        --dry-run=client -o yaml | ${KUBECTL} apply -f -

    log_info "MH TLS secret created successfully."
}

# Build and deploy Media Handler service
deploy_mh_service() {
    if [[ "${SKIP_BUILD}" != "true" ]]; then
        log_step "Building Media Handler container image..."
        build_image localhost/mh-service:latest infra/docker/mh-service/Dockerfile "${PROJECT_ROOT}"

        log_step "Loading image into kind cluster..."
        load_image_to_kind localhost/mh-service:latest
    else
        log_warn "Skipping MH image build (--skip-build). Ensure image is already loaded."
    fi

    log_step "Deploying Media Handler to cluster..."
    ${KUBECTL} apply -k "${PROJECT_ROOT}/infra/kubernetes/overlays/kind/services/mh-service/"

    log_info "Waiting for Media Handler to be ready..."
    ${KUBECTL} rollout status deployment/mh-0 -n dark-tower --timeout=180s
    ${KUBECTL} rollout status deployment/mh-1 -n dark-tower --timeout=180s

    log_info "Media Handler deployed successfully."
}

# Install Telepresence traffic-manager (optional)
install_telepresence() {
    log_step "Checking Telepresence..."

    if ! command -v telepresence &> /dev/null; then
        log_warn "Telepresence CLI not installed. Skipping traffic-manager installation."
        log_info "Install from: https://www.telepresence.io/docs/latest/install/"
        log_info "Then use: ./scripts/dev/iterate.sh ac"
        return 0
    fi

    # Install traffic-manager if not present
    if ! ${KUBECTL} get deployment traffic-manager -n ambassador &> /dev/null 2>&1; then
        log_info "Installing Telepresence traffic-manager..."
        telepresence helm install || log_warn "Failed to install traffic-manager (may already exist)"
    else
        log_info "Telepresence traffic-manager already installed."
    fi
}

# Setup port-forwards
setup_port_forwards() {
    log_step "Setting up port-forwards (running in background)..."

    # Kill any existing port-forwards for this cluster
    pkill -f "kubectl --context kind-${CLUSTER_NAME} port-forward" 2>/dev/null || true

    # Port variables can be overridden via DT_PORT_MAP
    local PF_POSTGRES="${POSTGRES_PORT:-5432}"
    local PF_AC="${AC_HTTP_PORT:-8082}"
    local PF_GC="${GC_HTTP_PORT:-8080}"
    local PF_MH="${MH_HEALTH_PORT:-8083}"
    local PF_PROMETHEUS="${PROMETHEUS_PORT:-9090}"
    local PF_GRAFANA="${GRAFANA_PORT:-3000}"
    local PF_LOKI="${LOKI_PORT:-3100}"

    # Start port-forwards in background
    ${KUBECTL} port-forward -n dark-tower svc/postgres "${PF_POSTGRES}:5432" &>/dev/null &
    ${KUBECTL} port-forward -n dark-tower svc/ac-service "${PF_AC}:8082" &>/dev/null &
    ${KUBECTL} port-forward -n dark-tower svc/gc-service "${PF_GC}:8080" &>/dev/null &
    ${KUBECTL} port-forward -n dark-tower svc/mh-service "${PF_MH}:8083" &>/dev/null &
    ${KUBECTL} port-forward -n dark-tower-observability svc/prometheus "${PF_PROMETHEUS}:9090" &>/dev/null &
    ${KUBECTL} port-forward -n dark-tower-observability svc/grafana "${PF_GRAFANA}:3000" &>/dev/null &
    ${KUBECTL} port-forward -n dark-tower-observability svc/loki "${PF_LOKI}:3100" &>/dev/null &

    sleep 2
    log_info "Port-forwards established."
}

# Print access information
print_access_info() {
    local p_ac="${AC_HTTP_PORT:-8082}"
    local p_gc="${GC_HTTP_PORT:-8080}"
    local p_mh="${MH_HEALTH_PORT:-8083}"
    local p_grafana="${GRAFANA_PORT:-3000}"
    local p_prometheus="${PROMETHEUS_PORT:-9090}"
    local p_loki="${LOKI_PORT:-3100}"
    local p_postgres="${POSTGRES_PORT:-5432}"
    local ctx="--context kind-${CLUSTER_NAME}"

    echo ""
    log_info "=========================================="
    log_info "Dark Tower kind cluster '${CLUSTER_NAME}' is ready!"
    log_info "=========================================="
    echo ""
    echo "Services Running in Cluster:"
    echo ""
    echo "  AC Service (Auth Controller):"
    echo "    URL: http://localhost:${p_ac}"
    echo "    Status: Running in-cluster (2 replicas)"
    echo ""
    echo "  GC Service (Global Controller):"
    echo "    HTTP API: http://localhost:${p_gc}"
    echo "    gRPC: localhost:50051 (cluster-internal)"
    echo "    Status: Running in-cluster (2 replicas)"
    echo ""
    echo "  MC Service (Meeting Controller) — 2 per-instance Deployments:"
    echo "    WebTransport (per-pod, QUIC/UDP via NodePort):"
    echo "      mc-service-0: https://localhost:4433"
    echo "      mc-service-1: https://localhost:4435"
    echo "    gRPC: localhost:50052 (cluster-internal)"
    echo "    Health: localhost:8081 (cluster-internal)"
    echo "    TLS: Self-signed (CA at infra/docker/certs/ca.crt)"
    echo ""
    echo "  MH Service (Media Handler) — 2 per-instance Deployments:"
    echo "    WebTransport (per-pod, QUIC/UDP via NodePort):"
    echo "      mh-service-0: https://localhost:4434"
    echo "      mh-service-1: https://localhost:4436"
    echo "    gRPC: localhost:50053 (cluster-internal)"
    echo "    Health: http://localhost:${p_mh}"
    echo "    TLS: Self-signed (CA at infra/docker/certs/ca.crt)"
    echo ""
    echo "  Grafana:"
    echo "    URL: http://localhost:${p_grafana}"
    echo "    Credentials: admin/admin"
    echo "    Datasources: Prometheus and Loki (pre-configured)"
    echo "    Dashboards: AC Service dashboard (pre-loaded)"
    echo ""
    echo "  Prometheus:"
    echo "    URL: http://localhost:${p_prometheus}"
    echo ""
    echo "  Loki:"
    echo "    URL: http://localhost:${p_loki}"
    echo "    (Access via Grafana Explore)"
    echo ""
    echo "  PostgreSQL:"
    echo "    Connection: localhost:${p_postgres}"
    echo "    DATABASE_URL: postgresql://darktower:dev_password_change_in_production@localhost:${p_postgres}/dark_tower"
    echo ""
    echo "OAuth 2.0 Service Credentials (pre-seeded, per ADR-0010):"
    echo ""
    echo "  global-controller / global-controller-secret-dev-001  (used by GC)"
    echo "  meeting-controller / meeting-controller-secret-dev-002  (used by MC)"
    echo "  media-handler / media-handler-secret-dev-003  (used by MH)"
    echo "  test-client / test-client-secret-dev-999  (for testing)"
    echo ""
    echo "  GC, MC, and MH use these credentials to obtain OAuth tokens from AC."
    echo "  Tokens are acquired automatically via TokenManager (client credentials flow)."
    echo ""
    echo "Quick Test (AC service is already running):"
    echo ""
    echo "  curl -X POST http://localhost:${p_ac}/api/v1/auth/service/token \\"
    echo "    -H 'Content-Type: application/x-www-form-urlencoded' \\"
    echo "    -d 'grant_type=client_credentials' \\"
    echo "    -d 'client_id=test-client' \\"
    echo "    -d 'client_secret=test-client-secret-dev-999'"
    echo ""
    echo "Local Development (Telepresence):"
    echo ""
    echo "  To iterate on AC service locally while connected to the cluster:"
    echo "    ./scripts/dev/iterate.sh ac"
    echo ""
    echo "  This will:"
    echo "    - Scale down in-cluster AC pods"
    echo "    - Route cluster traffic to your local cargo run"
    echo "    - Auto-restore cluster state on Ctrl+C"
    echo ""
    echo "View Logs & Metrics:"
    echo ""
    echo "  Open http://localhost:${p_grafana} (Grafana)"
    echo "  - Navigate to Dashboards > AC Service"
    echo "  - Or Explore > Loki for logs"
    echo ""
    echo "Restart In-Cluster Services:"
    echo ""
    echo "  kubectl ${ctx} rollout restart statefulset/ac-service -n dark-tower"
    echo "  kubectl ${ctx} rollout restart deployment/mc-0 deployment/mc-1 -n dark-tower"
    echo "  kubectl ${ctx} rollout restart deployment/mh-0 deployment/mh-1 -n dark-tower"
    echo ""
    echo "To tear down:"
    echo "  ./infra/kind/scripts/teardown.sh"
    echo ""
    log_info "Happy coding!"
    echo ""
}

# Deploy a single service (used by --only flag)
deploy_only_service() {
    local svc="$1"

    # Verify cluster exists
    if ! kind get clusters 2>/dev/null | grep -q "^${CLUSTER_NAME}$"; then
        log_error "Cluster '${CLUSTER_NAME}' does not exist. Run full setup first (without --only)."
        exit 1
    fi

    check_prerequisites

    case "$svc" in
        ac)
            create_ac_secrets
            deploy_ac_service
            ;;
        gc)
            deploy_gc_service
            ;;
        mc)
            create_mc_tls_secret
            deploy_mc_service
            ;;
        mh)
            create_mh_secrets
            create_mh_tls_secret
            deploy_mh_service
            ;;
        *)
            log_error "Unknown service '${svc}'"
            exit 1
            ;;
    esac

    log_info "Service '${svc}' deployed successfully."
}

# Main
main() {
    # --only: targeted single-service rebuild+redeploy
    if [[ -n "${ONLY_SERVICE}" ]]; then
        log_info "Rebuilding and redeploying service '${ONLY_SERVICE}'..."
        echo ""
        deploy_only_service "${ONLY_SERVICE}"
        return
    fi

    log_info "Setting up Dark Tower local development environment..."
    echo ""

    check_prerequisites
    create_cluster
    install_calico
    create_namespaces
    preload_third_party_images
    deploy_postgres
    deploy_redis
    deploy_observability
    run_migrations
    seed_test_data
    create_ac_secrets
    deploy_ac_service
    deploy_gc_service
    create_mc_tls_secret
    deploy_mc_service
    create_mh_secrets
    create_mh_tls_secret
    deploy_mh_service
    install_telepresence
    setup_port_forwards
    print_access_info
}

main "$@"
