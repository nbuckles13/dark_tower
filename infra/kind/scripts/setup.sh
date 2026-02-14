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
# Usage:
#   ./infra/kind/scripts/setup.sh
#
# See ADR-0013 for the single-tier development environment strategy.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
CLUSTER_NAME="dark-tower"
KIND_CONFIG="${PROJECT_ROOT}/infra/kind/kind-config.yaml"
CALICO_VERSION="v3.27.0"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
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

log_step() {
    echo -e "${BLUE}[STEP]${NC} $1"
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

    kubectl create -f "https://raw.githubusercontent.com/projectcalico/calico/${CALICO_VERSION}/manifests/calico.yaml"

    log_info "Waiting for Calico pods to be created..."
    # Wait for calico-node pods to exist before trying to wait for Ready
    local max_attempts=30
    local attempt=0
    while [[ $attempt -lt $max_attempts ]]; do
        if kubectl get pods -n kube-system -l k8s-app=calico-node 2>/dev/null | grep -q calico-node; then
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
    kubectl wait --for=condition=Ready pods -l k8s-app=calico-node -n kube-system --timeout=180s
    kubectl wait --for=condition=Ready pods -l k8s-app=calico-kube-controllers -n kube-system --timeout=180s

    log_info "Calico CNI installed successfully."

    # Now wait for nodes to be Ready (requires CNI to be installed first)
    log_info "Waiting for nodes to be ready..."
    kubectl wait --for=condition=Ready nodes --all --timeout=120s
}

# Create namespaces
create_namespaces() {
    log_step "Creating namespaces..."

    kubectl create namespace dark-tower --dry-run=client -o yaml | kubectl apply -f -
    kubectl create namespace dark-tower-observability --dry-run=client -o yaml | kubectl apply -f -

    log_info "Namespaces created."
}

# Deploy PostgreSQL
deploy_postgres() {
    log_step "Deploying PostgreSQL..."

    kubectl apply -f - <<EOF
apiVersion: v1
kind: Secret
metadata:
  name: postgres-secret
  namespace: dark-tower
type: Opaque
stringData:
  POSTGRES_DB: dark_tower
  POSTGRES_USER: darktower
  POSTGRES_PASSWORD: dev_password_change_in_production
---
apiVersion: v1
kind: PersistentVolumeClaim
metadata:
  name: postgres-pvc
  namespace: dark-tower
spec:
  accessModes:
    - ReadWriteOnce
  resources:
    requests:
      storage: 1Gi
---
apiVersion: apps/v1
kind: StatefulSet
metadata:
  name: postgres
  namespace: dark-tower
spec:
  serviceName: postgres
  replicas: 1
  selector:
    matchLabels:
      app: postgres
  template:
    metadata:
      labels:
        app: postgres
    spec:
      containers:
        - name: postgres
          image: postgres:16-alpine
          ports:
            - containerPort: 5432
          envFrom:
            - secretRef:
                name: postgres-secret
          volumeMounts:
            - name: postgres-data
              mountPath: /var/lib/postgresql/data
          readinessProbe:
            exec:
              command:
                - pg_isready
                - -U
                - darktower
                - -d
                - dark_tower
            initialDelaySeconds: 5
            periodSeconds: 5
      volumes:
        - name: postgres-data
          persistentVolumeClaim:
            claimName: postgres-pvc
---
apiVersion: v1
kind: Service
metadata:
  name: postgres
  namespace: dark-tower
spec:
  selector:
    app: postgres
  ports:
    - port: 5432
      targetPort: 5432
  clusterIP: None
EOF

    log_info "Waiting for PostgreSQL to be ready..."
    kubectl wait --for=condition=Ready pod -l app=postgres -n dark-tower --timeout=120s
    log_info "PostgreSQL deployed successfully."
}

# Deploy Redis (using manifests from infra/services/redis/)
deploy_redis() {
    log_step "Deploying Redis..."

    # Apply Redis manifests from our infrastructure directory
    # (excludes network-policy.yaml and service-monitor.yaml which require CRDs)
    for f in "${PROJECT_ROOT}/infra/services/redis/"*.yaml; do
        local basename
        basename="$(basename "$f")"
        [[ "$basename" == "service-monitor.yaml" ]] && continue
        [[ "$basename" == "network-policy.yaml" ]] && continue
        kubectl apply -f "$f"
    done

    log_info "Waiting for Redis to be ready..."
    kubectl wait --for=condition=Ready pod -l app=redis -n dark-tower --timeout=120s
    log_info "Redis deployed successfully."
}

