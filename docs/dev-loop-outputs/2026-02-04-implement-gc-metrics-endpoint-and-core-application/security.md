# Security Review: GC Metrics Implementation

**Reviewer**: Security Specialist
**Date**: 2026-02-04
**Files Reviewed**: 12 files (observability, handlers, middleware, routes, tests)

## Summary

The GC metrics implementation demonstrates strong security practices with privacy-by-default instrumentation (`#[instrument(skip_all)]`) consistently applied across all handlers and middleware. The `/metrics` endpoint follows industry-standard practice of being unauthenticated for Prometheus scraping. Metric labels are bounded and do not contain PII - dynamic values like meeting codes and UUIDs are normalized to placeholders to prevent cardinality explosion while maintaining privacy.

## Findings

### CRITICAL Security Issues

**None**

### MAJOR Security Issues

**None**

### MINOR Security Issues

**1. Logging of user_id and meeting_id in warn/info logs** (MINOR)
- **File**: `crates/global-controller/src/handlers/meetings.rs:98-103, 150-158, 237-244, 273-281, 350-356, 365-370`
- **Issue**: While handlers use `#[instrument(skip_all)]`, they log `user_id`, `meeting_id`, `guest_id`, `mc_id`, and `mh_id` in warn/info statements.
- **Assessment**: These are UUIDs, not PII directly, but they could be used for correlation. This is acceptable for operational debugging but worth noting.
- **Risk Level**: Low - UUIDs are pseudonymous identifiers, not PII. Logging them for debugging is standard practice.
- **Recommendation**: Document that these logs should not be retained indefinitely or correlated with user data without privacy controls.

### TECH_DEBT Issues

**1. Unused metric functions marked with `#[allow(dead_code)]`**
- **File**: `crates/global-controller/src/observability/metrics.rs:137, 167, 192, 209, 228, 249, 274`
- **Issue**: Several metric functions (record_mc_assignment, record_db_query, record_token_refresh, etc.) are defined but not yet wired in.
- **Assessment**: Not a security issue - functions are well-designed with proper label bounds. They will be wired in as instrumentation expands.
- **Recommendation**: Track in TODO.md for future work.

## PII Protection Review

### Privacy-by-Default Compliance: PASS

| Location | `skip_all` Applied | Notes |
|----------|-------------------|-------|
| `handlers/metrics.rs:27` | Yes | `#[tracing::instrument(skip_all, name = "gc.metrics.scrape")]` |
| `handlers/meetings.rs:64-72` | Yes | `skip_all` with safe fields (method, endpoint) |
| `handlers/meetings.rs:203-211` | Yes | `skip_all` with safe fields |
| `handlers/meetings.rs:323-331` | Yes | `skip_all` with safe fields |
| `handlers/me.rs:51-59` | Yes | `skip_all` with safe fields |
| `handlers/health.rs:33` | Yes | `#[instrument(skip_all, name = "gc.health.check")]` |
| `middleware/http_metrics.rs` | Yes | No instrumentation needed - pure measurement |
| `middleware/auth.rs:38` | Yes | `#[instrument(skip_all, name = "gc.middleware.auth")]` |

### Metric Label Safety: PASS

| Metric | Labels | PII Risk |
|--------|--------|----------|
| `gc_http_requests_total` | method, endpoint (normalized), status_code | None - all bounded |
| `gc_http_request_duration_seconds` | method, endpoint (normalized), status | None - all bounded |
| `gc_mc_assignments_total` | status, rejection_reason | None - bounded enums |
| `gc_mc_assignment_duration_seconds` | status | None - bounded enum |
| `gc_db_queries_total` | operation, status | None - bounded enums |
| `gc_db_query_duration_seconds` | operation | None - bounded enum |
| `gc_token_refresh_total` | status | None - bounded enum |
| `gc_token_refresh_failures_total` | error_type | None - bounded enum |
| `gc_errors_total` | operation, error_type, status_code | None - all bounded |
| `gc_grpc_mc_calls_total` | method, status | None - bounded |
| `gc_mh_selections_total` | status, has_backup | None - bounded |

### Endpoint Path Normalization: PASS

The `normalize_endpoint` function in `metrics.rs:74-122` correctly normalizes dynamic paths:
- `/api/v1/meetings/abc123` -> `/api/v1/meetings/{code}` (meeting code masked)
- `/api/v1/meetings/uuid-here/settings` -> `/api/v1/meetings/{id}/settings` (UUID masked)
- Unknown paths -> `/other` (prevents unbounded cardinality)

## Positive Security Highlights

1. **Privacy-by-default instrumentation**: All handlers use `#[instrument(skip_all)]` - this is the correct pattern per ADR-0011.

2. **Metrics endpoint follows industry standard**: The `/metrics` endpoint is correctly unauthenticated, following Prometheus best practices. Metrics do not contain secrets or PII.

3. **Bounded cardinality**: All metric labels are explicitly bounded per documentation in `metrics.rs:8-15` and `docs/observability/metrics/gc.md:305-316`. Total estimated cardinality is ~400 time series.

4. **No secrets in metrics**: Metrics do not expose tokens, passwords, or cryptographic material.

5. **CSPRNG for guest IDs**: Guest ID generation uses `ring::rand::SystemRandom` (lines 542-556 in meetings.rs), which is cryptographically secure.

6. **Proper error handling**: All metric recording functions use `Result<T, E>` and don't panic.

7. **Query safety**: All database queries use parameterized sqlx queries - no SQL injection risk.

8. **Test metrics isolation**: Test harness correctly uses `OnceLock` for shared metrics handle, preventing test interference.

## Recommendations

### Approved for Merge (No Blocking Issues)

1. **Document log retention policy**: Add note in ops docs that meeting/user ID logs should have retention limits.

2. **Wire in remaining metrics**: As development continues, wire in the `#[allow(dead_code)]` metric functions for complete observability.

3. **Add rate limiting to /metrics**: Consider adding basic rate limiting to prevent scrape abuse (not blocking - standard Prometheus setup handles this).

## Recommendation

- [x] SECURE - Implementation follows security best practices

---

## Structured Output

```
verdict: APPROVED
finding_count:
  blocker: 0
  critical: 0
  major: 0
  minor: 1
  tech_debt: 1
checkpoint_exists: true
summary: GC metrics implementation demonstrates strong security practices with privacy-by-default instrumentation consistently applied. All handlers use skip_all pattern, metric labels are bounded without PII, and endpoint paths are properly normalized to prevent cardinality explosion. The /metrics endpoint correctly follows industry standard of being unauthenticated. One minor finding (UUID logging in handlers) and one tech debt item (unused metric functions) noted - neither blocking.
```
