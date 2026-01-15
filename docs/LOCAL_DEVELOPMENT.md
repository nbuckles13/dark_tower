# Local Development Guide

This guide covers setting up and using the Dark Tower local development environment.

## Overview

Dark Tower uses a **single-tier local development environment** with full production parity (see [ADR-0013](decisions/adr-0013-local-development-environment.md)).

**Key features:**
- kind cluster with Calico CNI (NetworkPolicy enforcement)
- Full observability: Prometheus, Grafana (pre-configured), Loki
- PostgreSQL and Redis
- Same environment for development and CI
- ~2-3 minute one-time setup per session

## Prerequisites

### Required Tools

```bash
# kind - Kubernetes in Docker
# macOS
brew install kind

# Linux (via go install)
go install sigs.k8s.io/kind@latest

# Or download binary
curl -Lo ./kind https://kind.sigs.k8s.io/dl/v0.20.0/kind-linux-amd64
chmod +x ./kind
sudo mv ./kind /usr/local/bin/kind

# kubectl - Kubernetes CLI
# macOS
brew install kubectl

# Linux
curl -LO "https://dl.k8s.io/release/$(curl -L -s https://dl.k8s.io/release/stable.txt)/bin/linux/amd64/kubectl"
chmod +x kubectl
sudo mv kubectl /usr/local/bin/

# Verify installations
kind version
kubectl version --client
```

### Container Runtime

The setup script uses **Podman** (preferred) or Docker:

#### Installing Podman (Recommended)

```bash
# Ubuntu/Debian
sudo apt-get update
sudo apt-get install -y podman

# Fedora
sudo dnf install -y podman

# macOS
brew install podman

# Verify
podman --version

# Recommended: Create docker alias for compatibility
alias docker='podman'
alias docker-compose='podman-compose'
echo "alias docker='podman'" >> ~/.bashrc
echo "alias docker-compose='podman-compose'" >> ~/.bashrc
```

> **Note**: The docker alias is recommended because Claude agents will frequently use `docker` commands instead of `podman`. Without this alias, you'll encounter permission prompts or failed commands during AI-assisted development.

**Why Podman?**
- **Rootless**: Better security, no daemon running as root
- **Daemonless**: No background daemon required
- **Docker-compatible**: Drop-in replacement for Docker CLI
- **Production-aligned**: Same container runtime concepts

#### Using Docker (Alternative)

If you prefer Docker, the setup script will detect and use it automatically:

```bash
# macOS
brew install --cask docker

# Linux (Ubuntu/Debian)
curl -fsSL https://get.docker.com -o get-docker.sh
sudo sh get-docker.sh

# Verify
docker --version
```

### Optional Tools

```bash
# Skaffold - For automatic rebuilds and port-forwarding
# macOS
brew install skaffold

# Linux
curl -Lo skaffold https://storage.googleapis.com/skaffold/releases/latest/skaffold-linux-amd64
chmod +x skaffold
sudo mv skaffold /usr/local/bin/

# sqlx-cli - For database migrations
cargo install sqlx-cli --no-default-features --features postgres

# psql - PostgreSQL client (for direct database access)
# macOS
brew install postgresql

# Ubuntu/Debian
sudo apt-get install postgresql-client

# Fedora
sudo dnf install postgresql

# Verify
psql --version
```

## Quick Start

### 1. Create the Cluster

```bash
# Run the setup script (creates cluster, deploys infrastructure)
./infra/kind/scripts/setup.sh
```

This script deploys **infrastructure only**:
1. Create kind cluster with Calico CNI
2. Deploy PostgreSQL and Redis
3. Deploy observability stack (Prometheus, Grafana, Loki, Promtail)
4. Run database migrations
5. Set up port-forwarding

**Note**: The AC service is NOT deployed by setup.sh. See "Deploy AC Service" below.

**First-time setup takes ~2-3 minutes.** The infrastructure stays up, so you only do this once per session.

### 2. Access Services

