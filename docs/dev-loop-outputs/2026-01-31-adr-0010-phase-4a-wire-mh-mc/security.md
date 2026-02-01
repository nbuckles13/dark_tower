# Security Review Checkpoint

**Reviewer**: security
**Date**: 2026-01-31
**Status**: REQUEST_CHANGES

---

## Findings

### BLOCKER

**None** - No security vulnerabilities that would block deployment.

---

### CRITICAL

**1. Missing Captcha Validation (meetings.rs:206-207)**

**Location**: `crates/global-controller/src/handlers/meetings.rs`

**Issue**: The `get_guest_token` endpoint is PUBLIC and accepts guest join requests without captcha validation. The code includes a TODO comment but performs no actual validation:

```rust
// TODO: Validate captcha token (integration with captcha service)
// For now, we just check that it's not empty (validation handles this)
```

**Risk**: Without captcha validation, this endpoint is vulnerable to:
- Automated bot attacks creating unlimited guest tokens
- Meeting bombing (unauthorized mass entry)
- Resource exhaustion via token generation
- Bypass of rate limiting through distributed attacks

**Recommendation**:
- MUST implement captcha validation before production deployment
- Consider using reCAPTCHA v3 or hCaptcha with score-based validation
- Add configuration flag to require captcha in production
- Document as tech debt if deployment without captcha is intentional (with compensating controls)

---

**2. Hardcoded Service Token Fallback (meetings.rs:536-540 and main.rs:101)**

**Location**:
- `crates/global-controller/src/handlers/meetings.rs:536-540`
- `crates/global-controller/src/main.rs:101`

**Issue**: Service tokens use `.unwrap_or_default()` which provides an empty string as fallback:

```rust
// meetings.rs
let service_token = std::env::var("GC_SERVICE_TOKEN").unwrap_or_default();

// main.rs
let mc_client: Arc<dyn services::McClientTrait> = Arc::new(services::McClient::new(
    common::secret::SecretString::from(std::env::var("GC_SERVICE_TOKEN").unwrap_or_default()),
));
```

**Risk**:
- Empty service token allows unauthenticated AC/MC requests
- Silent failure mode - no error if token is not configured
- Violates zero-trust architecture principle

**Recommendation**:
- MUST fail fast if `GC_SERVICE_TOKEN` is not set or is empty
- Use `.expect()` or return error during startup
- Add validation that token is non-empty and meets minimum length requirements (e.g., >= 32 characters)
- Document token rotation procedure in runbook

---

**3. SQL Injection Prevention - Dynamic Query Building (meetings.rs:384, 397)**

**Location**: `crates/global-controller/src/handlers/meetings.rs`

**Current Implementation**:
```rust
async fn find_meeting_by_code(pool: &PgPool, code: &str) -> Result<MeetingRow, GcError> {
    let query = format!("{} WHERE meeting_code = $1", MEETING_SELECT_QUERY);
    let row = sqlx::query(&query)
        .bind(code)
        .fetch_optional(pool)
        .await?
```

**Assessment**:
- Uses parameterized binding ($1) correctly ✅
- String concatenation only combines static SQL fragments ✅
- No user input in concatenated portion ✅
- **VERDICT**: Safe from SQL injection

**Note**: While this pattern is secure, it's less idiomatic than inline SQL. For future code, prefer:
```rust
sqlx::query("SELECT ... FROM meetings WHERE meeting_code = $1")
```

This is a MINOR concern (see below), not CRITICAL.

---

### MAJOR

**4. Missing Input Validation - Meeting Code Length**

**Location**: `crates/global-controller/src/handlers/meetings.rs:71`

**Issue**: The `meeting_code` path parameter has no length validation before database query. While SQL injection is prevented via parameterization, there's no defense against:
- Extremely long meeting codes causing performance issues
- Database index bypass via overly long strings

**Recommendation**:
- Add maximum length validation (e.g., 64 characters)
- Validate format (alphanumeric + hyphens only)
- Return 400 Bad Request for invalid format

