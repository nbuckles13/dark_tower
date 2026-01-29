# Code Reviewer - Gotchas

Common code smells and anti-patterns to watch for in Dark Tower codebase.

---

## Gotcha: Single-Layer Security Validation
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/crypto/mod.rs`

If security parameters are only validated at config parse time, bugs or programmatic construction can bypass checks. Always validate at point of use too. Example: bcrypt cost should be checked both when loading config AND when hashing passwords.

---

## Gotcha: Magic Numbers Without Constants
**Added**: 2026-01-11
**Updated**: 2026-01-27 (expanded scope)
**Related files**: `crates/ac-service/src/config.rs`, `crates/meeting-controller/src/grpc/mc_service.rs`

Numeric values that represent domain-specific meanings (security parameters, estimation factors, capacity limits) should be defined as named constants with doc comments explaining their derivation. This applies beyond security-critical values:

```rust
// BAD: Inline magic number
let estimated_participants = 10;

// GOOD: Named constant with rationale
/// Estimated participants per meeting for capacity planning.
/// Based on typical meeting sizes observed in production (P50 = 4, P90 = 8).
/// Using 10 provides ~20% headroom above P90.
const ESTIMATED_PARTICIPANTS_PER_MEETING: u32 = 10;
```

The doc comment should explain the "why" - how was this value chosen? What happens if it's wrong?

---

## Gotcha: Missing Range Tests for Config Values
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/config.rs`

Config validation tests should cover: below minimum (rejected), at minimum (accepted), default value (accepted), at maximum (accepted), above maximum (rejected). Missing boundary tests allow edge case bugs to slip through.

---

## Gotcha: Deriving Debug on Structs with SecretBox Fields
**Added**: 2026-01-12
**Related files**: `crates/ac-service/src/config.rs`, `crates/ac-service/src/crypto/mod.rs`

Do NOT use `#[derive(Debug)]` on structs containing `SecretBox<T>` or `SecretString`. While `SecretBox` itself redacts in Debug output, the struct's derived Debug may expose other sensitive context (like database URLs with credentials). Always implement Debug manually to control exactly what's shown. Look for structs with secret fields that derive Debug - they need manual impl.

---

## Gotcha: Missing Documentation on Custom Serialize for Secrets
**Added**: 2026-01-12
**Related files**: `crates/ac-service/src/handlers/admin_handler.rs`

When implementing custom `Serialize` that exposes a `SecretString` via `.expose_secret()`, always add a doc comment explaining this is intentional. Without documentation, future reviewers may flag it as a security bug. Pattern: `/// Custom Serialize that exposes client_secret for API response. This is intentional: [reason].`

---

## Gotcha: Forgetting Clone Impl When Using SecretBox
**Added**: 2026-01-12
**Related files**: `crates/ac-service/src/config.rs`

`SecretBox<T>` does not derive `Clone` by design. If your struct needs Clone and contains SecretBox fields, you must implement Clone manually. Compiler error will catch this, but watch for workarounds like removing Clone requirement entirely - sometimes Clone is actually needed (e.g., Config shared across threads via Arc).

---

## Gotcha: Inconsistent Redaction Placeholder Strings
**Added**: 2026-01-12
**Related files**: `crates/ac-service/src/`

Use consistent `"[REDACTED]"` string across all Debug implementations. Inconsistent placeholders (e.g., `"***"`, `"<hidden>"`, `"[SECRET]"`) make log analysis harder and suggest incomplete refactoring. Grep for redaction patterns to verify consistency.

---

