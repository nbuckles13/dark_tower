# AC Operational Readiness Re-Review Debate

**Date**: 2025-12-08
**Topic**: AC Service Operational Readiness against ADR-0011 and ADR-0012
**Rounds**: 2
**Final Consensus**: 92% projected (implementation plan approved)
**Status**: Consensus Reached - Implementation Plan Created

## Participants

| Specialist | Role | Round 1 Score | Projected Final |
|------------|------|---------------|-----------------|
| AC | Service representative | 65% | 92% |
| Observability | ADR-0011 compliance | 15% | 92% |
| Operations | Deployment safety, runbooks | 35% | 95% |
| Infrastructure | ADR-0012 compliance | 15% | 90% |
| Security | PII, secrets, container security | 40% | 95% |
| Test | Operational test coverage | 45% | 90% |
| Database | Connection pooling, TLS, timeouts | 35% | 92% |

**Average**: 36% (Round 1) → 92% (Projected after implementation)

## Context

AC service is functionally complete with 86% test coverage and production-ready security (bcrypt, AES-256-GCM, EdDSA). However, the AC Operational Readiness Review (2025-12-07) identified critical gaps against the newly finalized:
- **ADR-0011**: Observability Framework
- **ADR-0012**: Infrastructure Architecture

This debate re-evaluated AC against these ADRs and produced a prioritized implementation plan.

## Round 1: Individual Specialist Reviews

### Unanimous P0 Blockers Identified

All 7 specialists agreed these issues block production deployment:

1. **No Dockerfile** - Cannot create container images
2. **No K8s Manifests** - Cannot deploy to any environment
3. **No Graceful Shutdown** - Deployments drop in-flight requests
4. **No `/ready` Endpoint** - K8s can't verify service health before routing traffic
5. **No Database Timeouts** - Hung queries cause cascading failures
6. **No Metrics/Tracing** - Zero observability (violates ADR-0011)
7. **No TLS on PostgreSQL** - Violates ADR-0012 security requirement

### Specialist-Specific Findings

#### AC Specialist (65%)
- Functionally excellent code, operationally undeployable
- Missing `#[instrument]` attributes on all handlers
- No load shedding mechanism
- Key rotation rate limits hardcoded (should be configurable)

#### Observability Specialist (15%)
- Zero metrics implemented (9 required per ADR-0011)
- Zero spans implemented (10 required per ADR-0011)
- PII logged in plaintext (client_id, ip_address)
- No SLO dashboards or alert definitions
- Explicit requirements provided:
  - `ac_token_issuance_duration_seconds` histogram
  - `ac_token_validation_duration_seconds` histogram
  - `ac_auth_failures_total` counter
  - Span hierarchy: `ac.token.issue` → `ac.db.query.client` → `ac.token.sign`

