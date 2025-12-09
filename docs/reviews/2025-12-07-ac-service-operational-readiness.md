# AC-Service Operational Readiness Gap Analysis

**Date**: 2025-12-07
**Service**: Authentication Controller (ac-service)
**Reviewers**: Observability, Operations, Infrastructure, Test (Chaos Testing)
**Trigger**: Introduction of new specialist requirements

## Executive Summary

This gap analysis evaluates ac-service against the standards defined by the newly introduced Observability, Operations, and Infrastructure specialists. The analysis identifies gaps between current implementation and operational readiness requirements, categorizing findings as either "Quick Fixes" (code review scope) or "Needs Debate" (architectural decisions requiring multi-agent consensus).

**Overall Assessment**: ac-service has strong foundations but lacks production-readiness instrumentation. The code is functionally complete and well-tested, but would be difficult to operate, debug, and scale in production without the improvements identified below.

---

## Observability Findings

### Current State

**What exists**:
- Basic `tracing` usage for error logging (e.g., `tracing::error!`, `tracing::debug!`)
- `TraceLayer::new_for_http()` in routes for HTTP request tracing
- Audit logging for key rotation events with structured fields

**What's missing**:
- No `#[instrument]` attributes on handlers or service functions
- No Prometheus metrics (request counts, latencies, error rates)
- No histogram buckets for SLO measurement
- No explicit trace span creation for database operations
- No correlation ID propagation between services
- No SLO definitions for AC operations
- No dashboard templates

### Findings

#### BLOCKER Issues

1. **No request metrics** - `routes/mod.rs`
   - **Gap**: No metrics for token issuance rate, validation latency, error rates
   - **Impact**: Cannot measure SLOs, cannot alert on degradation, cannot capacity plan
   - **Category**: Needs Debate (metrics framework selection, naming conventions)

2. **No `#[instrument]` on handlers** - `handlers/*.rs`
   - **Gap**: Handlers lack automatic span creation with request context
   - **Impact**: Cannot trace requests through handlers, debugging requires manual log correlation
   - **Category**: Needs Design (privacy-by-default: use `skip_all`, explicitly allow-list safe fields; Security must review for PII)

#### HIGH Issues

3. **No database operation spans** - `repositories/*.rs`
   - **Gap**: Database queries not instrumented with spans
   - **Impact**: Cannot identify slow queries in traces, no visibility into DB latency contribution
   - **Category**: Needs Design (specify span attributes: `db.table`, `db.operation`; part of observability framework debate)

4. **No crypto operation spans** - `crypto/mod.rs`
   - **Gap**: JWT signing/verification, encryption/decryption not instrumented
   - **Impact**: Cannot measure crypto operation latency, cannot identify performance bottlenecks
   - **Category**: Needs Design (specify span attributes: `crypto.operation`; part of observability framework debate)

5. **Missing SLO definitions**
   - **Gap**: No defined SLOs for AC operations
   - **Impact**: No error budget tracking, no objective measure of service health
   - **Category**: Needs Debate (define SLO targets for token issuance, validation)

#### MEDIUM Issues

6. **Health check too simple** - `routes/mod.rs:62`
   - **Gap**: Health check returns "OK" without checking database connectivity or Redis (when added)
   - **Impact**: Service may report healthy while dependencies are unavailable
   - **Category**: Quick Fix (add DB ping to health check; add Redis when implemented)

7. **No rate limit metrics**
   - **Gap**: Rate limiting occurs but no metrics on rejection rate
   - **Impact**: Cannot alert on rate limit abuse, cannot tune limits based on data
   - **Category**: Quick Fix (add counter for rate limit rejections)

### Recommended SLOs (for debate)

| Operation | SLI | Proposed Objective | Window |
|-----------|-----|-------------------|--------|
| Token issuance | Latency p99 | < 100ms | 30d |
| Token validation | Latency p99 | < 50ms | 30d |
| Token issuance | Availability | 99.9% | 30d |
| JWKS endpoint | Availability | 99.99% | 30d |
| Key rotation | Success rate | 100% | 30d |

---

## Operations Findings

### Current State

**What exists**:
- Key rotation endpoint with rate limiting (`/internal/rotate-keys`)
- Database transaction with advisory lock for TOCTOU prevention
- Configurable cluster name via `AC_CLUSTER_NAME` env var
- Audit logging for key rotation events

**What's missing**:
- No runbooks for any alerts
- No deployment strategy documentation
- No rollback procedures
- No graceful shutdown handling
- No readiness endpoint (separate from liveness)
- No cost estimation

