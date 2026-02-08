# Test Specialist - Patterns

Testing patterns worth documenting for Dark Tower codebase.

---

## Pattern: Defense-in-Depth Validation Tests
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/crypto/mod.rs`

When a function validates input that config already validated, test that the function still rejects invalid inputs. In `hash_client_secret()`, cost validation exists both in config AND the function. Test both layers independently. This catches bugs if callers bypass config.

---

## Pattern: Cross-Version Verification Tests
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/crypto/mod.rs`

For migration scenarios (bcrypt cost changes, algorithm upgrades), test that old artifacts verify correctly with new code. The `test_hash_verification_works_across_cost_factors` test creates hashes at costs 10-14 and verifies ALL of them work regardless of current config. Essential for zero-downtime deployments.

---

## Pattern: SecretBox Debug Redaction Tests
**Added**: 2026-01-12
**Related files**: `crates/ac-service/src/crypto/mod.rs`, `crates/ac-service/src/models/mod.rs`, `crates/ac-service/src/config.rs`

When struct contains `SecretBox<T>` or `SecretString`, test the Debug impl:
```rust
#[test]
fn test_struct_debug_redacts_secret() {
    let s = MyStruct { secret: SecretString::from("hunter2"), public: "visible" };
    let debug = format!("{:?}", s);
    assert!(debug.contains("[REDACTED]"), "Secret should be redacted");
    assert!(!debug.contains("hunter2"), "Actual value must not appear");
    assert!(debug.contains("visible"), "Public fields should appear");
}
```
This prevents accidental credential leaks in logs.

---

## Pattern: Wrapper Type Refactor Verification
**Added**: 2026-01-12, **Updated**: 2026-01-28
**Related files**: `crates/ac-service/tests/`, `crates/meeting-controller/tests/`

When refactoring raw types to wrapper types (e.g., `Vec<u8>` to `SecretBox<Vec<u8>>`):
1. Search all usages of the struct being modified
2. Update construction sites to wrap values: `SecretBox::new(Box::new(value))`
3. Update access sites to unwrap: `.expose_secret()`
4. **Verify test files are included in mod.rs** - orphaned tests won't catch type errors
5. Run `cargo test` and verify expected test count executes

**Key insight from Phase 6c review**: This is a type-level refactor where semantic behavior is preserved. Test updates are mechanical (wrapping at construction, unwrapping at usage), not behavioral. The compiler's type checker is the primary verification mechanism - if tests don't compile, the type mismatch is caught immediately. No new test cases needed for SecretBox migration itself, though test helpers may need updating. Example: When `SessionBindingManager.master_secret` changed from `Vec<u8>` to `SecretBox<Vec<u8>>`, 28 existing tests compiled and ran unchanged after updating constructor calls to wrap values.

---

## Pattern: NetworkPolicy Positive/Negative Test Pair
**Added**: 2026-01-13
**Related files**: `crates/env-tests/tests/40_resilience.rs`, `crates/env-tests/src/canary.rs`

When testing NetworkPolicy enforcement, always implement paired tests:
1. **Positive test** (same namespace): Deploy canary with allowed labels, verify connectivity WORKS
2. **Negative test** (cross namespace): Deploy canary in different namespace, verify connectivity BLOCKED

Interpretation matrix:
- Positive passes, negative fails = NetworkPolicy working correctly
- Both pass = NetworkPolicy NOT enforced (security gap!)
- Positive fails = Service down OR NetworkPolicy misconfigured (blocking all traffic)

Always run positive test first to validate test infrastructure works.

---

## Pattern: Cluster-Dependent Test Structure
**Added**: 2026-01-13
**Related files**: `crates/env-tests/tests/*.rs`

For tests requiring a running cluster, follow this structure:
```rust
#![cfg(feature = "flows")]  // Feature-gate to prevent accidental runs

async fn cluster() -> ClusterConnection {
    ClusterConnection::new()
        .await
        .expect("Failed to connect - ensure port-forwards are running")
}

#[tokio::test]
async fn test_feature() {
    let cluster = cluster().await;
    let client = ServiceClient::new(&cluster.service_base_url);
    // ... test logic
}
```
Use feature gates (smoke, flows, observability, resilience) to categorize test execution time.

---

## Pattern: CanaryPod for In-Cluster Testing
**Added**: 2026-01-13
**Related files**: `crates/env-tests/src/canary.rs`

For testing cluster-internal behavior (NetworkPolicies, service mesh, etc.), use CanaryPod pattern:
```rust
let canary = CanaryPod::deploy("target-namespace").await?;
let can_reach = canary.can_reach("http://service:port/health").await;
canary.cleanup().await?;  // Also cleaned on Drop
```
Key design decisions:
- Use `std::process::Command` to call kubectl (not async kubectl client)
- Implement `Drop` for automatic cleanup even on test panic
- Use `AtomicBool` to prevent double-cleanup
- Generate unique pod names with UUIDs to avoid collisions

---