# Deploy kube-state-metrics
deploy_kube_state_metrics() {
    log_step "Deploying kube-state-metrics for cluster metrics..."

    # Apply kube-state-metrics manifests from extracted config files
    kubectl apply -f "${PROJECT_ROOT}/infra/kubernetes/observability/kube-state-metrics.yaml"

    log_info "Waiting for kube-state-metrics to be ready..."
    kubectl wait --for=condition=available --timeout=60s \
        deployment/kube-state-metrics -n dark-tower-observability
    log_info "kube-state-metrics deployed successfully."
}

# Deploy node-exporter
deploy_node_exporter() {
    log_step "Deploying node-exporter for node metrics..."

    # Apply node-exporter manifests from extracted config files
    kubectl apply -f "${PROJECT_ROOT}/infra/kubernetes/observability/node-exporter.yaml"

    log_info "Waiting for node-exporter to be ready..."
    kubectl rollout status daemonset/node-exporter -n dark-tower-observability --timeout=60s
    log_info "node-exporter deployed successfully."
}

# Deploy Prometheus
deploy_prometheus() {
    log_step "Deploying Prometheus..."

    # Apply Prometheus manifests from extracted config files
    kubectl apply -f "${PROJECT_ROOT}/infra/kubernetes/observability/prometheus-config.yaml"

    log_info "Waiting for Prometheus to be ready..."
    kubectl wait --for=condition=Ready pod -l app=prometheus -n dark-tower-observability --timeout=120s
    log_info "Prometheus deployed successfully."
}

# Deploy Loki
deploy_loki() {
    log_step "Deploying Loki for log aggregation..."

    # Apply Loki manifests from extracted config files
    kubectl apply -f "${PROJECT_ROOT}/infra/kubernetes/observability/loki-config.yaml"

    log_info "Waiting for Loki to be ready..."
    kubectl wait --for=condition=Ready pod -l app=loki -n dark-tower-observability --timeout=120s
    log_info "Loki deployed successfully."
}

# Deploy Promtail for log shipping
deploy_promtail() {
    log_step "Deploying Promtail for log shipping to Loki..."

    # Apply Promtail manifests from extracted config files
    kubectl apply -f "${PROJECT_ROOT}/infra/kubernetes/observability/promtail-config.yaml"

    log_info "Waiting for Promtail to be ready..."
    kubectl wait --for=condition=Ready pod -l app=promtail -n dark-tower-observability --timeout=120s
    log_info "Promtail deployed successfully."
}

# Deploy Grafana with provisioning
deploy_grafana() {
    log_step "Deploying Grafana with pre-configured datasources and dashboards..."

    # Create ConfigMaps from provisioning files
    kubectl create configmap grafana-datasources \
        --from-file="${PROJECT_ROOT}/infra/grafana/provisioning/datasources/datasources.yaml" \
        -n dark-tower-observability \
        --dry-run=client -o yaml | kubectl apply -f -

    kubectl create configmap grafana-dashboards-config \
        --from-file="${PROJECT_ROOT}/infra/grafana/provisioning/dashboards/dashboards.yaml" \
        -n dark-tower-observability \
        --dry-run=client -o yaml | kubectl apply -f -

    kubectl create configmap grafana-dashboards \
        --from-file="${PROJECT_ROOT}/infra/grafana/dashboards/" \
        -n dark-tower-observability \
        --dry-run=client -o yaml | kubectl apply -f -

    kubectl apply -f - <<EOF
apiVersion: apps/v1
kind: Deployment
metadata:
  name: grafana
  namespace: dark-tower-observability
spec:
  replicas: 1
  selector:
    matchLabels:
      app: grafana
  template:
    metadata:
      labels:
        app: grafana
    spec:
      containers:
        - name: grafana
          image: grafana/grafana:10.2.2
          ports:
            - containerPort: 3000
          env:
            - name: GF_SECURITY_ADMIN_USER
              value: admin
            - name: GF_SECURITY_ADMIN_PASSWORD
              value: admin
            - name: GF_PATHS_PROVISIONING
              value: /etc/grafana/provisioning
          volumeMounts:
            - name: datasources
              mountPath: /etc/grafana/provisioning/datasources
            - name: dashboards-config
              mountPath: /etc/grafana/provisioning/dashboards
            - name: dashboards
              mountPath: /var/lib/grafana/dashboards
      volumes:
        - name: datasources
          configMap:
            name: grafana-datasources
        - name: dashboards-config
          configMap:
            name: grafana-dashboards-config
        - name: dashboards
          configMap:
            name: grafana-dashboards
---
apiVersion: v1
kind: Service
metadata:
  name: grafana
  namespace: dark-tower-observability
spec:
  selector:
    app: grafana
  ports:
    - port: 3000
      targetPort: 3000
      nodePort: 30030
  type: NodePort
EOF

    log_info "Waiting for Grafana to be ready..."
    kubectl wait --for=condition=Ready pod -l app=grafana -n dark-tower-observability --timeout=120s
    log_info "Grafana deployed successfully."
}

