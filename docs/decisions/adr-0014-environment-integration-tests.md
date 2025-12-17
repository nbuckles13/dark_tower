# ADR-0014: Environment Integration Test Suite

**Status**: Accepted
**Date**: 2025-12-16
**Debate**: [docs/debates/2025-12-16-environment-integration-tests.md](../debates/2025-12-16-environment-integration-tests.md)

## Context

The Dark Tower project has a local development environment (ADR-0013) designed to match production: same Kubernetes manifests, NetworkPolicy enforcement, observability stack, and container images. However, there was no way to validate this environment works correctly before deploying to production.

Existing tests run against spawned in-process servers (`TestAuthServer`), not the actual Kubernetes deployment artifacts. This gap means:
- Kubernetes manifest errors aren't caught until production
- NetworkPolicy misconfigurations go undetected
- Observability pipeline issues (Prometheus scraping, log aggregation) are invisible
- Service-to-service communication patterns aren't validated

## Decision

Create an environment-level integration test suite (`crates/env-tests/`) that validates the local dev environment against the same criteria used for production deployment.

### Crate Structure

```
crates/env-tests/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── cluster.rs          # ClusterConnection, health checks
│   ├── eventual.rs         # Retry helpers, ConsistencyCategory
│   ├── fixtures/
│   │   ├── mod.rs
│   │   ├── auth_client.rs  # Token issuance, JWKS fetch
│   │   └── metrics.rs      # Prometheus queries
│   └── canary.rs           # NetworkPolicy test pods
└── tests/
    ├── 00_cluster_health.rs   # P0 smoke
    ├── 10_auth_smoke.rs       # P0 smoke
    ├── 20_auth_flows.rs       # P1 flows
    ├── 25_auth_security.rs    # P1 security
    ├── 30_observability.rs    # P1 observability (optional)
    ├── 40_resilience.rs       # P2 resilience
    └── 90_runbook.rs          # P2 manual
```

### Cargo Features

```toml
[features]
smoke = []                           # Fast cluster health (30s)
flows = []                           # Service flows (2-3min)
observability = []                   # Metrics/logs validation (Loki optional)
resilience = ["observability"]       # Pod restarts, chaos (requires observability)
# No default features - tests require explicit feature flags
# This ensures `cargo test` from repo root runs 0 env-tests (instant pass in CI)
all = ["smoke", "flows", "observability", "resilience"]
```

### Prerequisites

Before running env-tests:

1. **Kind cluster running**: `./infra/kind/scripts/setup.sh` sets up cluster, deploys services, and starts port-forwards
2. **kubectl in PATH**: Required for NetworkPolicy diagnostics and secret exposure validation
3. **Port-forwards active**: AC (8082), Prometheus (9090), Grafana (3000), optionally Loki (3100)

For CI, the setup script manages port-forwards in the background before running tests.

### Key Design Decisions

1. **Port-Forwards**: Developer runs `./infra/kind/scripts/setup.sh` before tests. Tests fail-fast with actionable error messages if ports unreachable. Future enhancement: in-cluster test Job for CI.

2. **NetworkPolicy Testing**: Hybrid approach - canary pods with HTTP probes for smoke tests (production-like), kubectl exec for diagnostic-level validation in resilience tests.

3. **Rate Limit Testing**: Skip in env-tests. Rate limiting is configuration-driven and thoroughly tested at the unit level. Environment tests focus on deployment validation.

4. **Observability Stack**: Prometheus is **required** (lightweight ~200MB, essential for health monitoring). Loki is **optional** with auto-skip and warning (~600MB with Promtail, resource-heavy). CI runs `--features all` to ensure full validation.

5. **Test Isolation**: RAII cleanup guards for resource cleanup. `#[serial]` attribute only for P2 resilience tests (pod restarts, chaos). All P0/P1 tests are read-only and parallel-safe using pre-seeded credentials.

6. **Security P0 (Negative Tests)**: Smoke tests include explicit negative validation:
   - `ac_cannot_reach_unauthorized_endpoints` - canary pod with wrong labels should timeout
   - `secrets_not_exposed_in_env_vars` - kubectl jsonpath scan for plaintext secrets
   - `secrets_not_exposed_in_logs` - log sampling for credentials/tokens
   - Implicit validation: functional tests fail if NetworkPolicy blocks required access (e.g., AC→PostgreSQL)

### Test Categories

| Priority | Feature | Tests | Duration | Purpose |
|----------|---------|-------|----------|---------|
| P0 | smoke | Cluster health, auth smoke, secret exposure | 30s | Gate for all other tests |
| P1 | flows | Auth flows, cross-replica, JWKS validation | 2-3min | Core functionality |
| P1 | observability | Metrics scraping, log aggregation, dashboards | 2-3min | Optional locally |
| P2 | resilience | Pod restarts, chaos scenarios | 5min+ | Pre-deploy validation |

### Test Run Commands

```bash
# From repo root - runs 0 env-tests (no default features)
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

### Timing and Eventual Consistency

Tests that depend on Prometheus scraping or Loki log aggregation use a `ConsistencyCategory` enum with documented SLAs:

- `MetricsScrape`: 30s timeout (2x scrape interval)
- `LogAggregation`: 20s timeout (2x flush interval)
- `ReplicaSync`: 10s timeout (2x expected sync time)
- `K8sResourceUpdate`: 60s timeout (2x expected update time)

The `assert_eventually` helper provides exponential backoff retries.

## Consequences

### Benefits

- **Production parity validation**: Tests run against actual deployment artifacts
- **Early detection**: Kubernetes manifest, NetworkPolicy, and observability issues caught before production
- **Flexible scope**: Features allow running quick smoke tests or full validation
- **Developer-friendly**: Optional Loki doesn't block local development
- **CI integration**: Pre-deploy workflow ensures full validation before production

### Trade-offs

- **Rate limit coverage gap**: Rate limiting not validated in cluster (accepted: unit tests provide sufficient coverage)
- **Port-forward dependency**: Tests require `./infra/kind/scripts/setup.sh` before running (documented in Prerequisites)
- **No default features**: Must explicitly specify `--features` to run any tests (intentional: prevents slow tests in repo-wide `cargo test`)
- **Loki optional locally**: Log aggregation tests may not run during development (mitigated: CI enforces with `--features all`)
- **Test duration**: Full validation takes 8-10 minutes (acceptable for pre-deploy gate)

### Not Included

- **Rate limit environment testing**: Covered by unit tests
- **Full auth flow testing**: Requires Global Controller (future work)
- **Cross-region testing**: Out of scope for local dev environment

## Alternatives Considered

### Shell-based smoke tests

Simpler but lacks rich assertions, async support, and integration with Rust test infrastructure.

### In-cluster test Job

Eliminates port-forward dependency but requires building test container images and adds RBAC complexity. Consider for future CI enhancement.

### Required Loki everywhere

Would block developers who don't run full observability stack. Auto-skip with warning provides better developer experience while CI ensures coverage.

## References

- ADR-0013: Local Development Environment
- ADR-0009: Integration Test Infrastructure
- ADR-0005: Integration Testing Strategy
- Debate: [2025-12-16-environment-integration-tests.md](../debates/2025-12-16-environment-integration-tests.md)