## Pattern: Test Server Harness for Integration HTTP Testing
**Added**: 2026-01-14
**Related files**: `crates/gc-test-utils/src/server_harness.rs`, `crates/global-controller/tests/health_tests.rs`

For HTTP service integration testing, create a reusable server harness that:
1. Spawns a real HTTP server on a random available port (127.0.0.1:0)
2. Provides access to the database pool for assertions
3. Implements Drop for automatic cleanup
4. Uses `#[sqlx::test(migrations = "...")]` for database setup

```rust
pub struct TestGcServer {
    addr: SocketAddr,
    pool: PgPool,
    _handle: JoinHandle<()>,
}

impl TestGcServer {
    pub async fn spawn(pool: PgPool) -> Result<Self, anyhow::Error> {
        // 1. Create config from test vars
        let config = Config::from_vars(&test_vars)?;

        // 2. Build app state and routes
        let app = routes::build_routes(Arc::new(AppState { pool, config }));

        // 3. Bind to random port
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;

        // 4. Spawn server in background
        let handle = tokio::spawn(async move {
            axum::serve(listener, app.into_make_service()).await.ok()
        });

        Ok(Self { addr, pool, _handle: handle })
    }

    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }
}
```

Key benefits:
- Real HTTP server, not mocked
- Runs database migrations automatically (via sqlx::test)
- Random ports prevent conflicts in parallel test execution
- Drop impl ensures server stops on test completion
- Direct database pool access for assertions

---

## Pattern: Layered JWT Testing (Defense-in-Depth)
**Added**: 2026-01-15
**Related files**: `crates/global-controller/tests/auth_tests.rs`

JWT security requires testing at multiple layers, not just the happy path:
1. **Token algorithm layer**: Reject `alg:none`, `alg:HS256`, accept only `alg:EdDSA`
2. **JWK structure layer**: Reject `kty != "OKP"`, reject `alg != "EdDSA"` (when present)
3. **Signature verification layer**: Reject tampered payloads
4. **Claims validation layer**: Reject expired tokens, invalid iat, missing required fields

Each layer is independent - a compromised JWKS endpoint or network MITM could bypass token-level checks, which is why JWK structure validation is essential. Test each layer separately:

```rust
#[test]
fn test_algorithm_confusion_attack_alg_none_rejected() {
    // Token layer: attack via header algorithm field
    let token = create_token_with_header_override(json!({"alg": "none", "typ": "JWT"}), valid_claims);
    assert!(validate_token(&token).is_err());
}

#[test]
fn test_jwk_structure_validation_rejects_wrong_kty() {
    // JWK layer: attack via key type mismatch
    let jwk = create_jwk_with_kty("RSA");  // Wrong type for EdDSA
    assert_eq!(verify_token_with_jwk(valid_token, &jwk).status(), 401);
}

#[test]
fn test_signature_validation_detects_tampering() {
    // Signature layer: payload modified after signing
    let tampered_payload = modify_jwt_payload(valid_token, |p| p["sub"] = "attacker");
    assert!(validate_token(&tampered_payload).is_err());
}
```

Why this matters: Algorithm confusion (CVE-2016-10555, CVE-2017-11424) is a real attack. Testing only "EdDSA works" misses the attacks that use `none` or `HS256`.

---

## Pattern: Rate Limiting Tests via Loop
**Added**: 2026-01-15
**Related files**: `crates/ac-service/tests/integration/user_auth_tests.rs`

Test rate limiting by sending requests in a loop until lockout triggers:
```rust
for i in 0..6 {
    let response = client.post(...).send().await?;
    if i < 5 {
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    } else {
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    }
}
```
This approach validates that rate limiting kicks in after threshold without hardcoding timing assumptions. For registration, use unique emails per attempt; for login, use same invalid password.

---

## Pattern: Cross-Service Client Fixture with Graceful Service Availability
**Added**: 2026-01-18
**Related files**: `crates/env-tests/src/fixtures/gc_client.rs`, `crates/env-tests/src/fixtures/auth_client.rs`

When testing cross-service flows, create service client fixtures that handle optional service availability:
```rust
// Check if service is available before running tests
if !cluster.is_gc_available().await {
    println!("SKIPPED: GC not deployed");
    return;
}
```
This allows tests to run even during phased rollouts where not all services are deployed. Pattern elements: (1) Health check method, (2) `is_X_available()` boolean wrapper, (3) Tests skip gracefully with message. Essential for incremental development.

---

## Pattern: Concurrent Race Condition Testing with Barrier
**Added**: 2026-01-21
**Related files**: `crates/global-controller/tests/meeting_assignment_tests.rs`

For testing atomic operations under concurrent load, use `tokio::sync::Barrier` to synchronize multiple tasks before they all attempt the same operation:

```rust
use std::sync::Arc;
use tokio::sync::Barrier;

#[sqlx::test(migrations = "../../migrations")]
async fn test_concurrent_race_condition(pool: PgPool) -> Result<(), anyhow::Error> {
    let pool = Arc::new(pool);
    let num_concurrent = 10;
    let barrier = Arc::new(Barrier::new(num_concurrent));

    let handles: Vec<_> = (0..num_concurrent)
        .map(|i| {
            let pool = Arc::clone(&pool);
            let barrier = Arc::clone(&barrier);
            tokio::spawn(async move {
                // Wait for all tasks to be ready
                barrier.wait().await;
                // All tasks now proceed simultaneously
                some_atomic_operation(&pool, &format!("caller-{}", i)).await
            })
        })
        .collect();

    let results: Vec<_> = futures::future::join_all(handles)
        .await
        .into_iter()
        .map(|r| r.expect("task should not panic"))
        .collect();

    // Assert all succeeded AND returned consistent results
    let unique_results: HashSet<_> = results.iter().collect();
    assert_eq!(unique_results.len(), 1, "Atomic operation should be consistent");
}
```

Key elements:
- `Barrier::new(N)` ensures N tasks synchronize before proceeding
- `Arc` wrapping for pool and barrier to share across tasks
- Verify ALL concurrent attempts succeed (not just some)
- Verify ALL return the same result (consistency check)
- Verify database state matches expectations (e.g., only one row created)

This pattern is essential for validating atomic CTEs, distributed locks, and idempotent operations.

---

## Pattern: RPC Retry Testing with Mixed Success/Failure Sequences
**Added**: 2026-01-24, **Updated**: 2026-01-25
**Related files**: `crates/global-controller/tests/meeting_assignment_tests.rs`, `crates/meeting-controller/tests/grpc_client_tests.rs`

When testing RPC retry logic, test sequences where some calls fail before eventual success - not just all-fail or all-succeed:

```rust
#[test]
async fn test_retry_with_mixed_rejection_then_accept(pool: PgPool) {
    // Setup: 3 MHs - first two will reject, third will accept
    let mh1 = create_mh(&pool, "mh-1", HealthStatus::Healthy).await;
    let mh2 = create_mh(&pool, "mh-2", HealthStatus::Healthy).await;
    let mh3 = create_mh(&pool, "mh-3", HealthStatus::Healthy).await;

    // Mock RPC responses: mh-1 rejects, mh-2 rejects, mh-3 accepts
    let mock = MockRpcClient::new()
        .when_called(&mh1.id).return_error(RpcError::Rejected)
        .when_called(&mh2.id).return_error(RpcError::Rejected)
        .when_called(&mh3.id).return_ok(AssignmentResponse::Accepted);

    let result = assign_with_retry(&pool, meeting_id, &mock).await;

    // Verify: assignment succeeded to third MH
    assert!(result.is_ok());
    assert_eq!(result.unwrap().mh_id, mh3.id);

    // Verify: first two MHs were tried before success
    assert_eq!(mock.call_count(&mh1.id), 1);
    assert_eq!(mock.call_count(&mh2.id), 1);
    assert_eq!(mock.call_count(&mh3.id), 1);
}
```

Key test scenarios for retry logic:
- All succeed on first try (happy path)
- All fail (exhausts retries)
- **Mixed: some fail, eventual success** (the often-missed case)
- Fail with different error types (transient vs permanent)
- **Backoff timing**: Verify exponential backoff delays (use tokio::time::pause for determinism)
- **Circuit breaker integration**: If using circuit breakers, verify they open after failures

The mixed scenario catches bugs where retry counter is incorrectly updated or candidate list isn't properly iterated. Also applied in Phase 6c MC review for gRPC client retry testing.

---

## Pattern: Enum State Boundary Value Testing
**Added**: 2026-01-24
**Related files**: `crates/global-controller/tests/meeting_assignment_tests.rs`

When code handles multiple enum states (health status, meeting status), test boundary/transitional states explicitly:

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum HealthStatus {
    Healthy,
    Degraded,  // <-- Often forgotten in tests
    Unhealthy,
}

#[test]
async fn test_load_report_with_degraded_health_status(pool: PgPool) {
    // Setup: MC with Degraded status (not Healthy, not Unhealthy)
    let mc = create_mc(&pool, "mc-1", HealthStatus::Degraded).await;

    // Action: Update load (should work for Degraded)
    let result = update_mc_load(&pool, &mc.id, LoadReport { cpu: 50.0 }).await;

    // Assert: Degraded MCs can report load (they're still functional)
    assert!(result.is_ok());

    // Verify: Load was actually updated
    let updated = get_mc(&pool, &mc.id).await?;
    assert_eq!(updated.current_load.cpu, 50.0);
}
```

Why this matters:
- `Degraded` is a boundary state - not fully healthy, not fully unhealthy
- Code often has `if status == Healthy` or `if status != Unhealthy` - these handle Degraded differently
- Without explicit tests, Degraded behavior is undefined and may break silently

Pattern for multi-state enums:
1. Test each state in isolation (Healthy, Degraded, Unhealthy)
2. Test state transitions (Healthy -> Degraded -> Unhealthy)
3. Test operations that should work in boundary states
4. Test operations that should fail in boundary states

---

## Pattern: Weighted Selection Algorithm Edge Case Testing
**Added**: 2026-01-24
**Related files**: `crates/global-controller/tests/meeting_assignment_tests.rs`

When testing weighted random selection (load balancing, capacity allocation), test these edge cases:

```rust
// Edge case 1: All candidates at maximum capacity
#[test]
async fn test_get_candidate_mhs_all_at_max_capacity(pool: PgPool) {
    // All MHs at 100% capacity
    create_mh_with_load(&pool, "mh-1", 100.0).await;
    create_mh_with_load(&pool, "mh-2", 100.0).await;
    create_mh_with_load(&pool, "mh-3", 100.0).await;

    let candidates = get_candidate_mhs(&pool).await?;

    // Behavior should be defined: empty list, error, or all included with equal weight
    assert!(candidates.is_empty() || candidates.len() == 3);
}