#### Operations Specialist (35%)
- No graceful shutdown (SIGTERM causes immediate termination)
- `/health` endpoint too simple (doesn't check DB)
- No `/ready` endpoint for K8s readiness probe
- No timeouts on HTTP requests or DB queries
- No runbooks for any alerts (7 required)
- Explicit requirements provided:
  - 30s graceful shutdown drain
  - `/ready` must verify DB connectivity + active signing key
  - 30s HTTP request timeout, 5s DB query timeout

#### Infrastructure Specialist (15%)
- No Dockerfile exists
- No K8s manifests exist
- No secrets management (AC_MASTER_KEY via env var)
- Explicit requirements provided:
  - Multi-stage Dockerfile with distroless base
  - StatefulSet (not Deployment) for key management stability
  - Resources: 500m/2000m CPU, 1Gi/2Gi memory per ADR-0012
  - NetworkPolicy: ingress from GC only, egress to PostgreSQL only

#### Security Specialist (40%)
- Code security excellent (bcrypt, CSPRNG, AES-256-GCM)
- Infrastructure security nonexistent
- PII in logs without masking
- No container scanning configured
- Missing `#[instrument(skip_all)]` for privacy-by-default

#### Test Specialist (45%)
- 86% functional coverage (excellent)
- 0% operational coverage (blocking)
- No chaos tests (DB failure, timeout, pool exhaustion)
- No graceful shutdown tests
- No metrics emission tests
- No PII leakage tests

#### Database Specialist (35%)
- No TLS on PostgreSQL connection (violates ADR-0012)
- Pool undersized (5 connections, should be 10-20)
- No `statement_timeout` configured
- No `acquire_timeout` on pool
- No DB metrics or spans

## Round 2: Consolidated Implementation Plan

### Ownership Model

| Role | Responsibility |
|------|----------------|
| **Requirements Owner** | Defines what must be implemented (domain expert) |
| **Implementer** | Writes the code/config |
| **Reviewer** | Validates implementation meets ADR requirements |

### Phase 1: Deployment Foundation
**Goal**: Enable AC to be containerized and deployed to Kubernetes
**Effort**: 8 hours
**Requirements Owner**: Infrastructure
**Implementers**: Infrastructure
**Reviewers**: Security, Database

| Task | Implementer | Effort | Priority |
|------|-------------|--------|----------|
| Create multi-stage Dockerfile | Infrastructure | 2h | P0 |
| Add TLS to PostgreSQL connection | Database | 30m | P0 |
| Configure DB pool (20 conn, timeouts) | Database | 45m | P0 |
| Create K8s StatefulSet manifest | Infrastructure | 2h | P0 |
| Create K8s Service manifest | Infrastructure | 30m | P0 |
| Create K8s NetworkPolicy | Infrastructure | 30m | P0 |
| Create K8s ConfigMap/Secret templates | Infrastructure | 30m | P0 |
| Container security scan (Trivy) | Security | 30m | P1 |
| Build and test container locally | Infrastructure | 1h | P0 |

**Acceptance Criteria**:
- Container builds and runs locally
- Trivy scan shows zero HIGH/CRITICAL vulnerabilities
- K8s deployment succeeds in local kind cluster
- DB connection uses TLS (verify-full mode)

**Artifacts**:
- `infra/docker/ac-service/Dockerfile`
- `infra/services/ac-service/statefulset.yaml`
- `infra/services/ac-service/service.yaml`
- `infra/services/ac-service/network-policy.yaml`
- `infra/services/ac-service/configmap.yaml`

### Phase 2: Operational Safety
**Goal**: AC can be safely deployed without dropping requests
**Effort**: 6 hours
**Requirements Owner**: Operations
**Implementers**: AC (code), Operations (K8s updates)
**Reviewers**: Operations, Database

| Task | Implementer | Effort | Priority |
|------|-------------|--------|----------|
| Implement graceful shutdown (30s drain) | AC | 2h | P0 |
| Add `/ready` endpoint (DB + key check) | AC | 1.5h | P0 |
| Add HTTP request timeout (30s) | AC | 1h | P0 |
| Add SQL query timeout (5s) | AC | 30m | P0 |
| Update K8s with readinessProbe | Operations | 30m | P0 |
| Add graceful shutdown test | Test | 30m | P1 |

**Acceptance Criteria**:
- SIGTERM causes graceful shutdown within 30s
- Zero dropped requests during rolling deployment
- `/ready` returns 503 when DB unreachable
- Queries timeout after 5s (not hang)

**Artifacts**:
- Updated `crates/ac-service/src/main.rs` (graceful shutdown)
- Updated `crates/ac-service/src/routes/mod.rs` (`/ready`, timeouts)
- Updated `infra/services/ac-service/statefulset.yaml` (readinessProbe)

### Phase 3: Observability
**Goal**: AC can be monitored in production per ADR-0011
**Effort**: 10 hours
**Requirements Owner**: Observability
**Implementers**: AC (code), Observability (dashboards)
**Reviewers**: Security (PII review)

| Task | Implementer | Effort | Priority |
|------|-------------|--------|----------|
| Add metrics crates to Cargo.toml | AC | 15m | P0 |
| Implement `/metrics` endpoint | AC | 2h | P0 |
| Add business metrics (tokens, errors, latency) | AC | 2h | P0 |
| Add `#[instrument(skip_all)]` to handlers | AC | 2h | P0 |
| Implement PII redaction helpers | AC | 1.5h | P0 |
| Create Grafana dashboard | Observability | 1.5h | P1 |
| Add metrics emission tests | Test | 30m | P1 |

**Required Metrics** (per ADR-0011):
- `ac_token_issuance_duration_seconds{grant_type,status}` - Histogram
- `ac_token_validation_duration_seconds{status}` - Histogram
- `ac_db_query_duration_seconds{operation}` - Histogram
- `ac_auth_failures_total{reason,grant_type}` - Counter
- `ac_key_rotation_total{status,forced}` - Counter
- `ac_rate_limit_denials_total{endpoint}` - Counter

**Required Spans** (per ADR-0011):
- `ac.token.issue` → `ac.client.validate` → `ac.db.query.client`
- `ac.token.issue` → `ac.token.sign`
- `ac.token.verify` → `ac.db.query.signing_key`
- `ac.key.rotate`

**Acceptance Criteria**:
- `/metrics` returns Prometheus-formatted output
- Grafana dashboard shows latency p50/p95/p99, error rate, throughput
- Zero PII in logs (verified by automated scan)
- Trace spans visible in Jaeger

**Artifacts**:
- Updated `crates/ac-service/Cargo.toml`
- New `crates/ac-service/src/observability/mod.rs`
- Updated handlers with `#[instrument]` and metrics
- `infra/grafana/dashboards/ac-service.json`

### Phase 4: Documentation & Testing
**Goal**: AC is operationally mature with runbooks and chaos tests
**Effort**: 4 hours
**Requirements Owner**: Operations
**Implementers**: Operations (runbooks), Test (chaos tests)
**Reviewers**: All specialists

| Task | Implementer | Effort | Priority |
|------|-------------|--------|----------|
| Create deployment runbook | Operations | 1.5h | P1 |
| Create incident response runbook | Operations | 1h | P1 |
| Add chaos test: DB connection loss | Test | 1h | P1 |
| Add chaos test: slow query timeout | Test | 30m | P1 |

**Required Runbooks**:
- `docs/runbooks/ac-service-deployment.md`
- `docs/runbooks/alerts/AC-001-token-issuance-latency.md`
- `docs/runbooks/alerts/AC-002-token-validation-errors.md`
- `docs/runbooks/alerts/AC-003-key-rotation-failed.md`
- `docs/runbooks/alerts/AC-004-database-connection-failures.md`

**Acceptance Criteria**:
- Runbooks successfully used in dry-run exercise
- Chaos tests pass in local kind environment
- Service recovers automatically when DB restored

### Phase 5: Operational Readiness Review & Hardening
**Goal**: Final validation before declaring AC production-ready
**Effort**: 4 hours
**Requirements Owner**: Operations
**Implementers**: Various
**Reviewers**: All specialists

| Task | Implementer | Effort | Priority |
|------|-------------|--------|----------|
| Run ADR-0011/ADR-0012 compliance checklist | Operations | 1h | P1 |
| Configure Prometheus AlertManager rules | Observability | 1h | P1 |
| Set up SLO dashboards with error budget | Observability | 1h | P1 |
| Security final review (container scan, PII audit) | Security | 30m | P1 |
| Load test at 2x expected traffic | Test | 30m | P1 |

**Acceptance Criteria**:
- All ADR-0011 requirements checked and passing
- All ADR-0012 requirements checked and passing
- AlertManager rules firing correctly in test
- SLO dashboard shows error budget tracking
- Load test passes without errors at 2x traffic
- All specialists sign off at ≥90% satisfaction

## Implementation Summary

| Phase | Effort | Cumulative | Satisfaction |
|-------|--------|------------|--------------|
| Phase 1: Deployment Foundation | 8h | 8h | 50% |
| Phase 2: Operational Safety | 6h | 14h | 70% |
| Phase 3: Observability | 10h | 24h | 85% |
| Phase 4: Documentation & Testing | 4h | 28h | 90% |
| Phase 5: Operational Readiness Review | 4h | 32h | 92% |

**Total Effort**: 32 hours (~4 developer-days)

## Decisions Made

### Selected Approaches

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Container base image | Distroless | Minimal attack surface per ADR-0012 |
| K8s workload type | StatefulSet | Stable pod identity for key rotation |
| Graceful shutdown timeout | 30s | Matches K8s terminationGracePeriodSeconds |
| DB pool size | 20 connections | Handles burst traffic, per ADR-0012 |
| Query timeout | 5s | Fail fast, prevent cascading failures |
| Metrics library | `metrics` + `metrics-exporter-prometheus` | Lightweight, Rust-native |
| Tracing approach | `#[instrument(skip_all)]` | Privacy-by-default per ADR-0011 |

### Deferred Items

| Item | Reason | When to Address |
|------|--------|-----------------|
| Circuit breaker pattern | Complexity, DB failures rare | Phase 5 if needed |
| GPU/CPU pinning | AC doesn't need it | Never (per ADR-0012) |
| Blue-green deployment | Canary sufficient | If canary proves insufficient |

## Outcome

**Consensus**: Implementation plan approved with 92% projected satisfaction.

**Next Steps**:
1. Begin Phase 1 implementation (Infrastructure lead)
2. Track progress in `docs/reviews/` or project board
3. Run specialist reviews after each phase
4. Final operational readiness sign-off after Phase 5

## References

- [ADR-0011: Observability Framework](../decisions/adr-0011-observability-framework.md)
- [ADR-0012: Infrastructure Architecture](../decisions/adr-0012-infrastructure-architecture.md)
- [AC Operational Readiness Review (2025-12-07)](../reviews/2025-12-07-ac-service-operational-readiness.md)
