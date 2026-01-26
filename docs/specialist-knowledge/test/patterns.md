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
**Added**: 2026-01-12
**Related files**: Integration test files

When refactoring raw types to wrapper types (e.g., `Vec<u8>` to `SecretBox<Vec<u8>>`):
1. Search all usages of the struct being modified
2. Update construction sites to wrap values: `SecretBox::new(Box::new(value))`
3. Update access sites to unwrap: `.expose_secret()`
4. **Verify test files are included in mod.rs** - orphaned tests won't catch type errors
5. Run `cargo test` and verify expected test count executes

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
**Added**: 2026-01-24
**Related files**: `crates/global-controller/tests/meeting_assignment_tests.rs`

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

The mixed scenario catches bugs where retry counter is incorrectly updated or candidate list isn't properly iterated.

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