// Edge case 2: Load ratio at exact boundary
#[test]
async fn test_candidate_selection_load_ratio_boundary(pool: PgPool) {
    // Test at exact threshold boundary (e.g., 80% threshold)
    create_mh_with_load(&pool, "mh-at-79", 79.0).await;   // Just below
    create_mh_with_load(&pool, "mh-at-80", 80.0).await;   // At threshold
    create_mh_with_load(&pool, "mh-at-81", 81.0).await;   // Just above

    let candidates = get_candidate_mhs(&pool).await?;

    // Verify boundary behavior is consistent
    // If threshold is "< 80%", only 79 should be included
    // If threshold is "<= 80%", both 79 and 80 should be included
    assert!(candidates.iter().any(|c| c.id == "mh-at-79"));
    // Document expected behavior for at-threshold case
}

// Edge case 3: Single candidate with weight calculation
#[test]
async fn test_single_candidate_weighted_selection(pool: PgPool) {
    // Only one MH available
    create_mh_with_load(&pool, "mh-only", 50.0).await;

    let selected = select_mh_weighted(&pool).await?;

    // Must return the only candidate, regardless of weight
    assert_eq!(selected.id, "mh-only");
}
```

Key edge cases for weighted selection:
1. **All at max capacity**: Division by zero in weight calculation?
2. **Boundary values**: Off-by-one in threshold comparisons
3. **Single candidate**: Weight calculation degenerates but should still work
4. **Zero weight**: What if weight formula produces 0 for some candidates?
5. **Equal weights**: Selection should be uniform random

---

## Pattern: Exhaustive Error Variant Testing
**Added**: 2026-01-25
**Related files**: `crates/meeting-controller/src/errors.rs`

When error enums map to protocol codes or client messages, test EVERY variant exhaustively:

```rust
#[test]
fn test_error_code_mapping() {
    // Internal errors -> 6
    assert_eq!(McError::Redis("conn failed".to_string()).error_code(), 6);
    assert_eq!(McError::Config("bad config".to_string()).error_code(), 6);
    assert_eq!(McError::Internal.error_code(), 6);
    assert_eq!(McError::FencedOut("stale".to_string()).error_code(), 6);

    // Auth errors -> 2
    assert_eq!(McError::SessionBinding(SessionBindingError::TokenExpired).error_code(), 2);
    assert_eq!(McError::JwtValidation("expired".to_string()).error_code(), 2);

    // ... every single variant ...
}
```

Why exhaustive testing matters:
- New variants added later get no test coverage if match arms have wildcards
- Protocol codes must be stable (client depends on them)
- Missing test = silent regression when someone changes a match arm

Also test:
- `Display` formatting for each variant
- `client_message()` doesn't leak internal details (IP addresses, secret names, etc.)
- `From` trait implementations (e.g., `SessionBindingError` into `McError`)

---

## Pattern: Deterministic Time-Based Tests with tokio::time::pause
**Added**: 2026-01-25
**Related files**: `crates/meeting-controller/tests/session_actor_tests.rs`

For testing time-dependent behavior (grace periods, timeouts, expiration), use `tokio::time::pause()` to gain deterministic control over time:

```rust
#[tokio::test(start_paused = true)]
async fn test_grace_period_boundary() {
    let timeout = Duration::from_secs(30);

    // Start operation that has a 30s grace period
    let handle = spawn_with_grace_period(timeout);

    // Advance to just before timeout (29s) - should still be active
    tokio::time::advance(Duration::from_secs(29)).await;
    assert!(handle.is_active(), "Should be active at 29s");

    // Advance past timeout (35s total) - should have expired
    tokio::time::advance(Duration::from_secs(6)).await;
    assert!(!handle.is_active(), "Should expire after 30s");
}
```

Key benefits:
- Tests run instantly (no waiting 30 real seconds)
- Deterministic behavior (no race conditions)
- Precise boundary testing (exactly at threshold)
- Works with tokio::time::timeout, sleep, interval

Use `start_paused = true` in test attribute OR call `tokio::time::pause()` at test start.

---

## Pattern: HMAC/Cryptographic Validation Exhaustive Testing
**Added**: 2026-01-25
**Related files**: `crates/meeting-controller/src/session/binding.rs`

When testing HMAC or cryptographic token validation, test each field that contributes to the signature independently:

```rust
// Token binds: session_id + correlation_id + nonce
fn test_token_validation_exhaustive() {
    let valid_token = create_valid_token(session_id, correlation_id, nonce);

    // Test each bound field independently
    assert!(validate(wrong_session_id, correlation_id, nonce, &valid_token).is_err());
    assert!(validate(session_id, wrong_correlation_id, nonce, &valid_token).is_err());
    assert!(validate(session_id, correlation_id, wrong_nonce, &valid_token).is_err());

    // Also test combined mismatches
    assert!(validate(wrong_session_id, wrong_correlation_id, nonce, &valid_token).is_err());
}
```

Each field mismatch should return an error, and the error type should be consistent (e.g., `InvalidToken`) to avoid leaking which field failed. This pattern catches bugs where only some fields are actually included in the signature.

---

## Pattern: Lua Script Behavioral Testing (Not Just Structural)
**Added**: 2026-01-25
**Related files**: `crates/meeting-controller/tests/redis_lua_tests.rs`, `crates/meeting-controller/src/redis_scripts.rs`

When testing Redis Lua scripts, behavioral tests are more valuable than structural tests. Don't just verify the script runs - verify it produces correct results under various conditions:

```rust
// Structural test (weak): script returns something
#[test]
fn test_script_executes() {
    let result = script.invoke(&mut conn).await;
    assert!(result.is_ok());
}