## Gotcha: Health Check HTTP 200 vs Error Status
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/handlers/health.rs`

Common mistake: returning an error status (500) when a health check probe fails. This causes the HTTP request to fail and the probe to timeout, which is worse than returning 200 with `"unhealthy"` status. K8s expects to parse the response body, so:
- BAD: `.map_err(|_| GcError::DatabaseUnavailable)` - probe fails
- GOOD: `let db_healthy = sqlx::query().await.is_ok()` then return 200 with status field

The probe should always succeed HTTP-wise; the body tells K8s the actual health state.

---

## Gotcha: Confusing Service Layer vs Repository Layer Errors
**Added**: 2026-01-15
**Related files**: `crates/ac-service/src/services/`, `crates/ac-service/src/repositories/`

Repository layer errors are internal implementation details. Service layer should wrap these in domain-specific errors that handlers understand. Don't leak repository errors through service layer - always map. Pattern: repository might return `DatabaseError::UniqueViolation`, service wraps in `UserServiceError::EmailAlreadyExists`.

---

## Gotcha: Token Parsing in Middleware vs Handler
**Added**: 2026-01-15
**Related files**: `crates/ac-service/src/middleware/org_extraction.rs`

Don't implement token parsing logic in both middleware and handlers. Parse once in middleware, attach structured claims to extensions, handlers use pre-parsed data. Duplicate parsing is error-prone and wastes cycles.

---

## Gotcha: Weak OR Assertion Logic in Rate Limiting Tests
**Added**: 2026-01-15
**Related files**: `crates/ac-service/tests/integration/user_auth_tests.rs`

Rate limiting tests that assert `hit_rate_limit || success_count <= N` can pass even if rate limiting is broken. The OR condition allows either branch to satisfy the assertion. Better: assert that rate limit was actually hit (`assert!(hit_rate_limit, ...)`), or loop until confirmed. Weak assertions mask bugs.

---

## Gotcha: Duplicated JWT Decoding Logic in Tests
**Added**: 2026-01-15
**Related files**: `crates/ac-service/tests/integration/user_auth_tests.rs`

When tests verify JWT claims, the base64 decode + JSON parse pattern gets duplicated. Extract to a helper function in the test harness (e.g., `decode_jwt_payload(token: &str) -> Result<serde_json::Value, anyhow::Error>`). Reduces duplication and makes JWT format changes easier to maintain.

---

## Gotcha: Improvements in New Code That Should Be Backported
**Added**: 2026-01-18
**Related files**: `crates/env-tests/src/fixtures/gc_client.rs`, `crates/env-tests/src/fixtures/auth_client.rs`

When reviewing new code that follows an existing pattern but adds improvements, flag the improvement for backporting. Example: `GcClient` added `sanitize_error_body()` which `AuthClient` lacks. Don't block the review, but document as a suggestion or add to TODO.md. Pattern: "GcClient adds [feature] which AuthClient does not have - consider backporting."

---

## Gotcha: #[expect(dead_code)] vs #[allow(dead_code)]
**Added**: 2026-01-22
**Related files**: `crates/global-controller/src/`

Use `#[allow(dead_code)]` with a comment explaining future use, not `#[expect(dead_code)]`. The `#[expect]` attribute causes "unfulfilled lint expectation" warnings when the code is actually used (e.g., by tests or future features). This creates noise in CI and can mask real warnings.

```rust
// BAD: Causes warning when tests use this code
#[expect(dead_code)]
fn future_feature() { ... }

// GOOD: Silent suppression with documentation
#[allow(dead_code)] // Used by MC registration (Phase 6)
fn future_feature() { ... }
```

This is particularly common when scaffolding code for future phases or writing repository methods ahead of their service-layer callers.

---

## Gotcha: Duplicate Logging Between Repository and Service Layers
**Added**: 2026-01-22
**Related files**: `crates/global-controller/src/repositories/`, `crates/global-controller/src/services/`

Logging the same operation at both repository and service layers creates duplicate log entries that clutter observability and make debugging harder. Choose ONE layer for logging:

- **Repository layer**: Log database-specific details (query timing, row counts)
- **Service layer**: Log business operations (user actions, workflow steps)

Typically prefer service layer logging because it captures business context. Repository layer should only log if there's database-specific diagnostic value not available at service layer.

```rust
// BAD: Both layers log the same operation
// In repository:
tracing::info!("Assigning meeting {} to MC {}", meeting_id, mc_id);
// In service:
tracing::info!("Assigned meeting {} to MC {}", meeting_id, mc_id);

// GOOD: Service layer only (has business context)
// In repository: (no logging)
// In service:
tracing::info!(meeting_id = %meeting_id, mc_id = %mc_id, "Meeting assigned to MC");
```

---

## Gotcha: Silent Config Fallback to Defaults
**Added**: 2026-01-25
**Updated**: 2026-01-28 (added GC example)
**Related files**: `crates/meeting-controller/src/config.rs`, `crates/global-controller/src/config.rs`

Config parsing that silently falls back to default values when environment variables are invalid can mask configuration errors in production. The service may appear to work but with unintended settings:

```rust
// BAD: Silent fallback hides misconfiguration
let timeout = env::var("MC_TIMEOUT")
    .ok()
    .and_then(|v| v.parse().ok())
    .unwrap_or(DEFAULT_TIMEOUT);

// GOOD: Return error for incorrect values
let timeout = match env::var("MC_TIMEOUT") {
    Ok(v) => v.parse().map_err(|e|
        ConfigError::InvalidTimeout(format!("Must be valid integer, got '{}': {}", v, e))
    )?,
    Err(_) => DEFAULT_TIMEOUT,
};
```

For security-critical settings (JWT clock skew, rate limits, bcrypt cost), always fail instead of falling back. For operational settings (timeouts, capacity), log a warning at minimum. GC's JWT clock skew parsing is the reference implementation - it returns ConfigError on invalid input.

---