### Findings

#### BLOCKER Issues

1. **No graceful shutdown** - `main.rs`
   - **Gap**: Service terminates without draining in-flight requests
   - **Impact**: Deployments cause request failures, tokens may be issued but response lost
   - **Category**: Quick Fix (add graceful shutdown with signal handling)

2. **No readiness probe** - `routes/mod.rs`
   - **Gap**: Only `/health` endpoint (liveness), no `/ready` endpoint
   - **Impact**: K8s may route traffic before service is fully initialized (DB connected, keys loaded)
   - **Category**: Quick Fix (add `/ready` endpoint that verifies DB and key availability)

#### HIGH Issues

3. **No runbooks** - N/A
   - **Gap**: No operational documentation for AC alerts
   - **Impact**: On-call cannot respond effectively to incidents
   - **Category**: Needs Documentation

   **Required runbooks**:
   - "AC Token Issuance Latency High"
   - "AC Database Connection Failures"
   - "AC Key Rotation Failed"
   - "AC Rate Limit Exceeded (abuse)"
   - "AC JWKS Endpoint Errors"

4. **No deployment documentation**
   - **Gap**: No documented deployment procedure for AC
   - **Impact**: Risk of breaking deployments, no rollback plan
   - **Category**: Needs Documentation

5. **Circuit breaker on database calls missing**
   - **Gap**: Database failures cascade directly to clients
   - **Impact**: During DB issues, AC becomes completely unavailable instead of degraded
   - **Category**: Needs Debate (circuit breaker pattern, fallback behavior)

#### MEDIUM Issues

6. **No timeout on external calls**
   - **Gap**: Database queries have no explicit timeout
   - **Impact**: Hung queries can exhaust connection pool, cascade to all requests
   - **Category**: Quick Fix (add statement_timeout to DB connection)

7. **Key rotation rate limit not configurable**
   - **Gap**: 6-day normal / 1-hour force rotation hardcoded
   - **Impact**: Cannot tune rotation frequency without code change
   - **Category**: Quick Fix (make configurable via environment)

### Deployment Considerations

**Current gaps**:
- No documentation on zero-downtime deployment
- No database migration rollback procedures
- No feature flags for gradual rollout

**Key rotation deployment note**: Current implementation handles key rotation atomically with overlap period, which is good. However, need to document:
- How to verify rotation succeeded
- How to rollback if new key is compromised
- How to invalidate old key early if needed

---

## Infrastructure Findings

### Current State

**What exists**:
- Dockerfile: None
- Kubernetes manifests: None
- Terraform: None
- CI/CD: GitHub Actions for testing

**What's missing**:
- Container image definition
- Kubernetes deployment resources
- Resource limits and requests
- Network policies
- Secrets management integration

### Findings

#### BLOCKER Issues

1. **No Dockerfile**
   - **Gap**: No container image definition for AC
   - **Impact**: Cannot deploy to Kubernetes or any container orchestrator
   - **Category**: Needs Debate (base image, multi-stage build, distroless vs alpine)

2. **No Kubernetes manifests**
   - **Gap**: No Deployment, Service, ConfigMap, Secret definitions
   - **Impact**: Cannot deploy AC to production
   - **Category**: Needs Debate (resource sizing, replica count, PDB)

#### HIGH Issues

3. **No resource limits defined**
   - **Gap**: No understanding of AC resource requirements
   - **Impact**: Cannot right-size pods, risk of OOM or CPU throttling
   - **Category**: Needs Benchmarking (profile under load to determine limits)

4. **No network policy**
   - **Gap**: No definition of what can talk to AC
   - **Impact**: Any pod in cluster can access AC endpoints
   - **Category**: Quick Fix once K8s manifests exist

5. **Master key management**
   - **Gap**: `AC_MASTER_KEY` injected via env var, no Vault integration
   - **Impact**: Secret potentially visible in pod spec, no rotation support
   - **Category**: Needs Debate (Vault integration vs external-secrets)

#### MEDIUM Issues

6. **No HPA (Horizontal Pod Autoscaler)**
   - **Gap**: No autoscaling configuration
   - **Impact**: Cannot scale based on load
   - **Category**: Depends on metrics (need Prometheus first)

7. **No PodDisruptionBudget**
   - **Gap**: No protection against simultaneous pod termination
   - **Impact**: Cluster upgrades could terminate all AC pods
   - **Category**: Quick Fix once K8s manifests exist

### Cloud Agnosticism Assessment

**Current state**: Code is cloud-agnostic
- Uses standard PostgreSQL (no Aurora-specific features)
- No cloud SDK dependencies
- Could run on any Kubernetes cluster