// Behavioral test (strong): script fences correctly
#[test]
fn test_fencing_script_rejects_stale_generation() {
    // Setup: current fencing generation is 5
    redis.set_fencing_generation("session-1", 5).await?;

    // Action: try to write with generation 3 (stale)
    let result = fenced_write_script(&mut conn, "session-1", 3, "data").await;

    // Assert: write rejected because generation is stale
    assert_eq!(result, Err(FencingError::StaleGeneration { current: 5, attempted: 3 }));
}
```

Test matrix for fencing scripts:
- Current generation (accept)
- Higher generation (accept, update)
- Lower generation (reject as stale)
- No existing generation (first write, accept)

This caught real bugs where scripts would silently fail validation checks.

---

## Pattern: Capacity Check Testing with Atomics
**Added**: 2026-01-25
**Related files**: `crates/meeting-controller/tests/capacity_tests.rs`

When testing capacity enforcement with atomic counters, verify both the business logic and atomic semantics:

```rust
// Test 1: Single capacity check (basic)
#[test]
fn test_capacity_allows_under_limit() {
    let capacity = AtomicCapacity::new(100);
    assert!(capacity.try_reserve(1).is_ok());
}

// Test 2: Concurrent capacity exhaustion (atomics behavior)
#[test]
fn test_concurrent_reservations_respect_limit() {
    let capacity = Arc::new(AtomicCapacity::new(10));
    let barrier = Arc::new(Barrier::new(20)); // More requesters than capacity

    let handles: Vec<_> = (0..20).map(|_| {
        let cap = Arc::clone(&capacity);
        let bar = Arc::clone(&barrier);
        tokio::spawn(async move {
            bar.wait().await;
            cap.try_reserve(1)
        })
    }).collect();

    let results: Vec<_> = join_all(handles).await;
    let successes = results.iter().filter(|r| r.is_ok()).count();
    assert_eq!(successes, 10, "Exactly capacity reservations should succeed");
}

// Test 3: Draining state behavior
#[test]
fn test_draining_rejects_new_reservations() {
    let capacity = AtomicCapacity::new(100);
    capacity.set_draining(true);
    assert!(capacity.try_reserve(1).is_err(), "Draining should reject new work");
}
```

Draining is often overlooked - test that capacity checks respect draining state separate from numeric limits.

---

## Pattern: Actor Lifecycle Testing (spawn, shutdown, cancellation)
**Added**: 2026-01-25
**Related files**: `crates/meeting-controller/src/session/actor.rs`

For actor-based systems, test the full lifecycle explicitly:

```rust
// 1. Spawn test: actor starts and is responsive
#[tokio::test]
async fn test_actor_spawn_and_handle_valid() {
    let (handle, rx) = spawn_actor(config).await;
    assert!(!handle.is_finished());
    handle.send(Ping).await.expect("should be responsive");
}

// 2. Graceful shutdown: actor processes pending work before stopping
#[tokio::test]
async fn test_actor_graceful_shutdown() {
    let (handle, _) = spawn_actor(config).await;
    handle.shutdown().await;
    assert!(handle.is_finished());
    // Verify cleanup occurred (resources released, connections closed)
}

// 3. Cancellation: actor handles abrupt termination
#[tokio::test]
async fn test_actor_cancellation() {
    let (handle, _) = spawn_actor(config).await;
    handle.abort();
    let result = handle.await;
    assert!(result.is_err()); // JoinError::Cancelled
}

