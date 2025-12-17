# Multi-Agent Debate: Environment Integration Test Suite

**Date**: 2025-12-16
**Status**: Consensus Reached
**Participating Agents**: Infrastructure, Auth Controller, Test, Security, Observability, Operations

## Topic

Design an environment-level integration test suite that:
1. **Validates local dev environment** - Ensures shared components (Kubernetes manifests, networking, observability) work correctly before deploying to production
2. **Validates high-level user/service flows** - E2E tests for major authentication and (later) meeting flows

## Context

### User Requirements
- Tests should be a **Rust test crate** (not shell scripts)
- Tests must run **locally** against the kind cluster
- Optional CI job that can run as a **pre-deploy step** (not on every PR)
- **Full environment** scope: service flows + observability + NetworkPolicy tests

### Current Infrastructure
- **kind cluster** with Calico CNI (NetworkPolicy enforcement)
- **Services**: AC (2 replicas), PostgreSQL, Redis
- **Observability**: Prometheus, Grafana, Loki, Promtail
- **Port forwards**: localhost:8082 (AC), :5432 (Postgres), :9090 (Prometheus), :3000 (Grafana)
- **Pre-seeded credentials**: global-controller, meeting-controller, media-handler, test-client

### Existing Test Infrastructure
- **TestAuthServer** in `ac-test-utils` - spawns isolated AC instances
- **Integration tests** in `crates/ac-service/tests/` - run against spawned servers
- **No existing cluster-level tests** - all tests use in-process servers

### Production Parity Goals
The local dev environment was designed (ADR-0013) to match production:
- Same Kubernetes manifests
- Same NetworkPolicy enforcement
- Same observability stack
- Same container images

Testing against this environment validates the actual deployment artifacts.

## Key Questions for Debate

1. **Test Organization**: How should we structure `crates/env-tests/`?
2. **Cluster Detection**: How do tests know if cluster is running? Fail gracefully or hard fail?
3. **Test Categories**: What's the right split between:
   - Environment validation (infrastructure working?)
   - Service flow tests (auth working?)
   - Observability tests (metrics/logs flowing?)
   - NetworkPolicy tests (security enforced?)
4. **CI Integration**: How to structure the optional pre-deploy workflow?
5. **Test Data**: Use pre-seeded credentials or create fresh data per test?
6. **Parallelism**: Can tests run in parallel or must they be sequential?
7. **Cleanup**: Should tests clean up after themselves or assume fresh cluster?

## Debate Rounds

### Round 1

#### Infrastructure Specialist (Score: 75/100)

**Proposal: Test Structure & Cluster Detection**

```
crates/env-tests/
├── Cargo.toml
├── src/lib.rs                    # Shared utilities
├── tests/
│   ├── cluster_health.rs         # P0: Cluster readiness
│   ├── auth_flows.rs             # P1: Authentication flows
│   ├── network_policies.rs       # P1: NetworkPolicy validation
│   ├── observability.rs          # P1: Metrics/logs flowing
│   └── resilience.rs             # P2: Failure scenarios
```

**Cluster Detection**: Fail-fast with TCP connect checks to AC (8082), Prometheus (9090), Grafana (3000). Use `#[ignore]` attribute with custom test harness that skips when cluster unavailable.

**NetworkPolicy Tests**: Use `kubectl exec` from test pod to validate connectivity rules. Create temporary test pods with known labels.

**CI Integration**: Nightly workflow + manual dispatch. Don't block PRs - too slow and requires kind cluster.

**Concerns**:
- Port-forward brittleness - need abstraction layer
- Test isolation for stateful services
- NetworkPolicy testing requires cluster admin permissions

---

#### Auth Controller Specialist (Score: 75/100)

**Proposal: Authentication Flow Tests**

**Test Categories**:
1. **Token Issuance** - Use pre-seeded `test-client` credentials, verify JWT structure
2. **JWKS Validation** - Fetch `/jwks`, validate key format and algorithm
3. **Cross-Replica Consistency** - Issue token on pod-0, validate on pod-1
4. **Rate Limiting** - Hit limits, verify 429 responses

**Test Data Strategy**: Use pre-seeded credentials exclusively. No test data creation - keeps tests idempotent.

**Token Validation**: Use `jsonwebtoken` crate to validate tokens locally against fetched JWKS. Don't call back to AC for validation.

**Concerns**:
- No consuming service yet to test "real" auth flows (GC not implemented)
- Rate limit tests may be flaky due to shared state across replicas
- Need to handle rate limit cleanup between tests

---

#### Observability Specialist (Score: 75/100)

**Proposal: Observability Validation Tests**

**Metrics Tests**:
- Query Prometheus for `ac_token_issuance_total` after issuing token
- Verify metric labels (client_id, success/failure)
- Validate histogram buckets for latency metrics