**Future considerations**:
- Keep master key management portable (Vault preferred over AWS Secrets Manager)
- Use standard Ingress (not ALB-specific annotations)

---

## Chaos Testing Findings

### Current State

**What exists**:
- Comprehensive unit tests (86% coverage)
- Integration tests with test database
- Fuzz testing for JWT validation

**What's missing**:
- No chaos tests for failure scenarios
- No database failure tests
- No network failure tests
- No resource exhaustion tests

### Findings

#### HIGH Issues

1. **No database failure chaos tests**
   - **Gap**: No tests for AC behavior when PostgreSQL is unavailable
   - **Impact**: Unknown behavior during DB outage, may panic or hang
   - **Category**: Needs Implementation

   **Required scenarios**:
   - Connection refused
   - Connection timeout
   - Query timeout
   - Connection pool exhaustion

2. **No recovery verification tests**
   - **Gap**: No tests that verify AC recovers after failures
   - **Impact**: Unknown if AC self-heals or requires restart
   - **Category**: Needs Implementation

#### MEDIUM Issues

3. **No rate limit recovery tests**
   - **Gap**: Rate limiter behavior after token bucket refill untested
   - **Impact**: May have edge cases in rate limit recovery
   - **Category**: Quick Fix (add test)

4. **No concurrent key rotation tests**
   - **Gap**: Race condition prevention tested, but not under heavy load
   - **Impact**: May have edge cases under extreme concurrency
   - **Category**: Needs Load Testing

### Recommended Chaos Test Scenarios

| Scenario | Failure Mode | Expected Behavior |
|----------|--------------|-------------------|
| PostgreSQL down | Connection refused | Return 503, health check fails |
| PostgreSQL slow | Query timeout | Return 503 within timeout |
| Redis down (future) | Cache unavailable | Fallback to DB, higher latency |
| Connection pool exhausted | All connections in use | Queue requests, timeout gracefully |
| Memory pressure | Approaching OOM | Shed load, not crash |

---

## Implementation Plan

**Status**: ADR-0011 and ADR-0012 debates complete. Implementation plan created 2025-12-08.

See [AC Operational Readiness Re-Review Debate](../debates/2025-12-08-ac-operational-readiness-re-review.md) for full implementation plan.

### Summary

| Phase | Goal | Effort | Owner |
|-------|------|--------|-------|
| **Phase 1** | Deployment Foundation (Dockerfile, K8s) | 8h | Infrastructure |
| **Phase 2** | Operational Safety (shutdown, /ready, timeouts) | 6h | Operations, AC |
| **Phase 3** | Observability (metrics, tracing, PII) | 10h | Observability, AC |
| **Phase 4** | Documentation & Testing (runbooks, chaos) | 4h | Operations, Test |
| **Phase 5** | Operational Readiness Review | 4h | Operations (all review) |

**Total**: 32 hours (~4 developer-days)

### Phase Details

#### Phase 1: Deployment Foundation
- Multi-stage Dockerfile with distroless base
- K8s StatefulSet, Service, NetworkPolicy
- TLS on PostgreSQL connection (verify-full)
- DB pool configuration (20 connections, timeouts)

#### Phase 2: Operational Safety
- Graceful shutdown (30s drain)
- `/ready` endpoint (DB + signing key check)
- HTTP request timeout (30s)
- SQL query timeout (5s)

#### Phase 3: Observability
- Prometheus `/metrics` endpoint
- Business metrics per ADR-0011
- `#[instrument(skip_all)]` on all handlers
- PII redaction helpers
- Grafana dashboard

#### Phase 4: Documentation & Testing
- Deployment runbook
- Incident response runbooks (AC-001 through AC-004)
- Chaos tests (DB failure, slow queries)

#### Phase 5: Operational Readiness Review
- ADR-0011/ADR-0012 compliance checklist
- Prometheus AlertManager rules
- SLO dashboards with error budget
- Security final review
- Load test at 2x traffic

---

## Appendix: Files Reviewed

- `crates/ac-service/src/main.rs`
- `crates/ac-service/src/lib.rs`
- `crates/ac-service/src/routes/mod.rs`
- `crates/ac-service/src/handlers/auth_handler.rs`
- `crates/ac-service/src/handlers/admin_handler.rs`
- `crates/ac-service/src/services/token_service.rs`
- `crates/ac-service/src/crypto/mod.rs`
- `crates/ac-service/src/errors.rs`
- `crates/ac-service/Cargo.toml`
