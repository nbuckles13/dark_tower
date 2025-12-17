# Environment Integration Tests

Environment-level integration tests for the Dark Tower local development environment. These tests validate that Kubernetes deployments, observability stack, and service flows work correctly against actual deployment artifacts.

## Prerequisites

Before running these tests, ensure:

1. **Kind cluster running**: Execute `./infra/kind/scripts/setup.sh` from the repository root
2. **Port-forwards active**: The setup script starts port-forwards for:
   - AC Service: `localhost:8082`
   - Prometheus: `localhost:9090`
   - Grafana: `localhost:3000`
   - Loki (optional): `localhost:3100`
3. **kubectl in PATH**: Required for NetworkPolicy diagnostics and secret exposure validation

## Test Categories

Tests are organized by features to allow flexible execution:

| Feature | Tests | Duration | Purpose |
|---------|-------|----------|---------|
| `smoke` | Cluster health, auth smoke, secret exposure | 30s | Fast validation that cluster is working |
| `flows` | Auth flows, cross-replica, JWKS validation | 2-3min | Core service functionality |
| `observability` | Metrics scraping, log aggregation | 2-3min | Observability stack validation (Loki optional) |
| `resilience` | Pod restarts, chaos scenarios | 5min+ | Pre-deploy resilience testing |
| `all` | All test categories | 8-10min | Complete validation suite |

## Usage

### From Repository Root

```bash
# Run 0 env-tests (no default features)
cargo test

# Smoke tests only (30s)
cargo test -p env-tests --features smoke

# Smoke + service flows (3min)
cargo test -p env-tests --features smoke,flows

# With observability stack (4min, Loki optional)
cargo test -p env-tests --features smoke,flows,observability

# Pre-deploy validation - full suite (8-10min)
cargo test -p env-tests --features all
```

### Pre-Deployment Validation

Before deploying to production, run the full test suite:

```bash
./infra/kind/scripts/setup.sh
cargo test -p env-tests --features all
```

## Test Structure

```
crates/env-tests/
├── src/
│   ├── cluster.rs          # ClusterConnection, health checks
│   ├── eventual.rs         # Retry helpers for eventual consistency
│   ├── fixtures/
│   │   ├── auth_client.rs  # Token issuance, JWKS operations
│   │   └── metrics.rs      # Prometheus queries
│   └── canary.rs           # NetworkPolicy test pods (placeholder)
└── tests/
    ├── 00_cluster_health.rs   # P0 smoke tests
    ├── 10_auth_smoke.rs       # P0 auth smoke tests
    ├── 20_auth_flows.rs       # P1 auth flow tests
    ├── 25_auth_security.rs    # P1 security tests
    ├── 30_observability.rs    # P1 observability tests
    ├── 40_resilience.rs       # P2 resilience tests (stubs)
    └── 90_runbook.rs          # P2 runbook validation (stubs)
```

## Pre-Seeded Test Credentials

The kind cluster has these credentials seeded for testing:

- **Client ID**: `test-client`
- **Client Secret**: `test-client-secret-dev-999`
- **Scopes**: `test:all`

## Observability Stack

- **Prometheus**: Required (lightweight ~200MB, essential for health monitoring)
- **Loki**: Optional with auto-skip and warning (~600MB with Promtail)
  - Tests that require Loki will print a warning and skip if unavailable
  - CI runs with `--features all` to ensure full validation

## Design

For the full architecture and design decisions, see:

- **ADR-0014**: [docs/decisions/adr-0014-environment-integration-tests.md](../../docs/decisions/adr-0014-environment-integration-tests.md)
- **Design Debate**: [docs/debates/2025-12-16-environment-integration-tests.md](../../docs/debates/2025-12-16-environment-integration-tests.md)

## Future Work

- Complete resilience tests (currently stubs marked with `#[ignore]`)
- Complete runbook validation tests (currently stubs)
- NetworkPolicy canary pod implementation for network isolation testing
- In-cluster test Job for CI (eliminate port-forward dependency)