**Log Tests**:
- Query Loki for AC logs after operations
- Verify structured log format (JSON with trace_id)
- Check log levels appropriate for operations

**Dashboard Tests**:
- Verify Grafana datasources connected
- Validate key panels return data (not "No data" state)

**Concerns**:
- Loki/Promtail may not be running in all dev setups
- Timing sensitivity - metrics may not be scraped immediately
- Log query syntax differs from Prometheus

---

#### Operations Specialist (Score: 82/100)

**Proposal: Operational Validation Framework**

**Test Modes via Cargo Features**:
```toml
[features]
smoke = []           # Fast cluster health only (30s)
flows = []           # Service flows (2-3min)
resilience = []      # Pod restarts, failures (5min+)
runbook = []         # Operational procedures (manual)
default = ["smoke", "flows"]
```

**Pre-deployment Gate**:
```yaml
# .github/workflows/pre-deploy.yml
on:
  workflow_dispatch:
  schedule:
    - cron: '0 6 * * *'  # Daily at 6 AM
```

**Graceful Degradation**: Tests should report partial success, not all-or-nothing. Use test summary with pass/fail/skip counts.

**Test Isolation**:
- Use `#[serial]` for tests that modify state
- RAII cleanup guards for any created resources
- Timeout all operations (30s default)

**Concerns**:
- No observability validation in initial proposal
- Need clear documentation on when to run which mode

---

#### Test Specialist (Score: 78/100)

**Proposal: Anti-Flakiness Architecture**

**File Organization** (priority-based):
```
tests/
├── 00_cluster_health.rs    # P0: Must pass before others run
├── 10_auth_smoke.rs        # P0: Basic token issuance
├── 20_auth_flows.rs        # P1: Full auth flows
├── 30_observability.rs     # P1: Metrics/logs
├── 40_resilience.rs        # P2: Chaos testing
└── 90_runbook.rs           # P2: Operational validation
```

**Fixture Architecture**:
```rust
// Singleton cluster detection
pub struct ClusterFixture {
    ac_client: reqwest::Client,
    prometheus_client: reqwest::Client,
    base_urls: ClusterUrls,
}

impl ClusterFixture {
    pub async fn new() -> Result<Self, ClusterNotRunning> {
        // TCP connect checks with 5s timeout
    }
}
```

**Anti-Flakiness Measures**:
- Exponential backoff retries (3 attempts, 100ms → 400ms → 1600ms)
- Explicit timeouts on all async operations
- `#[serial]` for any test touching shared state
- Deterministic fixtures - no random data

**Cargo Features**: `smoke`, `flows`, `observability`, `resilience`, `all`

**Concerns**:
- Port-forward reliability across test runs
- Timing-dependent tests (metrics scraping)
- Test interdependencies if run out of order

---

#### Security Specialist (Score: 68/100)

**Proposal: Security Validation Tests**

**NetworkPolicy Testing** (without exec):
- Deploy canary pods with known labels during test setup
- Test connectivity via HTTP probes, not shell exec
- Verify AC cannot reach unauthorized endpoints

**Secret Exposure Tests**:
- Scan pod environment variables for leaked secrets
- Verify logs don't contain credentials
- Check configmaps don't have sensitive data

**Auth Attack Scenarios**:
- Expired token rejection
- Invalid signature rejection
- Tampered claims rejection
- Credential stuffing rate limiting

**Audit Log Validation**:
- Verify security events logged
- Check log format matches security requirements

**Concerns**:
- NetworkPolicy testing via pod exec is insecure pattern
- No secret exposure validation in other proposals
- Rate limit tests may interfere with other tests
- Need to test both positive and negative auth scenarios

---

### Round 1 Summary

| Specialist | Score | Key Concerns |
|------------|-------|--------------|
| Infrastructure | 75 | Port-forward brittleness, test isolation |
| Auth Controller | 75 | No consuming service, rate limit flakiness |
| Observability | 75 | Loki not always running, timing sensitivity |
| Operations | 82 | No observability validation initially |
| Test | 78 | Port-forward reliability, timing issues |
| Security | 68 | NetworkPolicy method, secret exposure tests |

**Consensus**: Not reached (target: 90%). Key blocking issues:
1. Port-forward abstraction needed (multiple specialists)
2. NetworkPolicy testing approach (Infrastructure vs Security disagree)
3. Timing sensitivity for observability tests
4. Rate limit test isolation

---

### Round 2

#### Infrastructure Specialist (Score: 85/100)

**Port-Forward Strategy**: Assume port-forwards are running (developer responsibility). Tests fail-fast with clear error messages: "Port-forward not detected on :8082". TCP socket checks with 5s timeout. Document in README. For CI: use in-cluster test Job later.

