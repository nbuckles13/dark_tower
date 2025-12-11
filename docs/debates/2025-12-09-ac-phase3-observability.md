# AC Service Phase 3 Observability Debate

**Date**: 2025-12-09
**Participants**: Auth Controller, Test, Security, Observability, Operations
**Rounds**: 2
**Final Consensus**: 86.4% average (4/5 specialists at 92%+)
**Status**: Approved for implementation

## Context

Design the observability instrumentation for AC service per ADR-0011:
- Metrics with Prometheus conventions
- Spans with `#[instrument(skip_all)]` and safe field allow-listing
- Testing strategy for observability code
- PII protection in logs and metrics

## Round 1 Summary

| Specialist | Score | Key Concerns |
|------------|-------|--------------|
| Auth Controller | 75/100 | Missing rate limit, bcrypt, JWKS metrics |
| Test | 72/100 | Need concrete test implementations |
| Security | 45/100 | Error sanitization, timing side-channels, HMAC key mgmt |
| Observability | 75/100 | Error_type cardinality, missing audit failure metrics |
| Operations | 72/100 | Missing DB pool, rate limit, key rotation heartbeat |

## Round 2 Resolution

### 1. Additional Metrics Approved

```rust
// Rate limiting
ac_rate_limit_decisions_total{action}           // action: allowed, rejected

// Bcrypt performance (coarse buckets for security)
ac_bcrypt_duration_seconds{operation}           // operation: hash, verify
// Buckets: [0.05, 0.10, 0.20, 0.50, 1.0]

// Compliance-critical
ac_audit_log_failures_total{event_type, reason}

// DB pool health
ac_db_pool_connections{state}                   // state: idle, active, waiting
ac_db_pool_checkout_duration_seconds

// Key rotation
ac_key_rotation_last_success_timestamp          // Gauge
ac_key_rotation_failures_total{reason}
ac_signing_key_age_days                         // Gauge
```

### 2. Error Category Mapping Approved

```rust
enum ErrorCategory {
    Authentication,  // InvalidCredentials, RateLimitExceeded
    Authorization,   // InsufficientScope
    Cryptographic,   // InvalidToken, InvalidSignature
    Internal,        // Database, Internal
}
```

Cardinality: 10+ error types → 4 categories

### 3. Histogram Buckets Approved

**Token issuance** (SLO p99 < 350ms):
```rust
&[0.010, 0.025, 0.050, 0.100, 0.150, 0.200, 0.250, 0.300, 0.350, 0.500, 1.000, 2.000]
```

**DB queries** (SLO p99 < 50ms):
```rust
&[0.001, 0.002, 0.005, 0.010, 0.020, 0.050, 0.100, 0.250, 0.500, 1.000]
```

### 4. Span Strategy Approved

**Handler-level spans only** (no individual DB spans):
- `ac.token.issue` - Token issuance entry point
- `ac.jwks.get` - JWKS endpoint
- `ac.admin.rotate_key` - Key rotation

**Rationale**: 5+ DB queries per token request would cause span explosion. Use metrics for DB visibility.

### 5. Safe Fields Approved

**SAFE (plaintext)**:
- `grant_type`, `status`, `error_category`, `operation`, `table`, `cache_status`

**HASHED (HMAC-SHA256, first 8 chars)**:
- `client_id_hash` - For correlation across logs

**NEVER LOG**:
- `client_secret`, `jwt`, `private_key`, `password_hash`

### 6. Security Controls Approved

**HMAC Key Management**:
- Storage: K8s Secret `observability-hmac-key-ac`
- Rotation: 30 days via CronJob
- Format: 256-bit random, base64 encoded

**Cardinality Limits**:
- Max 10,000 unique values per label
- When exceeded: Replace with "OTHER"
- Alert: `ac_cardinality_limit_reached_total{metric_name}`

**DEBUG Mode Enforcement**:
- Fail startup if `ENVIRONMENT=production` and `RUST_LOG=debug`
- No `panic!` (use `Result` per ADR-0002)

### 7. Testing Strategy Approved

**P0 - PII Leakage Tests**:
- Regex patterns for JWT, bcrypt, base64, DB URLs
- Assert secrets never appear in collected logs
- Run on every PR

**Metrics Tests**:
- Use `metrics-util::debugging::DebuggingRecorder`
- Test success and error paths
- Verify cardinality stays bounded

**Span Tests**:
- Verify safe fields present, unsafe fields absent
- Test span context propagation

## Round 2 Final Scores

| Specialist | Score | Notes |
|------------|-------|-------|
| Auth Controller | 95/100 | All concerns resolved |
| Observability | 92/100 | PII cleanup needed in existing code |
| Operations | 92/100 | Need signing key age metric |
| Test | 78/100 | Gaps addressable in implementation |
| Security | 75/100 | Cardinality limits implementation detail |
| **Average** | **86.4%** | Approved for implementation |

## Consensus Decisions

1. **Metrics Framework**: `metrics` + `metrics-exporter-prometheus` crates
2. **Error Categories**: 4 bounded categories (not 10+ error types)
3. **Histogram Buckets**: Extended with 10ms floor, 10s ceiling
4. **Span Strategy**: Handler-level only, no DB spans
5. **Safe Fields**: Explicit allow-list, `skip_all` by default
6. **HMAC Key**: K8s Secret with 30-day rotation
7. **Cardinality Limit**: 10,000 per label, "OTHER" bucketing
8. **PII Cleanup**: Audit existing `tracing::*` calls for `client_id` leakage

## Action Items

| Priority | Item | Owner |
|----------|------|-------|
| P0 | Add observability module with metrics | AC Specialist |
| P0 | Add `/metrics` endpoint | AC Specialist |
| P0 | Fix PII leakage in existing logs | Security review |
| P1 | Add `#[instrument(skip_all)]` to handlers | AC Specialist |
| P1 | Add PII leakage tests | Test Specialist |
| P2 | Create Grafana dashboards | Observability Specialist |
| P2 | Create alert runbooks | Operations Specialist |

## References

- ADR-0011: Observability Framework
- ADR-0012: Infrastructure Architecture