// 4. Recovery: actor restarts after failure
#[tokio::test]
async fn test_actor_restart_after_panic() {
    let (handle, _) = spawn_actor_with_supervision(config).await;
    handle.send(CausePanic).await;
    // Wait for supervisor to restart
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(!handle.is_finished()); // Supervisor restarted actor
}
```

This ensures actors behave correctly at boundaries, not just during normal operation.

---

## Pattern: gRPC/tonic Interceptor Testing
**Added**: 2026-01-25
**Related files**: `crates/meeting-controller/tests/grpc_interceptor_tests.rs`, `crates/meeting-controller/src/grpc/auth_interceptor.rs`

When testing gRPC interceptors (auth, tracing, rate limiting), cover these edge cases that are often missed:

```rust
// Edge case 1: Empty Authorization header
#[test]
fn test_interceptor_rejects_empty_auth_header() {
    let mut request = Request::new(());
    request.metadata_mut().insert("authorization", "".parse().unwrap());
    assert!(interceptor.call(request).is_err());
}

// Edge case 2: Malformed Bearer prefix (wrong case, extra spaces)
#[test]
fn test_interceptor_rejects_malformed_bearer() {
    for malformed in ["bearer token", "BEARER token", "Bearer  token", "Bearer\ttoken"] {
        let mut request = Request::new(());
        request.metadata_mut().insert("authorization", malformed.parse().unwrap());
        // Should reject - only "Bearer <token>" (single space, proper case) is valid
        assert!(interceptor.call(request).is_err());
    }
}

// Edge case 3: Valid format but expired/invalid token
#[test]
fn test_interceptor_validates_token_contents() {
    let mut request = Request::new(());
    let expired_token = create_expired_jwt();
    request.metadata_mut().insert(
        "authorization",
        format!("Bearer {}", expired_token).parse().unwrap()
    );
    assert!(interceptor.call(request).is_err());
}
```

Common bugs caught:
- Case-sensitive "Bearer" parsing (HTTP headers are case-insensitive, but token format isn't)
- Multiple space handling
- Empty token after "Bearer " prefix
- Missing validation of token contents (just checking format is present)

---

## Pattern: Error Body Sanitization in Test Clients
**Added**: 2026-01-18
**Related files**: `crates/env-tests/src/fixtures/gc_client.rs`

API client fixtures should sanitize error response bodies to prevent credential leaks in test logs:
```rust
fn sanitize_error_body(body: &str) -> String {
    static JWT_PATTERN: LazyLock<Regex> = LazyLock::new(||
        Regex::new(r"eyJ[A-Za-z0-9_-]+\.eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+").unwrap()
    );
    static BEARER_PATTERN: LazyLock<Regex> = LazyLock::new(||
        Regex::new(r"Bearer\s+eyJ[A-Za-z0-9_-]+").unwrap()
    );

    let result = JWT_PATTERN.replace_all(body, "[JWT_REDACTED]");
    let result = BEARER_PATTERN.replace_all(&result, "[BEARER_REDACTED]");
    if result.len() > 256 {
        format!("{}...[truncated]", &result[..256])
    } else {
        result.to_string()
    }
}
```
Apply sanitization in error handling paths. This caught credential leaks that custom Debug alone missed.

---

## Pattern: Type-Level Refactor Verification (Compiler-Verified)
**Added**: 2026-01-28, **Updated**: 2026-01-29
**Related files**: `crates/meeting-controller/tests/`, `crates/ac-service/tests/`, `crates/global-controller/tests/`

When refactoring raw types to wrapped types (e.g., `Vec<u8>` → `SecretBox<Vec<u8>>`, `Internal` → `Internal(String)`), the test verification approach differs from behavior changes. Type-level refactors are primarily **compiler-verified**:

**Phase 6c Learning (SecretBox)**: Reviewed SecretBox migration for `master_secret` in MC actors.
**Phase 4 Learning (Error Variants)**: Reviewed GcError::Internal unit variant → tuple variant migration.
**AC Code Quality Learning (Error Hiding)**: Reviewed error hiding fixes (`|_|` → `|e|` with `error = %e` logging).

All type-level refactors show the same pattern:
- All type mismatches caught by compiler (no silent failures)
- Test updates are mechanical: wrap at construction, pattern match at usage
- Existing test cases remain valid without modification
- No new test cases required
- All tests execute successfully after type updates (GC: 259 → 259, MC: 115 → 115, AC: 447 → 447)

**Verification checklist for type-level refactors**:
1. **Compiler passes**: `cargo check --workspace` - all type mismatches resolved
2. **Test count preserved**: Same number of tests execute before/after
3. **Pattern matching used**: Tests use `matches!(err, Error::Variant(_))` instead of exact equality
4. **Semantic equivalence**: Wrapper is transparent - behavior identical, just with added properties

Examples of type-level refactors:
- **SecretBox**: `.expose_secret()` is transparent (just derefs to &T), memory zeroing automatic on drop
- **Error variant context**: `Internal` → `Internal(String)` adds debuggability, client sees same generic message
- **Error hiding fixes**: `|_|` → `|e|` with `error = %e` logging adds observability, error types unchanged

Result: Type-level refactors are low-risk from test coverage perspective. Focus review on compiler correctness and no perf regressions. Test updates are mechanical, not behavioral.

---

## Pattern: Error Path Testing for Pure Refactors
**Added**: 2026-01-29
**Related files**: `crates/ac-service/src/crypto/mod.rs`, `crates/ac-service/src/handlers/`, `crates/ac-service/src/config.rs`

When reviewing error hiding fixes (`.map_err(|_| ...)` → `.map_err(|e| ...)` with logging), test coverage assessment differs from new feature implementation. The refactor preserves error types while adding observability:

**What to verify**:
- Existing tests cover the error paths being modified
- Tests verify error type returned, not internal error message text
- All tests pass without modification (confirms behavioral preservation)

**What NOT to require**:
- New tests for the error context logging itself (internal observability change)
- Tests that assert on log output (use `tracing_test` if truly needed, but rarely is)
- Modification to existing error assertions (error types unchanged)

**AC code quality review pattern**:
- 28 error paths modified across 4 files
- All error paths already tested (crypto: 90+ tests, handlers: 20+ tests, config: 35+ tests)
- Tests verify correct error type returned (e.g., `AcError::Crypto`, `AcError::InvalidCredentials`)
- 447 tests pass unchanged (370 unit + 77 integration)
- No new test cases needed - existing coverage validates error paths work correctly

Result: Error hiding fixes are observability improvements, not behavioral changes. Existing error path tests remain sufficient. Optionally note as tech debt: "Consider adding log assertion tests using `tracing_test` to verify error context is captured" - but this is enhancement, not blocker.

---

## Pattern: User POV Testing for Cross-Service env-tests
**Added**: 2026-01-31
**Related files**: `crates/env-tests/tests/22_mc_gc_integration.rs`, `crates/env-tests/tests/21_cross_service_flows.rs`

When testing cross-service integration in env-tests, always test from the **user's perspective** via HTTP APIs, NOT from internal service perspective via gRPC. Wrong approach: "Test GC's internal gRPC API by mocking what MC does" (belongs in `crates/global-controller/tests/`). Correct approach: "Test what the user sees when they call `GET /v1/meetings/{code}`".

Key distinction:
- **env-tests**: User-facing HTTP APIs, end-to-end flows visible to clients
- **crate integration tests**: Internal gRPC APIs, service-to-service contracts

For MC-GC integration, this means testing that `JoinMeetingResponse` includes `mc_assignment` (user sees MC endpoint), NOT testing that GC's `RegisterMC` gRPC handler stores MC correctly (internal). This scope separation prevents duplicate test coverage and ensures env-tests validate the actual user experience.

---

## Pattern: MockBehavior Enum for gRPC Test Server State Machines
**Added**: 2026-01-31
**Related files**: `crates/meeting-controller/tests/gc_integration.rs`

When testing gRPC client interactions with recovery flows, use a `MockBehavior` enum to model different server response patterns:

```rust
#[derive(Debug, Clone, Copy)]
enum MockBehavior {
    Accept,                 // Normal operation
    Reject,                 // Registration rejected
    NotFound,               // Always return NOT_FOUND (simulates lost state)
    NotFoundThenAccept,     // First request NOT_FOUND, subsequent requests accept
}