**NetworkPolicy Testing**: Agrees with Security's canary pod approach. Deploy lightweight pods with specific labels, test connectivity via HTTP probes (not shell exec). RBAC requirements: `pods.create/delete` (narrower than `pods.exec`).

**Test Isolation**:
- Rate limits: Per-client isolation with dedicated test credentials (`test-ratelimit-1`, `test-ratelimit-2`)
- Redis: Cleanup guards with pattern deletion (`test-*`)
- Database: Read-only tests using pre-seeded credentials only

**Remaining Concerns**: Port-forwards can die mid-test; rate limit 60s reset is slow.

---

#### Auth Controller Specialist (Score: 85/100)

**Rate Limit Testing**: Use unique client_ids per test with time-based suffixes. Alternative: Skip rate limit tests entirely (covered in unit tests). Recommendation: Skip in env-tests.

**Auth Flow Completeness**:
1. Validate locally with `jsonwebtoken` against JWKS
2. Add `/validate` test endpoint to AC (feature-gated)
3. This simulates consuming service until GC exists

**Cross-Replica Testing**: Statistical approach - issue 10+ tokens via load-balanced service, validate all work. If pod-specific needed later, add port-forwards for individual pods.

**Remaining Concerns**: Port-forward reliability; no GC yet for full auth flows.

---

#### Observability Specialist (Score: 85/100)

**Timing Strategy**: Hybrid approach:
1. Direct `/metrics` endpoint (skip Prometheus storage) for immediate validation
2. Prometheus storage with exponential backoff (3 retries: immediate, +15s, +30s) for aggregate tests

**Optional Stack Handling**: Cargo feature `observability` (not in default). Runtime auto-detect with TCP checks. Clear warning when skipped: "⚠️ Skipping - stack not available".

**Integration with Operations**: Observability as orthogonal feature. `resilience` requires `observability` (dependency chain). CI runs `--features all`.

**Remaining Concerns**: LogQL syntax complexity; Grafana API auth for CI.

---

#### Operations Specialist (Score: 90/100)

**Observability Integration**: Separate optional feature. `resilience` requires `observability` for chaos metrics validation.

**Feature Mode Documentation**:
```
cargo test --package env-tests              # smoke + flows (3min)
cargo test --features observability         # + observability (4min)
cargo test --features all                   # full validation (8-10min)
```
- Daily dev: default (smoke + flows)
- Pre-commit with observability: `--features observability`
- Pre-deploy (CI): `--features all`

**Cross-Specialist Coordination**:
- Test priority levels (P0/P1/P2) from Test specialist
- `RateLimitIsolation` RAII guard for Auth Controller
- `CleanupGuard` trait for all stateful tests
- `wait_for_metric()` with exponential backoff for Observability

**Remaining Concerns**: NetworkPolicy approach needs final consensus; runbook feature scope undefined.

---

#### Test Specialist (Score: 88/100)

**Port-Forward Abstraction**: Layered health checks with actionable error messages:
- Phase 1: TCP connectivity (5s)
- Phase 2: HTTP health (10s)
- Phase 3: Service readiness (15s)
- Before each test: lightweight verify_alive() check

**Timing-Dependent Tests**: `ConsistencyCategory` enum with documented SLAs:
- `MetricsScrape`: 30s timeout (2x scrape interval)
- `LogAggregation`: 20s timeout
- `ReplicaSync`: 10s timeout
- `K8sResourceUpdate`: 60s timeout

**Test Independence**: File-scoped fixtures, explicit ordering via numbered prefixes, `#[serial(rate_limit)]` for stateful tests.

**Cross-Specialist Integration**:
- Agrees with Operations' feature modes
- Strongly agrees with Security's canary pod approach
- Provided `assert_eventually` helper for Observability timing

**Remaining Concerns**: Rate limit interference still has some risk; Loki availability needs clearer decision.

---

#### Security Specialist (Score: 85/100)

**NetworkPolicy Approach**: Hybrid solution:
- Smoke tests: HTTP probe canary pods (production-like)
- Resilience tests: kubectl exec for diagnostic-level validation
Both have tradeoffs, use each where appropriate.

**Secret Exposure Implementation**: Add to `cluster_health.rs` (P0 smoke):
- Scan pod env vars for leaked secrets (kubectl get pods -o jsonpath)
- Check ConfigMaps don't contain passwords
- Sample logs for credential patterns (JWT tokens, connection strings)

**Test Isolation**: Dedicated `security-test-client` credentials, `#[serial]` execution, explicit 61s cleanup for rate limits.

**Minimum Security Coverage**:
- P0 (blocks deploy): Secret exposure, basic NetworkPolicy, token signature validation
- P1 (recommended): Expired token, invalid signature, rate limit, cross-replica
- Defer to unit tests: JWT tampering, SQL injection, password hashing

**Remaining Concerns**: Port-forward reliability; timing sensitivity for rate limits; no GC yet.