# Run database migrations
run_migrations() {
    log_step "Running database migrations..."

    # Start port-forward in background
    kubectl port-forward -n dark-tower svc/postgres 5432:5432 &
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
    kubectl exec -n dark-tower postgres-0 -- psql -U darktower -d dark_tower -c "
INSERT INTO service_credentials (client_id, client_secret_hash, service_type, region, scopes, is_active)
VALUES
    ('global-controller', '\$2b\$12\$Gcm3fKCVQzVeCKBkVumWeu9MpAqayxTo08p4aS7xScQTCK8Fi6nBu', 'global-controller', 'us-west-2', ARRAY['meeting:create', 'meeting:read', 'meeting:update'], true),
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
}

# Create AC service secrets
create_ac_secrets() {
    log_step "Creating AC service secrets..."

    # Use consistent dev master key (same as local dev scripts)
    local MASTER_KEY="AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8="
    local DB_URL="postgresql://darktower:dev_password_change_in_production@postgres.dark-tower.svc.cluster.local:5432/dark_tower"

    kubectl create secret generic ac-service-secrets \
        --from-literal=DATABASE_URL="${DB_URL}" \
        --from-literal=AC_MASTER_KEY="${MASTER_KEY}" \
        -n dark-tower \
        --dry-run=client -o yaml | kubectl apply -f -

    log_info "AC service secrets created."
}

# Build and deploy AC service
deploy_ac_service() {
    log_step "Building AC service container image..."

    # Detect container runtime command
    local CONTAINER_CMD
    if [[ "${KIND_EXPERIMENTAL_PROVIDER:-}" == "podman" ]]; then
        CONTAINER_CMD="podman"
    else
        CONTAINER_CMD="docker"
    fi

    # Build from project root
    pushd "${PROJECT_ROOT}" > /dev/null
    ${CONTAINER_CMD} build \
        -t localhost/ac-service:latest \
        -f infra/docker/ac-service/Dockerfile \
        .
    popd > /dev/null

    log_step "Loading image into kind cluster..."
    # kind load docker-image has issues with podman, use save/load workaround
    if [[ "${KIND_EXPERIMENTAL_PROVIDER:-}" == "podman" ]]; then
        local TMPFILE
        TMPFILE=$(mktemp /tmp/ac-service-image.XXXXXX.tar)
        podman save localhost/ac-service:latest -o "${TMPFILE}"
        kind load image-archive "${TMPFILE}" --name "${CLUSTER_NAME}"
        rm -f "${TMPFILE}"
    else
        kind load docker-image localhost/ac-service:latest --name "${CLUSTER_NAME}"
    fi

    log_step "Deploying AC service to cluster..."
    # Apply AC service manifests, excluding service-monitor.yaml (requires Prometheus Operator CRD)
    for f in "${PROJECT_ROOT}/infra/services/ac-service/"*.yaml; do
        [[ "$(basename "$f")" == "service-monitor.yaml" ]] && continue
        kubectl apply -f "$f"
    done

    log_info "Waiting for AC service to be ready..."
    kubectl rollout status statefulset/ac-service -n dark-tower --timeout=180s

    log_info "AC service deployed successfully."
}