---

**5. Rate Limiting Documentation Gap**

**Location**: `crates/global-controller/src/handlers/meetings.rs:175`

**Issue**: The `get_guest_token` endpoint documentation claims "5 requests per minute per IP address" but:
- No rate limiting middleware is visible in the implementation
- No tower-governor or similar rate limiter in routes
- Rate limiting may be planned but not implemented

**Recommendation**:
- Verify rate limiting is actually implemented at the middleware layer
- Add integration test for rate limiting behavior
- If not implemented, this becomes a CRITICAL issue (must implement before production)

---

**6. Error Information Leakage - Internal Error Messages**

**Location**: Multiple files

**Examples**:
- `mc_assignment.rs:289`: Logs detailed error including tried MCs list
- `mh_service.rs:164-165`: "Registration failed" is generic ✅ but log includes error details

**Assessment**:
- External error messages are properly generic ✅
- Internal logs contain detailed error information ✅ (this is correct for debugging)
- No sensitive data (credentials, keys) exposed in logs ✅

**Note**: Current implementation is acceptable. Logs are internal-only and provide necessary debugging context.

---

### MINOR

**7. CSPRNG Fallback Behavior**

**Location**:
- `crates/global-controller/src/services/mh_selection.rs:167-174`
- `crates/global-controller/src/repositories.rs` (likely similar pattern)

**Issue**: When CSPRNG fails, code falls back to "first candidate":

```rust
if rng.fill(&mut random_bytes).is_err() {
    tracing::warn!(...);
    return candidates.first();
}
```

**Risk**: Predictable selection if CSPRNG fails, though this is an edge case.

**Recommendation**:
- Consider failing fast instead of degrading to predictable behavior
- CSPRNG failure indicates serious system compromise
- Alternative: Use `expect()` to panic on CSPRNG failure (CSPRNG failure should never happen in practice)

**Justification for current approach**: Availability over perfect security - allows system to degrade gracefully. Acceptable for non-security-critical load balancing.

---

**8. Guest ID Randomness Quality**

**Location**: `crates/global-controller/src/handlers/meetings.rs:517-532`

**Implementation**: Uses `ring::rand::SystemRandom` to generate UUIDv4 ✅

**Assessment**:
- Correct use of CSPRNG ✅
- Proper UUID version 4 formatting ✅
- Error handling on RNG failure ✅

**No issues** - Implementation is secure.

---

**9. Token Timeout Hardcoded**

**Location**: Multiple files

**Issue**: All timeouts are hardcoded constants:
- `MC_RPC_TIMEOUT_SECS = 10` (mc_client.rs:29)
- `DEFAULT_TOKEN_TTL_SECONDS = 900` (meetings.rs:38)
- `DEFAULT_CHECK_INTERVAL_SECONDS = 5` (mh_health_checker.rs:19)

**Risk**: Limited operational flexibility; cannot adjust timeouts without code changes.

**Recommendation**:
- Move to configuration (environment variables)
- Keep current values as defaults
- Non-blocking for this phase; can be addressed in operational hardening

---

**10. JWT Clock Skew Configuration**

**Location**: `crates/global-controller/src/main.rs:113-116`

**Implementation**: Clock skew is configurable via `config.jwt_clock_skew_seconds` ✅

**Assessment**: Proper configuration pattern ✅

**No issues** - Implementation follows security best practices.

---

**11. Channel Pooling Security**

**Location**: `crates/global-controller/src/services/mc_client.rs:91-122`

**Implementation**: gRPC channel pooling with concurrent access via RwLock.

**Security Considerations**:
- Channels are reused per endpoint ✅
- Service token is protected by SecretString ✅
- Token exposed only during metadata insertion (line 177) ✅
- No channel poisoning risk (channels are cryptographically isolated)

**No issues** - Implementation is secure.

---

**12. Database Connection Pool Configuration**

**Location**: `crates/global-controller/src/main.rs:76-90`