After setup completes, services are available at:

| Service | URL | Credentials |
|---------|-----|-------------|
| **Grafana** | http://localhost:3000 | admin / admin |
| **Prometheus** | http://localhost:9090 | - |
| **Loki** | http://localhost:3100 | (via Grafana Explore) |
| **PostgreSQL** | localhost:5432 | darktower / dev_password_change_in_production |
| **Redis** | localhost:6379 | dev_password_change_in_production |

**Grafana is pre-configured** with:
- Prometheus datasource
- Loki datasource
- AC Service dashboard

### 3. Run AC Service

The setup script deploys infrastructure only. You run the AC service separately.

**Option A: Run locally with cargo** (recommended - fast iteration)

```bash
# Set environment variables
export DATABASE_URL="postgresql://darktower:dev_password_change_in_production@localhost:5432/dark_tower"
export AC_MASTER_KEY="$(./scripts/generate-master-key.sh)"

# Run the service (available at http://localhost:8082)
cargo run --bin auth-controller
```

- Sub-second rebuilds with `cargo build`
- Metrics appear in Grafana (Prometheus scrapes localhost:8082)
- Logs go to stdout (standard Rust development experience)

**Option B: Deploy with Skaffold** (full K8s observability)

Use Skaffold when you need full observability (logs in Loki) or K8s-specific testing:

```bash
# From project root
cd infra
skaffold dev

# Skaffold will:
# - Build the AC service Docker image
# - Load it into the kind cluster
# - Deploy to the cluster (available at http://localhost:8083)
# - Watch for code changes and rebuild automatically
```

Note: Skaffold uses port 8083 to avoid conflict with local development.

### 4. View Logs and Metrics

**Grafana (recommended):**
```bash
# Open Grafana
open http://localhost:3000  # macOS
xdg-open http://localhost:3000  # Linux

# Navigate to:
# - Dashboards > AC Service Dashboard (metrics)
# - Explore > Loki (logs)
```

**kubectl (direct):**
```bash
# View AC service logs
kubectl logs -f -n dark-tower -l app=ac-service

# View all pods
kubectl get pods -A

# Describe a pod
kubectl describe pod -n dark-tower <pod-name>
```

### 5. Teardown

When you're done:

```bash
./infra/kind/scripts/teardown.sh
```

This deletes the cluster and cleans up port-forwards.

## Development Workflow

### Unit Tests (No Cluster Required)

For unit and integration tests, only the test database is needed:

```bash
# Start test database
podman-compose -f docker-compose.test.yml up -d
# Or with Docker: docker-compose -f docker-compose.test.yml up -d

# Run tests
export DATABASE_URL="postgresql://postgres:postgres@localhost:5433/dark_tower_test"
cargo test --workspace

# Run with coverage
cargo llvm-cov --workspace --lcov --output-path lcov.info

# Stop test database
podman-compose -f docker-compose.test.yml down
```

### Cluster Tests (env-tests)

The `env-tests` crate contains tests that run against a deployed cluster:

```bash
# 1. Start the cluster (once per session)
./infra/kind/scripts/setup.sh

# 2. Run env-tests
cargo test -p env-tests --features smoke        # Quick health checks (~30s)
cargo test -p env-tests --features flows        # Auth flow tests (~2-3min)
cargo test -p env-tests --features observability # Metrics/logs validation
cargo test -p env-tests --features all          # Everything

# 3. When done, teardown
./infra/kind/scripts/teardown.sh
```

**Feature flags:**
| Feature | Purpose | Duration |
|---------|---------|----------|
| `smoke` | Basic health checks | ~30s |
| `flows` | Service flow validation | ~2-3min |
| `observability` | Metrics and logs | ~30s |
| `resilience` | Pod restarts, chaos | ~5min |
| `all` | All of the above | ~10min |

**Pre-seeded test credentials:**
- Client ID: `test-client`
- Client Secret: `test-client-secret-dev-999`
- Scope: `test:all`