struct MockGcServer {
    behavior: MockBehavior,
    // ... other fields
}

#[tonic::async_trait]
impl GlobalControllerService for MockGcServer {
    async fn fast_heartbeat(&self, req: Request<FastHeartbeatRequest>)
        -> Result<Response<HeartbeatResponse>, Status>
    {
        match self.behavior {
            MockBehavior::NotFound => {
                Err(Status::not_found("MC not registered with GC"))
            }
            MockBehavior::NotFoundThenAccept => {
                let count = self.heartbeat_count.fetch_add(1, Ordering::SeqCst);
                if count == 0 {
                    Err(Status::not_found("MC not registered"))
                } else {
                    Ok(Response::new(HeartbeatResponse { acknowledged: true, ... }))
                }
            }
            _ => Ok(Response::new(HeartbeatResponse { ... }))
        }
    }
}
```

**Key benefits**:
- Single mock server handles multiple test scenarios (no duplicate server code)
- Clearly models state transitions (NotFoundThenAccept simulates re-registration recovery)
- Extensible: add new behaviors without changing existing tests
- Stateful variants (NotFoundThenAccept) use atomic counters to track request count

**Test scenarios enabled**:
- `MockBehavior::Accept`: Happy path (registration, heartbeats succeed)
- `MockBehavior::Reject`: Error handling (registration rejected by GC)
- `MockBehavior::NotFound`: NOT_FOUND detection (heartbeat returns `McError::NotRegistered`)
- `MockBehavior::NotFoundThenAccept`: Full recovery flow (heartbeat fails → re-register → heartbeat succeeds)

**Phase 6c MC review**: This pattern enabled testing the full re-registration flow without a real GC deployment. 4 integration tests cover NOT_FOUND detection and recovery using MockBehavior variants.

Alternative patterns considered and rejected:
- **Separate mock per scenario**: Duplicates code, harder to maintain
- **Runtime configuration flags**: Less type-safe, behavior changes implicit
- **Trait-based mocking**: Overkill for simple state machine, adds complexity

Use MockBehavior when testing client retry/recovery logic against unreliable services.

---

## Pattern: OnceLock for Test Channel Senders (Avoiding mem::forget)
**Added**: 2026-02-02
**Related files**: `crates/meeting-controller/src/grpc/gc_client.rs`, `crates/meeting-controller/tests/gc_integration.rs`

When tests need a `watch::Receiver` that outlives the test function (e.g., for `TokenReceiver`), use `OnceLock` to keep the sender alive without memory leaks:

```rust
fn mock_token_receiver() -> TokenReceiver {
    use std::sync::OnceLock;
    use tokio::sync::watch;

    // Static sender keeps the channel alive without memory leak
    static TOKEN_SENDER: OnceLock<watch::Sender<SecretString>> = OnceLock::new();

    let sender = TOKEN_SENDER.get_or_init(|| {
        let (tx, _rx) = watch::channel(SecretString::from("test-token"));
        tx
    });

    TokenReceiver::from_test_channel(sender.subscribe())
}
```

**Why not `mem::forget`**: The old pattern `mem::forget(tx)` intentionally leaks the sender, which works but:
- Creates actual memory leaks (tools like Valgrind/MIRI will flag it)
- Violates Rust idioms (forget is almost always wrong)
- Confuses readers about intent

**Why OnceLock works**:
- Static lifetime keeps sender alive for entire test process
- `get_or_init()` is thread-safe (parallel tests share same sender)
- Sender is initialized once, reused across test functions
- No leak - static is dropped at process exit (normal cleanup)

**When to use**: Any test helper that creates a channel where sender must outlive the returned receiver.

---

## Pattern: Testing Infinite Retry Loops with Timeout Wrappers
**Added**: 2026-02-02
**Related files**: `crates/common/src/token_manager.rs`

When testing components with intentional infinite retry (background token refresh, reconnection loops), use `tokio::time::timeout` to wrap the operation and verify retry behavior:

```rust
#[tokio::test]
async fn test_401_triggers_infinite_retry() {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&mock_server)
        .await;

    let config = test_config(&mock_server.uri());

    // Operation retries forever on 401 - use timeout to verify this behavior
    let result = tokio::time::timeout(
        Duration::from_secs(2),
        spawn_token_manager(config)
    ).await;

    // Timeout proves infinite retry is working as designed
    assert!(result.is_err(), "Should timeout on 401 (infinite retry)");
}
```

Key insight: The test verifies **design intent** (infinite retry) rather than testing a single error occurrence. This is the correct pattern for:
- Token refresh loops that should never give up
- Service reconnection that persists through transient failures
- Background tasks that must recover automatically

Alternative approaches and when to use them:
- **wiremock `up_to_n_times(N)`**: When you want to test eventual success after N failures
- **AtomicU32 call counter**: When you need to verify specific retry counts
- **`#[tokio::test(start_paused = true)]`**: When testing backoff timing deterministically