---

### Round 2 Summary

| Specialist | R1 Score | R2 Score | Change | Remaining Concerns |
|------------|----------|----------|--------|-------------------|
| Infrastructure | 75 | 85 | +10 | Port-forward mid-test failure |
| Auth Controller | 75 | 85 | +10 | No GC for full auth flows |
| Observability | 75 | 85 | +10 | LogQL complexity, Grafana auth |
| Operations | 82 | 90 | +8 | ✅ CONSENSUS REACHED |
| Test | 78 | 88 | +10 | Rate limit interference risk |
| Security | 68 | 85 | +17 | Timing sensitivity |

**Consensus Progress**: 1/6 specialists at 90%+. Average: 86.3%.

**Key Agreements Reached**:
1. ✅ Port-forward abstraction: Assume running, fail-fast with clear errors
2. ✅ NetworkPolicy testing: Canary pods for smoke, kubectl exec for diagnostic
3. ✅ Timing strategy: Direct metrics + Prometheus with exponential backoff
4. ✅ Feature structure: smoke, flows, observability, resilience (observability separate)
5. ✅ Test isolation: RAII cleanup guards, `#[serial]` for stateful tests
6. ✅ Security tests in P0: Secret exposure validation

**Remaining Blockers for Full Consensus**:
1. Rate limit test strategy: Skip vs. isolated client_ids
2. Loki requirement: Required vs. auto-skip with warning

---

### Round 3

#### Final Decisions

**Decision 1: Rate Limit Testing → SKIP in env-tests**

Rationale:
- Coverage already exists in unit tests (`crates/ac-service/tests/`)
- Multiple specialists expressed flakiness concerns (timing, shared state)
- 60s reset windows make test execution slow
- Focus on deployment validation, not algorithm re-testing

Trade-off accepted: We won't validate rate limiting works in the deployed cluster. This is acceptable because rate limiting is configuration-driven and thoroughly tested at unit level.

**Decision 2: Loki Requirement → OPTIONAL with auto-skip and warning**

Rationale:
- Not all devs run Loki locally (resource-heavy)
- Graceful degradation aligns with Operations' "partial success" philosophy
- CI runs `--features all`, ensuring Loki validation in pre-deploy
- Warning message makes gap visible without blocking dev workflow

Implementation:
```rust
if !loki_available() {
    eprintln!("⚠️  Skipping Loki log tests - ensure observability stack running for full validation");
    return Ok(()); // Skip, not fail
}
```

---

### Round 3 Final Scores

| Specialist | R2 Score | R3 Score | Status |
|------------|----------|----------|--------|
| Infrastructure | 85 | 92 | ✅ CONSENSUS |
| Auth Controller | 85 | 95 | ✅ CONSENSUS |
| Observability | 85 | 93 | ✅ CONSENSUS |
| Operations | 90 | 95 | ✅ CONSENSUS |
| Test | 88 | 91 | ✅ CONSENSUS |
| Security | 85 | 90 | ✅ CONSENSUS |

**Average Score: 92.7%** (threshold: 90%)

---

## Consensus Summary

**STATUS: ✅ CONSENSUS REACHED**

### Agreed Architecture

**Crate Structure**:
```
crates/env-tests/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── cluster.rs          # ClusterConnection, health checks
│   ├── eventual.rs         # Retry helpers, ConsistencyCategory
│   ├── fixtures/
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

**Cargo Features**:
```toml
[features]
smoke = []
flows = []
observability = []
resilience = ["observability"]
# No default - tests require explicit features
all = ["smoke", "flows", "observability", "resilience"]
```

**Test Run Commands**:
```bash
# From repo root - runs 0 env-tests (no default features)
cargo test

# Smoke + flows (3 min)
cargo test -p env-tests --features smoke,flows

# With observability (4 min)
cargo test -p env-tests --features smoke,flows,observability

# Pre-deploy validation (8-10 min)
cargo test -p env-tests --features all
```

### Key Design Decisions

1. **Port-forwards**: Developer responsibility, fail-fast with clear errors
2. **NetworkPolicy**: Canary pods for smoke, kubectl exec for diagnostics
3. **Rate limits**: Skip in env-tests (covered by unit tests)
4. **Loki**: Optional with auto-skip and warning, required in CI
5. **Test isolation**: RAII cleanup guards, `#[serial]` for stateful tests
6. **Security P0**: Secret exposure validation, token signature verification

### Implementation Priority

- **P0**: Cluster health, auth smoke, secret exposure
- **P1**: Auth flows, cross-replica, observability
- **P2**: Resilience, chaos testing, runbook validation

---

## Next Steps

1. Create ADR-0014 documenting this architecture
2. Implement `crates/env-tests/` skeleton
3. Add `.github/workflows/pre-deploy.yml`
4. Document port-forward requirements in README