# Build and deploy Global Controller service
deploy_gc_service() {
    log_step "Building Global Controller container image..."

    # Detect container runtime command
    local CONTAINER_CMD
    if [[ "${KIND_EXPERIMENTAL_PROVIDER:-}" == "podman" ]]; then
        CONTAINER_CMD="podman"
    else
        CONTAINER_CMD="docker"
    fi

    # Build from project root
    pushd "${PROJECT_ROOT}" > /dev/null
    ${CONTAINER_CMD} build \
        -t localhost/global-controller:latest \
        -f infra/docker/global-controller/Dockerfile \
        .
    popd > /dev/null

    log_step "Loading image into kind cluster..."
    if [[ "${KIND_EXPERIMENTAL_PROVIDER:-}" == "podman" ]]; then
        local TMPFILE
        TMPFILE=$(mktemp /tmp/global-controller-image.XXXXXX.tar)
        podman save localhost/global-controller:latest -o "${TMPFILE}"
        kind load image-archive "${TMPFILE}" --name "${CLUSTER_NAME}"
        rm -f "${TMPFILE}"
    else
        kind load docker-image localhost/global-controller:latest --name "${CLUSTER_NAME}"
    fi

    log_step "Deploying Global Controller to cluster..."
    # Apply GC manifests, excluding service-monitor.yaml (requires Prometheus Operator CRD)
    for f in "${PROJECT_ROOT}/infra/services/global-controller/"*.yaml; do
        [[ "$(basename "$f")" == "service-monitor.yaml" ]] && continue
        kubectl apply -f "$f"
    done

    log_info "Waiting for Global Controller to be ready..."
    kubectl rollout status deployment/global-controller -n dark-tower --timeout=180s

    log_info "Global Controller deployed successfully."
}

# Build and deploy Meeting Controller service
deploy_mc_service() {
    log_step "Building Meeting Controller container image..."

    # Detect container runtime command
    local CONTAINER_CMD
    if [[ "${KIND_EXPERIMENTAL_PROVIDER:-}" == "podman" ]]; then
        CONTAINER_CMD="podman"
    else
        CONTAINER_CMD="docker"
    fi

    # Build from project root
    pushd "${PROJECT_ROOT}" > /dev/null
    ${CONTAINER_CMD} build \
        -t localhost/meeting-controller:latest \
        -f infra/docker/meeting-controller/Dockerfile \
        .
    popd > /dev/null

    log_step "Loading image into kind cluster..."
    if [[ "${KIND_EXPERIMENTAL_PROVIDER:-}" == "podman" ]]; then
        local TMPFILE
        TMPFILE=$(mktemp /tmp/meeting-controller-image.XXXXXX.tar)
        podman save localhost/meeting-controller:latest -o "${TMPFILE}"
        kind load image-archive "${TMPFILE}" --name "${CLUSTER_NAME}"
        rm -f "${TMPFILE}"
    else
        kind load docker-image localhost/meeting-controller:latest --name "${CLUSTER_NAME}"
    fi

    log_step "Deploying Meeting Controller to cluster..."
    # Apply MC manifests, excluding service-monitor.yaml (requires Prometheus Operator CRD)
    for f in "${PROJECT_ROOT}/infra/services/meeting-controller/"*.yaml; do
        [[ "$(basename "$f")" == "service-monitor.yaml" ]] && continue
        kubectl apply -f "$f"
    done

    log_info "Waiting for Meeting Controller to be ready..."
    kubectl rollout status deployment/meeting-controller -n dark-tower --timeout=180s

    log_info "Meeting Controller deployed successfully."
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
    if ! kubectl get deployment traffic-manager -n ambassador &> /dev/null 2>&1; then
        log_info "Installing Telepresence traffic-manager..."
        telepresence helm install || log_warn "Failed to install traffic-manager (may already exist)"
    else
        log_info "Telepresence traffic-manager already installed."
    fi
}