**Configuration**:
```rust
.max_connections(20)
.min_connections(2)
.acquire_timeout(Duration::from_secs(5))
.idle_timeout(Duration::from_secs(600))
.max_lifetime(Duration::from_secs(1800))
```

**Security Assessment**:
- Proper connection limits prevent resource exhaustion ✅
- Query timeout added via connection string (5 seconds) ✅
- No credential exposure in connection string handling ✅

**No issues** - Configuration is secure and production-ready.

---

**13. MH Health Checker - Stale Handler Detection**

**Location**: `crates/global-controller/src/tasks/mh_health_checker.rs:54-75`

**Security Consideration**: Marks handlers unhealthy if heartbeat is stale.

**Implementation**:
- Configurable staleness threshold ✅
- Graceful error handling (logs but continues) ✅
- Does not mark draining handlers as unhealthy ✅ (see integration test line 270)

**No issues** - Availability protection is correctly implemented.

---

**14. Input Validation - MH Registration**

**Location**: `crates/global-controller/src/grpc/mh_service.rs:46-120`

**Validation Implemented**:
- Handler ID: max 255 chars, alphanumeric + hyphens/underscores ✅
- Region: max 50 chars ✅
- Endpoints: max 255 chars, must start with http/https/grpc ✅
- max_streams: must be > 0 ✅

**Assessment**: Comprehensive input validation ✅

**No issues** - Well-implemented validation with clear error messages.

---

**15. gRPC Authentication Layer**

**Location**: `crates/global-controller/src/main.rs:124` and `grpc/auth_layer/`

**Implementation**: JWT authentication via `GrpcAuthLayer` applied to all gRPC services.

**Security Coverage**:
- Both MC and MH services protected ✅
- Uses same JWT validator as HTTP layer ✅
- Consistent authentication across protocols ✅

**No issues** - Zero-trust architecture correctly applied.

---

## Verdict

**Overall Assessment**: The implementation demonstrates strong security fundamentals with proper input validation, parameterized queries, CSPRNG usage, and zero-trust architecture. However, two CRITICAL issues must be addressed before production deployment.

**CRITICAL Issues Summary**:
1. **Missing captcha validation** - Public endpoint vulnerable to automated attacks
2. **Service token fallback to empty string** - Violates zero-trust; allows unauthenticated requests

**Recommendation**: **REQUEST_CHANGES**

**Action Items**:
1. **MUST FIX (before merge)**:
   - Replace `unwrap_or_default()` with `.expect("GC_SERVICE_TOKEN must be set")` in both locations
   - Add token validation (non-empty, minimum length)

2. **MUST ADDRESS (before production)**:
   - Implement captcha validation OR document compensating controls
   - Add rate limiting verification tests

3. **SHOULD FIX (this sprint)**:
   - Add meeting_code length/format validation
   - Verify rate limiting implementation exists

4. **NICE TO HAVE (future)**:
   - Make timeouts configurable via environment
   - Consider failing fast on CSPRNG errors instead of fallback

**Security Posture**: Good foundation with critical gaps that are easily fixable. Once the service token issue is resolved, the implementation will be production-ready pending captcha validation strategy.

---

## Positive Security Highlights

1. ✅ **Proper CSPRNG usage** for guest IDs and weighted selection
2. ✅ **Comprehensive input validation** on gRPC MH registration
3. ✅ **Parameterized SQL queries** preventing injection
4. ✅ **SecretString protection** for service tokens in memory
5. ✅ **JWT authentication** consistently applied across HTTP and gRPC
6. ✅ **Generic error messages** to external clients (no information leakage)
7. ✅ **Connection pooling limits** prevent resource exhaustion
8. ✅ **Graceful degradation** in health checker (logs errors, continues)
9. ✅ **Zero-trust architecture** - all MC/MH communication authenticated
10. ✅ **Excellent test coverage** for auth edge cases (expired tokens, algorithm confusion, etc.)

