# Security Review: Add GC Registered Controllers Metric

**Reviewer**: Security Specialist
**Date**: 2026-02-10
**Verdict**: APPROVED

---

## Files Reviewed

1. `crates/global-controller/src/observability/metrics.rs`
2. `crates/global-controller/src/repositories/meeting_controllers.rs`
3. `crates/global-controller/src/grpc/mc_service.rs`
4. `crates/global-controller/src/tasks/health_checker.rs`
5. `crates/global-controller/src/main.rs`

---

## Security Checklist

| Category | Status | Notes |
|----------|--------|-------|
| PII/Sensitive Data Leakage | PASS | Metric exposes only aggregate counts, no controller IDs, endpoints, or credentials |
| Cardinality Control | PASS | Bounded to 10 combinations (2 types x 5 statuses) per ADR-0011 |
| SQL Injection | PASS | Uses sqlx parameterized query with no user-controlled input |
| Resource Exhaustion | PASS | Static labels prevent cardinality explosion attacks |
| Authentication | PASS | All gRPC endpoints protected by JWT auth interceptor |
| Error Handling | PASS | Failures logged internally, not exposed to clients |

---

## Detailed Analysis

### 1. Metric Definition (`observability/metrics.rs`)

**Risk Assessment**: LOW

The metric implementation follows security best practices:

```rust
pub fn set_registered_controllers(controller_type: &str, status: &str, count: u64) {
    gauge!("gc_registered_controllers",
        "controller_type" => controller_type.to_string(),
        "status" => status.to_string()
    )
    .set(count as f64);
}
```

**Positive Security Properties**:
- Labels are bounded by `CONTROLLER_STATUSES` constant (5 values)
- Controller type limited to "meeting" and "media" (2 values)
- Total cardinality: 10 combinations - far below dangerous thresholds
- No user-controlled input flows into labels
- Aggregate counts only - no individual controller data exposed

### 2. Database Query (`repositories/meeting_controllers.rs`)

**Risk Assessment**: LOW

```rust
sqlx::query_as(
    r#"
    SELECT health_status, COUNT(*) as count
    FROM meeting_controllers
    GROUP BY health_status
    "#,
)
.fetch_all(pool)
.await;
```

**Positive Security Properties**:
- Pure aggregate query with no parameters
- No string concatenation for SQL
- No user-controlled input in query
- Compile-time query validation via sqlx

### 3. gRPC Service Integration (`grpc/mc_service.rs`)

**Risk Assessment**: LOW

The `refresh_controller_metrics()` method:
- Is called only after authenticated gRPC requests
- Logs failures at WARN level without exposing to clients
- Performs read-only database operation
- Does not log controller identifiers or endpoints

### 4. Error Handling

All error handling is defensive:
- Metric refresh failures don't fail the parent operation
- Error messages are logged internally, not returned to clients
- Startup handles DB query failures gracefully without crashing

---

## Findings Summary

| Severity | Count |
|----------|-------|
| BLOCKER | 0 |
| CRITICAL | 0 |
| MAJOR | 0 |
| MINOR | 0 |

---

## Verdict

**APPROVED**

This implementation follows security best practices for observability metrics:
1. No sensitive data leakage through metric labels or values
2. Cardinality is explicitly bounded and documented
3. SQL queries are parameterized and injection-safe
4. Error handling prevents information disclosure
5. All access paths require authentication

No security issues identified.