These credentials are automatically seeded by `setup.sh` for env-tests.

**Why separate test database?**
- Different port (5433 vs 5432) avoids conflicts with dev cluster
- Isolated data for tests
- Can run tests while dev cluster is running

### Database Operations

#### Running Migrations

```bash
# Migrations run automatically during setup.sh
# To run manually:
export DATABASE_URL="postgresql://darktower:dev_password_change_in_production@localhost:5432/dark_tower"
sqlx migrate run
```

#### Creating Migrations

```bash
# Create a new migration
sqlx migrate add <name>

# Example:
sqlx migrate add add_user_sessions_table

# Edit the generated file in migrations/
# Then run:
sqlx migrate run
```

#### Connecting to Database

```bash
# Using psql
export DATABASE_URL="postgresql://darktower:dev_password_change_in_production@localhost:5432/dark_tower"
psql $DATABASE_URL

# Or directly
psql -h localhost -p 5432 -U darktower -d dark_tower
```

## Observability

### Metrics (Prometheus)

```bash
# Access Prometheus
open http://localhost:9090

# Example queries:
# - rate(http_requests_total[5m])
# - histogram_quantile(0.95, rate(http_request_duration_seconds_bucket[5m]))
```

### Dashboards (Grafana)

```bash
# Access Grafana
open http://localhost:3000  # admin/admin

# Pre-loaded dashboards:
# - AC Service Dashboard: Request rates, latency, token issuance, errors
```

Grafana is **fully provisioned** on startup with:
- Prometheus datasource
- Loki datasource
- AC service dashboard

No manual configuration needed!

### Logs (Loki)

```bash
# View logs in Grafana
open http://localhost:3000

# Navigate to Explore > Loki
# Example LogQL queries:
# - {namespace="dark-tower", app="ac-service"}
# - {namespace="dark-tower"} |= "error"
# - {namespace="dark-tower"} | json | level="error"
```

**Why Loki over kubectl logs?**
- Historical logs (survives pod restarts)
- Powerful filtering and search
- Correlation with metrics in same UI
- Same experience as production

## Environment Parity

The local environment matches CI and production:

```
Dev (kind + Podman)     CI (kind + Docker)     Production (EKS/GKE)
───────────────────     ──────────────────     ────────────────────
Calico CNI              Calico CNI             Calico (or similar)
Prometheus + Grafana    Prometheus + Grafana   Prometheus + Grafana
Loki                    Loki                   Loki
NetworkPolicy enforced  NetworkPolicy enforced NetworkPolicy enforced

Same manifests, same observability, same network policies
```

**Benefits:**
- Catch NetworkPolicy issues in development
- Reproduce CI failures locally
- Same debugging experience everywhere

## Troubleshooting

### Cluster Won't Start

```bash
# Check if cluster exists
kind get clusters

# Check cluster status
kubectl cluster-info --context kind-dark-tower

# View all pods
kubectl get pods -A

# Delete and recreate
./infra/kind/scripts/teardown.sh
./infra/kind/scripts/setup.sh
```

### Pods Not Ready

```bash
# Check pod status
kubectl get pods -A

# Describe pod to see events
kubectl describe pod -n dark-tower <pod-name>

# Check logs
kubectl logs -n dark-tower <pod-name>

# Check Calico
kubectl get pods -n kube-system -l k8s-app=calico-node
```

### Port-Forward Not Working

```bash
# Kill existing port-forwards
pkill -f "kubectl port-forward"

# Manually set up port-forwards
kubectl port-forward -n dark-tower svc/postgres 5432:5432 &
kubectl port-forward -n dark-tower-observability svc/grafana 3000:3000 &
kubectl port-forward -n dark-tower-observability svc/prometheus 9090:9090 &
kubectl port-forward -n dark-tower-observability svc/loki 3100:3100 &
```

### Database Connection Issues