# Setup port-forwards
setup_port_forwards() {
    log_step "Setting up port-forwards (running in background)..."

    # Kill any existing port-forwards
    pkill -f "kubectl port-forward.*dark-tower" 2>/dev/null || true

    # Start port-forwards in background
    kubectl port-forward -n dark-tower svc/postgres 5432:5432 &>/dev/null &
    kubectl port-forward -n dark-tower svc/ac-service 8082:8082 &>/dev/null &
    kubectl port-forward -n dark-tower svc/global-controller 8080:8080 &>/dev/null &
    kubectl port-forward -n dark-tower-observability svc/prometheus 9090:9090 &>/dev/null &
    kubectl port-forward -n dark-tower-observability svc/grafana 3000:3000 &>/dev/null &
    kubectl port-forward -n dark-tower-observability svc/loki 3100:3100 &>/dev/null &

    sleep 2
    log_info "Port-forwards established."
}

# Print access information
print_access_info() {
    echo ""
    log_info "=========================================="
    log_info "Dark Tower kind cluster is ready!"
    log_info "=========================================="
    echo ""
    echo "Services Running in Cluster:"
    echo ""
    echo "  AC Service (Auth Controller):"
    echo "    URL: http://localhost:8082"
    echo "    Status: Running in-cluster (2 replicas)"
    echo ""
    echo "  GC Service (Global Controller):"
    echo "    HTTP API: http://localhost:8080"
    echo "    gRPC: localhost:50051 (cluster-internal)"
    echo "    Status: Running in-cluster (2 replicas)"
    echo ""
    echo "  MC Service (Meeting Controller):"
    echo "    gRPC: localhost:50052 (cluster-internal)"
    echo "    Health: localhost:8081 (cluster-internal)"
    echo "    Status: Running in-cluster (2 replicas)"
    echo ""
    echo "  Grafana:"
    echo "    URL: http://localhost:3000"
    echo "    Credentials: admin/admin"
    echo "    Datasources: Prometheus and Loki (pre-configured)"
    echo "    Dashboards: AC Service dashboard (pre-loaded)"
    echo ""
    echo "  Prometheus:"
    echo "    URL: http://localhost:9090"
    echo ""
    echo "  Loki:"
    echo "    URL: http://localhost:3100"
    echo "    (Access via Grafana Explore)"
    echo ""
    echo "  PostgreSQL:"
    echo "    Connection: localhost:5432"
    echo "    DATABASE_URL: postgresql://darktower:dev_password_change_in_production@localhost:5432/dark_tower"
    echo ""
    echo "OAuth 2.0 Service Credentials (pre-seeded, per ADR-0010):"
    echo ""
    echo "  global-controller / global-controller-secret-dev-001  (used by GC)"
    echo "  meeting-controller / meeting-controller-secret-dev-002  (used by MC)"
    echo "  media-handler / media-handler-secret-dev-003  (used by MH)"
    echo "  test-client / test-client-secret-dev-999  (for testing)"
    echo ""
    echo "  GC and MC use these credentials to obtain OAuth tokens from AC."
    echo "  Tokens are acquired automatically via TokenManager (client credentials flow)."
    echo ""
    echo "Quick Test (AC service is already running):"
    echo ""
    echo "  curl -X POST http://localhost:8082/api/v1/auth/service/token \\"
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
    echo "  Open http://localhost:3000 (Grafana)"
    echo "  - Navigate to Dashboards > AC Service"
    echo "  - Or Explore > Loki for logs"
    echo ""
    echo "Restart In-Cluster AC:"
    echo ""
    echo "  kubectl rollout restart statefulset/ac-service -n dark-tower"
    echo ""
    echo "To tear down:"
    echo "  ./infra/kind/scripts/teardown.sh"
    echo ""
    log_info "Happy coding!"
    echo ""
}

# Main
main() {
    log_info "Setting up Dark Tower local development environment..."
    echo ""

    check_prerequisites
    create_cluster
    install_calico
    create_namespaces
    deploy_postgres
    deploy_redis
    deploy_kube_state_metrics
    deploy_node_exporter
    deploy_prometheus
    deploy_loki
    deploy_promtail
    deploy_grafana
    run_migrations
    seed_test_data
    create_ac_secrets
    deploy_ac_service
    deploy_gc_service
    deploy_mc_service
    install_telepresence
    setup_port_forwards
    print_access_info
}

main "$@"
