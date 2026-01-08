# Principle: Observability

**All services MUST implement privacy-by-default observability.** Use `#[instrument(skip_all)]`, explicitly allow-list SAFE fields only.

**ADRs**: ADR-0011 (Observability Framework)

---

## DO

### Instrumentation
- **Use `#[instrument(skip_all)]` by default** on all handlers and critical functions
- **Explicitly allow-list SAFE fields** in `fields()` clause - opt-in, not opt-out
- **Include correlation IDs** - `trace_id`, `request_id` in all spans

### Metrics
- **Follow naming convention** - `{service}_{subsystem}_{metric}_{unit}`
- **Use standard labels** - `service`, `region`, `status`, `error_type`
- **Enforce cardinality limits** - max 1,000 unique label combinations per metric
- **Align histogram buckets with SLOs** - buckets at p50, p90, p95, p99 targets

### Spans
- **Follow naming convention** - `{service}.{subsystem}.{operation}` (e.g., `ac.token.issue`)
- **Include required attributes** - `service.name`, `service.version`, `deployment.environment`
- **Limit high-cardinality attributes** - hash or bucket participant/meeting IDs

### Logging Tiers
- **DEBUG** - Full payloads (dev only, never staging/prod)
- **INFO** - SAFE fields only
- **WARN/ERROR** - Correlation IDs and error classification only

### SLOs
- **Define for every service** - latency percentiles, error rates
- **Configure burn rate alerts** - >10x for 1h = critical, >5x for 6h = warning
- **Track error budgets** - 30-day rolling windows

---

## DON'T

### Privacy
- **NEVER use `#[instrument]` without `skip` or `skip_all`** on functions with secrets
- **NEVER log UNSAFE fields in plaintext** - use masked (`****`) or hashed (`h:abc123`)
- **NEVER include PII in error messages** - use generic messages
- **NEVER log request/response bodies** from auth endpoints

### Cardinality
- **NEVER use UUIDs as metric labels** - use indexed values or hashes
- **NEVER exceed 1,000 unique label combinations** per metric
- **NEVER use unbounded string values as labels**

### Assumptions
- **NEVER assume a field is SAFE** - when in doubt, mask it
- **NEVER log full tokens or secrets** - even in DEBUG

---

## Quick Reference

### Field Classification

| Category | SAFE Fields |
|----------|-------------|
| System | `service`, `region`, `environment` |
| Correlation | `trace_id`, `span_id`, `request_id` |
| Operation | `method`, `status_code`, `error_type` |
| Timing | `duration_ms`, `timestamp` |
| Enums | `grant_type`, `codec`, `media_type` |

| Category | UNSAFE Fields (require visibility selection) |
|----------|---------------------------------------------|
| Credentials | `password`, `secret`, `api_key`, `token` |
| PII | `email`, `phone`, `name`, `ip_address` |
| Session | `meeting_id`, `participant_id` |
| Payloads | `request_body`, `response_body` |

### Visibility Levels for UNSAFE Fields

| Level | Output | Use Case |
|-------|--------|----------|
| Masked (default) | `****` | Most UNSAFE fields |
| Hashed | `h:a1b2c3d4` | When correlation needed |
| Plaintext | Full value | DEBUG only, dev only |

### Metric Naming

| Component | Format |
|-----------|--------|
| Full name | `{service}_{subsystem}_{metric}_{unit}` |
| Example | `ac_token_issuance_duration_seconds` |
| Labels | `{grant_type, status}` |

### SLO Targets (Initial)

| Service | Operation | Target |
|---------|-----------|--------|
| AC | Token issuance | p99 < 350ms |
| AC | Token validation | p99 < 50ms |
| GC | Request | p95 < 200ms |
| MC | Session join | p99 < 500ms |
| MH | Audio forwarding | p99 < 30ms |

### Alert Severity

| Severity | Channel | Escalation |
|----------|---------|------------|
| Critical | PagerDuty + Slack #incidents | 15min |
| Warning | Slack #alerts | 1h |
| Info | Slack #observability | None |

---

## Guards

**Code Review**: Security specialist must review any field added to logging/tracing
**CI**: PII leakage tests for all log paths
**Runtime**: Span validation middleware filters sensitive attributes