## Gotcha: std::sync::Mutex in Async Test Mocks
**Added**: 2026-01-25
**Related files**: `crates/mc-test-utils/src/mock_redis.rs`

Using `std::sync::Mutex` in mock implementations for async code can cause deadlocks or poor performance:

```rust
// PROBLEMATIC: Blocks async runtime threads
pub struct MockRedis {
    data: std::sync::Mutex<HashMap<String, String>>,
}

// BETTER: Use async-aware mutex
pub struct MockRedis {
    data: tokio::sync::Mutex<HashMap<String, String>>,
}
```

While `std::sync::Mutex` may work in simple test scenarios, it can cause subtle issues when tests run concurrently or when mock methods are called from multiple async contexts. Flag as tech debt if found in test-utils crates.

---

## Gotcha: Wrong Error Variant for Communication Type
**Added**: 2026-01-27
**Related files**: `crates/meeting-controller/src/grpc/gc_client.rs`

When a service communicates with external systems via different protocols (Redis, gRPC, HTTP), use semantically correct error variants. Using the wrong variant (e.g., `McError::Redis` for a gRPC call) confuses debugging and breaks error handling logic that branches on variant:

```rust
// BAD: Using Redis error for gRPC call
async fn get_assignment(&self, meeting_id: &str) -> Result<MeetingAssignment, McError> {
    self.gc_client.get_assignment(meeting_id).await
        .map_err(|e| McError::Redis(e.to_string()))  // Wrong! This is gRPC
}

// GOOD: Correct error variant for the protocol
async fn get_assignment(&self, meeting_id: &str) -> Result<MeetingAssignment, McError> {
    self.gc_client.get_assignment(meeting_id).await
        .map_err(|e| McError::Grpc(e.to_string()))  // Correct variant
}
```

This matters for observability (error dashboards) and error handling (different retry strategies per protocol).

---

## Gotcha: Synchronous get_* Methods in Actor Handles
**Added**: 2026-01-25
**Related files**: `crates/meeting-controller/src/actors/controller.rs`

When an actor handle provides a method to retrieve state that depends on child actors, the method MUST be async. A synchronous getter that returns cached/stale values leads to incorrect status reporting:

```rust
// BAD: Returns stale cached value
pub fn get_meeting(&self, meeting_id: &str) -> MeetingInfo {
    // participant_count is always 0 because we're not querying the actor
    MeetingInfo { participant_count: self.cached_count, ... }
}

// GOOD: Async query to child actor for live state
pub async fn get_meeting(&self, meeting_id: &str) -> Result<MeetingInfo, McError> {
    let state = self.meeting_handle.get_state().await?;
    Ok(MeetingInfo { participant_count: state.participants.len(), ... })
}
```

This was MINOR-001 in the Phase 6b review - participant_count was always 0 because the method didn't query the MeetingActor.

---

## Gotcha: Missing Graceful Fallback When Actor Communication Fails
**Added**: 2026-01-25
**Related files**: `crates/meeting-controller/src/actors/controller.rs`

When querying child actors that may have shut down, always handle the error case gracefully. Returning an error when the actor is unavailable can break status endpoints or cause cascading failures:

```rust
// BAD: Propagates error when child actor is shutting down
let state = managed.handle.get_state().await?;

// GOOD: Graceful fallback with logging
match managed.handle.get_state().await {
    Ok(state) => Ok(MeetingInfo { participant_count: state.participants.len(), ... }),
    Err(_) => {
        warn!("Failed to query meeting actor state, returning cached info");
        Ok(MeetingInfo { participant_count: 0, ... })  // Safe default
    }
}
```

The graceful fallback ensures status queries don't fail during graceful shutdown or actor restarts.

---

## Gotcha: SecretBox Clone Performance vs Type Safety Trade-off
**Added**: 2026-01-28
**Related files**: `crates/meeting-controller/src/actors/controller.rs`

When reviewing code that calls `.expose_secret().clone()`, don't immediately flag as performance waste. `SecretBox` intentionally prevents cloning to protect against secret leaks. Occasional clones at key points (per-entity initialization) are acceptable if:

1. The clone happens in initialization code, not hot paths
2. A comment explains the pattern and references ADR-0023
3. Security specialist has approved the tradeoff
4. DRY reviewer confirms no duplicate patterns elsewhere

Red flag if you see many clones across multiple callsites - escalate to tech debt for `Arc<SecretBox<T>>` consideration.

```rust
// ACCEPTABLE: Single clone during initialization
let meeting_secret = SecretBox::new(Box::new(
    self.master_secret.expose_secret().clone()  // Minimal, justified
));

// NOT ACCEPTABLE: Multiple clones across hot path
for meeting in meetings {
    let secret = self.master_secret.expose_secret().clone();  // Flag this
    // ... repeated many times per request
}
```