```bash
# Check PostgreSQL pod
kubectl get pods -n dark-tower -l app=postgres

# Check PostgreSQL logs
kubectl logs -n dark-tower -l app=postgres

# Verify port-forward
ps aux | grep "kubectl port-forward.*postgres"

# Test connection
psql -h localhost -p 5432 -U darktower -d dark_tower
```

### Migrations Fail

```bash
# Check DATABASE_URL
echo $DATABASE_URL

# Verify database is accessible
psql $DATABASE_URL -c "SELECT 1"

# Check migration status
sqlx migrate info

# Force re-run (use with caution)
sqlx migrate run --source migrations
```

### Grafana Not Showing Dashboards

```bash
# Check ConfigMaps
kubectl get configmap -n dark-tower-observability

# Check Grafana logs
kubectl logs -n dark-tower-observability -l app=grafana

# Restart Grafana
kubectl rollout restart deployment/grafana -n dark-tower-observability
```

### Container Runtime Issues

**Podman:**
```bash
# Enable rootless mode
podman system migrate

# Check Podman info
podman info

# Test Podman
podman run hello-world
```

**Docker:**
```bash
# Check Docker daemon
docker info

# Restart Docker Desktop (macOS)
# Or restart Docker service (Linux)
sudo systemctl restart docker
```

## Advanced Usage

### Using NetworkPolicies

NetworkPolicies are enforced via Calico. Test them locally:

```bash
# Apply network policies
kubectl apply -f infra/services/ac-service/network-policy.yaml

# Test connectivity
kubectl run test-pod --rm -it --image=busybox -- sh

# Inside test-pod:
wget -O- http://ac-service.dark-tower:8082/health
```

### Chaos Testing Locally

LitmusChaos can be installed for local chaos testing:

```bash
# Install LitmusChaos
kubectl apply -f https://litmuschaos.github.io/litmus/litmus-operator-v3.0.0.yaml

# Run a chaos experiment
kubectl apply -f infra/chaos/pod-delete.yaml
```

### Performance Profiling

```bash
# Use Grafana to identify bottlenecks
# - Check p95/p99 latency in AC Service Dashboard
# - Identify slow queries in Loki
# - Correlate metrics with logs

# Use Rust profiling tools
cargo build --release
perf record -g ./target/release/auth-controller
perf report
```

## Common Commands

```bash
# === Cluster Management ===
./infra/kind/scripts/setup.sh          # Create cluster
./infra/kind/scripts/teardown.sh       # Delete cluster
kind get clusters                       # List clusters
kubectl cluster-info                    # Cluster info

# === Pod Management ===
kubectl get pods -A                     # All pods
kubectl get pods -n dark-tower          # Dark Tower pods
kubectl logs -f -n dark-tower -l app=ac-service  # AC logs
kubectl describe pod -n dark-tower <pod>  # Pod details
kubectl delete pod -n dark-tower <pod>  # Delete pod

# === Service Management ===
kubectl get svc -A                      # All services
kubectl rollout restart statefulset/ac-service -n dark-tower  # Restart AC

# === Database ===
sqlx migrate run                        # Run migrations
sqlx migrate add <name>                 # Create migration
psql $DATABASE_URL                      # Connect to DB

# === Development ===
skaffold dev                            # Watch mode
skaffold run                            # Deploy once
skaffold delete                         # Remove deployments
cargo test --workspace                  # Run tests
cargo llvm-cov --workspace              # Coverage
```

## See Also

- [ADR-0013: Local Development Environment](decisions/adr-0013-local-development-environment.md) - Single-tier architecture decision
- [ADR-0012: Infrastructure Architecture](decisions/adr-0012-infrastructure-architecture.md) - Overall infrastructure design
- [AC Service Tests README](../crates/ac-service/tests/README.md) - Testing guide
- [Podman Installation Guide](https://podman.io/getting-started/installation) - Podman setup
- [kind Documentation](https://kind.sigs.k8s.io/) - kind reference
- [Calico Documentation](https://docs.projectcalico.org/) - NetworkPolicy guide