---

## Pattern: Observability Wiring Tests - Implicit vs Explicit Verification
**Added**: 2026-02-05
**Related files**: `crates/meeting-controller/src/actors/metrics.rs`, `crates/meeting-controller/src/observability/metrics.rs`

When adding Prometheus wiring to existing code (ActorMetrics, MailboxMonitor, error counters), the test verification approach differs from new feature tests. The wiring itself is simple (direct function calls, no branching), so comprehensive testing of the wrapper functions is more valuable than testing individual call sites:

**Pattern**:
1. **Existing tests verify behavior changes** (internal counters update correctly)
2. **Wrapper module tests verify Prometheus emission** (metrics are recorded)
3. **Wiring is "exercised" indirectly** when behavior tests pass (functions called = Prometheus emitted)

Example from MC metrics wiring:
- `test_actor_metrics()` exercises `meeting_created()`, `connection_created()`, etc. - the Prometheus calls inside execute
- `observability::metrics::tests::test_set_meetings_active()` verifies the Prometheus wrapper works correctly
- Combined: Metrics wiring is validated without explicit "mock Prometheus and assert it was called" tests

**Why this approach works**:
- Wrapper functions are simple (gauge/counter emission, no logic)
- Wiring is simple (direct calls, no conditions)
- Risk of regression is low (compiler catches if function is removed)
- Tests remain independent (test suite doesn't depend on metrics infrastructure)

**Gotcha**: This approach works for simple wiring. For complex observability logic (conditional emission, aggregation, batching), add explicit tests that verify the Prometheus calls. Indicator: If the wiring code has if-else branches or complex logic, add dedicated tests for those paths.

**Cardinality risk**: When adding labeled metrics, verify label values are bounded (test explicitly that only expected label values are used, not unbounded user input). The observability module has a `test_cardinality_bounds` test that validates this - leverage existing tests rather than adding new ones.
