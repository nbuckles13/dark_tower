# Code Reviewer - Patterns

Reusable code quality patterns observed in Dark Tower codebase.

---

## Pattern: Configuration Value Pattern (Constants + Field + Parsing + Tests)
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/config.rs`

When adding configurable security parameters (e.g., bcrypt cost, JWT clock skew), follow the four-part pattern: (1) define constants with DEFAULT/MIN/MAX bounds, (2) add config struct field with serde defaults, (3) implement parsing with range validation, (4) add comprehensive tests. This ensures consistency and makes security boundaries explicit.

---

## Pattern: Defense-in-Depth Validation
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/config.rs`, `crates/ac-service/src/crypto/mod.rs`

Validate security-critical values at multiple layers: config parsing time AND at point of use. Even if config validation ensures valid ranges, crypto functions should independently verify inputs. Prevents bugs if validation is bypassed or config is constructed programmatically.

---

## Pattern: OWASP/NIST Reference Documentation
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/config.rs`

Document security-critical constants with references to authoritative sources (OWASP, NIST). Example: bcrypt cost factor 12 references OWASP password storage cheat sheet. This provides audit trail and justification for security decisions.

---

## Pattern: SecretBox Custom Debug Implementation
**Added**: 2026-01-12
**Related files**: `crates/ac-service/src/config.rs`, `crates/ac-service/src/crypto/mod.rs`

When a struct contains `SecretBox<T>` fields, implement custom `Debug` using `f.debug_struct()` with `&"[REDACTED]"` for sensitive fields. This prevents accidental logging:
```rust
impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("database_url", &"[REDACTED]")
            .field("bind_address", &self.bind_address)
            .field("master_key", &"[REDACTED]")
            .finish()
    }
}
```
Document which fields are redacted and why in the doc comment above the impl.

---

## Pattern: SecretBox Custom Clone Implementation
**Added**: 2026-01-12
**Related files**: `crates/ac-service/src/config.rs`, `crates/ac-service/src/crypto/mod.rs`

`SecretBox<T>` intentionally does not implement `Clone` to prevent accidental secret duplication. When cloning is required, implement manually:
```rust
impl Clone for Config {
    fn clone(&self) -> Self {
        Self {
            master_key: SecretBox::new(Box::new(self.master_key.expose_secret().clone())),
            // ... other fields
        }
    }
}
```
Document why Clone is needed in the struct doc comment.

---

## Pattern: SecretString Serialize for One-Time Exposure
**Added**: 2026-01-12
**Related files**: `crates/ac-service/src/models/mod.rs`, `crates/ac-service/src/handlers/admin_handler.rs`

For API responses that must expose a secret exactly once (e.g., client_secret at registration), implement custom `Serialize` that calls `.expose_secret()`:
```rust
impl Serialize for CreateClientResponse {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where S: Serializer {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("CreateClientResponse", 3)?;
        state.serialize_field("client_id", &self.client_id)?;
        state.serialize_field("client_secret", self.client_secret.expose_secret())?;
        state.end()
    }
}
```
CRITICAL: Add doc comment stating "This is intentional: the [response type] is the ONLY time the plaintext [secret] is shown to the user."

---

## Pattern: Debugging-Friendly Assertion Messages
**Added**: 2026-01-13
**Related files**: `crates/env-tests/tests/25_auth_security.rs`, `crates/env-tests/tests/40_resilience.rs`

Assertion messages should guide debugging, not just state what failed. Include: (1) what was expected, (2) what was received, (3) likely causes, (4) remediation steps. Example:
```rust
assert!(
    can_reach,
    "Canary pod in dark-tower namespace should be able to reach AC service at {}. \
     If this fails, check: 1) AC service is running, 2) Service DNS resolves, \
     3) No NetworkPolicy blocking same-namespace traffic.",
    target_url
);
```

---

## Pattern: CVE/CWE Reference Documentation in Security Tests
**Added**: 2026-01-13
**Related files**: `crates/env-tests/tests/25_auth_security.rs`

Document security tests with specific CVE/CWE references in doc comments. This provides audit trail and helps future reviewers understand the threat being mitigated. Example:
```rust
/// Test that tokens with embedded 'jwk' header are rejected (CVE-2018-0114).
/// An attacker should not be able to embed their own key in the token header.
#[tokio::test]
async fn test_jwk_header_injection_rejected() { ... }
```

---

## Pattern: Test Isolation with #[serial] for Shared Resources
**Added**: 2026-01-13
**Related files**: `crates/env-tests/tests/40_resilience.rs`

Use `#[serial]` from `serial_test` crate when tests share mutable state (Kubernetes cluster, database, network resources). Prevents race conditions and flaky tests:
```rust
use serial_test::serial;

#[tokio::test]
#[serial]
async fn test_same_namespace_connectivity() { ... }
```

---

## Pattern: Feature-Gated Test Organization
**Added**: 2026-01-13
**Related files**: `crates/env-tests/tests/*.rs`

Organize tests by feature flag based on infrastructure requirements:
- `#![cfg(feature = "smoke")]` - Basic tests requiring minimal infrastructure
- `#![cfg(feature = "flows")]` - Full integration tests requiring deployed services
- `#![cfg(feature = "resilience")]` - Chaos/resilience tests requiring cluster access

Each test file declares exactly one feature gate at the module level. This prevents accidental execution in `cargo test --workspace` while enabling selective test runs in CI.

---

## Pattern: Test Harness with Real Server Instance (Arc + JoinHandle)
**Added**: 2026-01-14
**Related files**: `crates/gc-test-utils/src/server_harness.rs`, `crates/ac-test-utils/src/server_harness.rs`

For integration testing, create a test harness that spawns a real service instance:
1. Use `Arc<AppState>` to wrap shared state (DB pool, config)
2. Spawn the HTTP server in background via `tokio::spawn()` and hold the `JoinHandle`
3. Return a wrapper struct (e.g., `TestGcServer`) that provides accessor methods (`url()`, `pool()`, `config()`)
4. Implement `Drop` to explicitly `abort()` the background task for immediate cleanup
5. Support test-specific config via `from_vars()` with sensible defaults

Benefits: Tests the full integration with real networking, real database pool, real middleware.

---

## Pattern: Health Check That Reports Status Without Erroring
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/handlers/health.rs`

Health checks for Kubernetes readiness/liveness probes should:
1. Always return HTTP 200 (no error status)
2. Include a `status` field ("healthy" or "unhealthy") in response body
3. Include probe-specific sub-statuses (e.g., `database` field)
4. Ping critical dependencies (database, cache) but don't fail the request if they're down
5. Use `is_ok()` for probe calls, not `?` operator
6. Let K8s interpret the response body to make routing decisions

This allows K8s to see unhealthy services and stop routing traffic, but doesn't cause the probe to timeout.

---

## Pattern: Repository Organization (repositories/ Directory)
**Added**: 2026-01-15
**Related files**: `crates/ac-service/src/repositories/`

When a service manages multiple domain entities (users, organizations, clients, etc.), organize repositories as separate files in `src/repositories/` directory, each scoped to one entity. Re-export all via `src/repositories/mod.rs` for convenience. Each file should contain only that entity's queries and error types. This improves discoverability and reduces file complexity.

---

## Pattern: Middleware for Organization Context Extraction
**Added**: 2026-01-15
**Related files**: `crates/ac-service/src/middleware/org_extraction.rs`

Use middleware to extract organization context from requests (subdomain, token claims, etc.) and attach to typed extensions. Handlers receive context via Axum's `Extension` extractor without manual extraction. Middleware should return error responses for invalid context, not panics. This centralizes org extraction logic across all handlers.

---

## Pattern: Integration Test Organization with Section Comments
**Added**: 2026-01-15
**Related files**: `crates/ac-service/tests/integration/user_auth_tests.rs`

Organize integration tests with clear section separators and category headers:
```rust
// ============================================================================
// Registration Tests (11 tests)
// ============================================================================

/// Test that valid registration returns user_id and access_token.
#[sqlx::test(migrations = "../../migrations")]
async fn test_register_happy_path(pool: PgPool) -> Result<(), anyhow::Error> { ... }
```
Benefits: Easy navigation in long test files, clear test count per category. Works well for files with 20+ tests.

---

## Pattern: Service Client Fixture with Error Body Sanitization
**Added**: 2026-01-18
**Related files**: `crates/env-tests/src/fixtures/gc_client.rs`

Test API clients should follow this complete pattern (GcClient is the reference):
1. Error enum with `HttpError`, `RequestFailed { status, body }`, `JsonError` variants
2. `sanitize_error_body()` helper using regex to remove JWT/Bearer patterns
3. Custom Debug on response types with sensitive fields
4. `health_check()` method for availability detection
5. `raw_*` methods returning `Response` for testing error paths

Key insight: Sanitize error bodies at capture time (in `handle_response()`), not in Debug output. This prevents credential leaks through all code paths including assertions and error chains.

---

## Pattern: Builder Pattern with #[must_use] for Complex Structs
**Added**: 2026-01-25
**Related files**: `crates/meeting-controller/src/config.rs`

For configuration structs with many optional fields, use the builder pattern with `#[must_use]` on the builder methods. This provides:
1. Compile-time enforcement that builder results are used
2. Fluent, readable configuration in tests
3. Clear defaults without Option-wrapping every field

```rust
#[derive(Default)]
pub struct McConfigBuilder { ... }

impl McConfigBuilder {
    #[must_use]
    pub fn bind_address(mut self, addr: SocketAddr) -> Self {
        self.bind_address = Some(addr);
        self
    }

    pub fn build(self) -> McConfig { ... }
}
```

Benefits: Unused builder chains trigger compiler warnings, preventing accidental configuration omissions.

---

## Pattern: ADR References in Doc Comments
**Added**: 2026-01-25
**Related files**: `crates/meeting-controller/src/`

Document code that implements ADR requirements with explicit references in doc comments:
```rust
/// Session state for active participants.
///
/// See ADR-0023 Section 4.2 for state machine requirements.
pub struct SessionState { ... }
```

This creates bidirectional traceability: ADRs reference code locations, code references ADR sections. Makes compliance audits easier and helps future reviewers understand design rationale.

---

## Pattern: Actor Handle/Task Separation
**Added**: 2026-01-25
**Related files**: `crates/meeting-controller/src/actors/meeting.rs`, `crates/meeting-controller/src/actors/controller.rs`

When implementing the actor pattern (ADR-0001), separate the public API (`Handle`) from the private implementation (`Actor`). The handle contains only `mpsc::Sender` and `CancellationToken`, providing async methods that send messages and await responses via oneshot channels. The actor struct owns all state and runs the message loop. This creates a clean API surface and ensures all state mutations happen within the actor task.

```rust
pub struct MeetingActorHandle {
    sender: mpsc::Sender<MeetingMessage>,
    cancel_token: CancellationToken,
}

impl MeetingActorHandle {
    pub async fn get_state(&self) -> Result<MeetingState, McError> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.sender.send(MeetingMessage::GetState { respond_to: tx }).await?;
        rx.await.map_err(|_| McError::Internal)
    }
}
```

---

## Pattern: Async State Queries for Accurate Status
**Added**: 2026-01-25
**Related files**: `crates/meeting-controller/src/actors/controller.rs`

When an actor needs to report the state of its children (e.g., participant counts), query the child actor asynchronously rather than caching stale values. This ensures status reflects actual state. Provide graceful fallback when the child actor is unavailable:

```rust
async fn get_meeting(&self, meeting_id: &str) -> Result<MeetingInfo, McError> {
    match managed.handle.get_state().await {
        Ok(state) => Ok(MeetingInfo {
            participant_count: state.participants.len(),  // Actual count
            ...
        }),
        Err(_) => {
            warn!("Failed to query meeting actor, returning cached info");
            Ok(MeetingInfo { participant_count: 0, ... })  // Graceful fallback
        }
    }
}
```

---

## Pattern: #[allow(clippy::expect_used)] with ADR-0002 Justification
**Added**: 2026-01-25
**Related files**: `crates/meeting-controller/src/actors/session.rs`

When `expect()` is unavoidable (e.g., CSPRNG operations, HKDF with fixed parameters), use `#[allow(clippy::expect_used)]` with an inline comment explaining why this is an unreachable invariant per ADR-0002. The comment should explain the technical reason the operation cannot fail:

```rust
#[allow(clippy::expect_used)] // ADR-0002: CSPRNG fill is an unreachable invariant
pub fn generate_token(&self, ...) -> (String, String) {
    // ADR-0002: CSPRNG fill on 16 bytes is an unreachable failure condition
    // SystemRandom uses OS-level entropy sources (getrandom/urandom) which
    // only fail if the OS itself is catastrophically broken
    rand::SecureRandom::fill(&rng, &mut nonce_bytes)
        .expect("CSPRNG should not fail on 16 bytes");
}
```

This documents the security rationale while maintaining ADR-0002 compliance.

---

## Pattern: SecretBox Performance Trade-off for Type Safety
**Added**: 2026-01-28
**Related files**: `crates/meeting-controller/src/actors/session.rs`, `crates/meeting-controller/src/actors/controller.rs`

When using `SecretBox<T>` for cryptographic secrets, be aware that `SecretBox` intentionally doesn't implement `Clone` to prevent accidental secret duplication. This creates a tradeoff: type-safe protection against secret leaks vs. occasional performance cost when secrets must be passed to child actors or stored in arrays.

Pattern for per-entity secret storage (e.g., meeting-specific secrets):
```rust
// In controller actor, for each meeting, create a new SecretBox
pub fn create_meeting(&mut self, meeting_id: String) -> Result<MeetingInfo, McError> {
    let meeting_secret = SecretBox::new(Box::new(
        self.master_secret.expose_secret().clone()  // Minimal, justified clone
    ));
    let handle = MeetingActor::spawn(meeting_id.clone(), meeting_secret, ...);
    // ...
}
```

**Why this is correct**: The per-meeting clone is necessary because `SecretBox` doesn't clone. The clone happens only when creating new meetings (not hot path), and is isolated to the single use site. Document this pattern with comments referencing ADR-0023.

**When this becomes a problem**: If the same secret needs cloning at multiple hot-path callsites, escalate to tech debt for `Arc<SecretBox<T>>` consideration (Phase 6d).

This pattern balances type safety (catching accidental secret leaks at compile time) with pragmatic performance (accepting minimal clones at strategic points).

---

## Pattern: GcError::Internal Variant Evolution
**Added**: 2026-01-28
**Related files**: `crates/global-controller/src/errors.rs`

When evolving error variants from unit variants to tuple variants for better error context, follow this pattern:

```rust
// Before
#[error("Internal server error")]
Internal,

// After
#[error("Internal server error: {0}")]
Internal(String),
```

Then update all usages:
1. **Production code creating errors**: `GcError::Internal(format!("context: {}", e))`
2. **Test pattern matching**: `GcError::Internal(_)` (use wildcard to avoid brittle tests)
3. **status_code() match arm**: `GcError::Internal(_)` (context doesn't affect status)
4. **IntoResponse implementation**: Log context server-side, return generic message to client

This preserves error context for debugging while preventing information leakage to clients. The pattern works for any error variant that needs contextual information.

---

## Pattern: Error Context Preservation with Security-Aware Logging
**Added**: 2026-01-29
**Related files**: `crates/ac-service/src/crypto/mod.rs`, `crates/ac-service/src/handlers/auth_handler.rs`, `crates/ac-service/src/config.rs`

When mapping errors, preserve the original error for debugging while maintaining generic client-facing messages. Use structured logging to capture context without leaking internal details:

```rust
// For internal cryptographic failures (tracing::error!)
.map_err(|e| {
    tracing::error!(target: "crypto", error = %e, "Nonce generation failed");
    AcError::Crypto("Encryption failed".to_string())
})

// For input validation failures (tracing::debug!)
.map_err(|e| {
    tracing::debug!(target: "auth", error = %e, "Invalid base64 in authorization header");
    AcError::InvalidCredentials
})
```

**Key distinctions:**
- Use `tracing::error!` for internal operation failures (crypto, database, network)
- Use `tracing::debug!` for expected input validation failures from external requests
- Always include `error = %e` for structured logging
- Use appropriate tracing targets (`crypto`, `auth`, etc.) for filtering
- Client-facing error message should be generic and non-revealing

This pattern balances debugging needs (detailed server-side logs) with security (generic client messages).

---

## Pattern: Unified Task Ownership (No Arc for Single Consumer)
**Added**: 2026-01-31
**Related files**: `crates/meeting-controller/src/main.rs` (unified GC task)

When a task is the sole owner of a resource, avoid wrapping it in Arc. Let the task own the value directly. This simplifies code, removes unnecessary reference counting overhead, and makes ownership clear:

```rust
// BEFORE (Round 2): Arc wrapper with no actual sharing
let gc_client = Arc::new(GcClient::new(...).await?);
let gc_client_clone = Arc::clone(&gc_client);
tokio::spawn(async move {
    run_heartbeat_task(gc_client_clone, ...).await;
});

// AFTER (Round 3): Direct ownership
let gc_client = GcClient::new(...).await?;
tokio::spawn(async move {
    run_gc_task(gc_client, ...).await;  // Task owns gc_client
});
```

**When to apply:**
- Task is the only user of the resource (no sharing needed)
- Resource is already cheaply cloneable (tonic Channel) for internal use
- Refactoring opportunity: multiple separate tasks → single unified task

**Benefits:**
- Clearer ownership semantics (move vs Arc::clone)
- Reduced cognitive load (no Arc in type signatures)
- Eliminates unnecessary atomic reference counting
- Enables better borrow checker reasoning

**Related refactor:** Iteration 3 unified separate registration/heartbeat tasks into single task that owns gc_client.

---

## Pattern: Never-Exit Resilience for Critical Background Tasks
**Added**: 2026-01-31
**Related files**: `crates/meeting-controller/src/main.rs` (GC task)

Background tasks managing critical infrastructure (GC registration, health monitoring) should never exit on transient failures. Exiting leaves the service in a degraded state with no recovery path. Instead, log and retry indefinitely:

```rust
// Initial registration - retry forever until success or shutdown
loop {
    tokio::select! {
        () = cancel_token.cancelled() => {
            info!("GC task: Cancelled before registration completed");
            return;
        }
        result = gc_client.register() => {
            match result {
                Ok(()) => {
                    info!("GC task: Initial registration successful");
                    break; // Proceed to heartbeat loop
                }
                Err(e) => {
                    // Log but never exit - keep retrying
                    warn!(error = %e, "GC task: Initial registration failed, will retry");
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            }
        }
    }
}

// Heartbeat loop - handle errors, re-register if needed
loop {
    tokio::select! {
        () = cancel_token.cancelled() => { break; }
        _ = ticker.tick() => {
            if let Err(e) = send_heartbeat().await {
                handle_heartbeat_error(&gc_client, e).await;  // Never exits
            }
        }
    }
}
```

**Key properties:**
- Only exit on explicit cancellation signal
- Log failures at warn level (not error - these are transient)
- Re-registration on NOT_FOUND (GC restart recovery)
- Fixed delay between retries (prevent tight loop)
- Protect active meetings/sessions during GC outages

**Why this matters:** If the GC task exits on failure, active meetings lose heartbeat updates and may be fenced out incorrectly. Never-exit design keeps meetings alive during transient GC unavailability.

---

## Pattern: MockBehavior Enum for Test Configuration
**Added**: 2026-01-31
**Related files**: `crates/meeting-controller/tests/gc_integration.rs`

For mock servers that need to simulate different behaviors (success, failure, retry scenarios), use a `MockBehavior` enum to configure behavior centrally. This is cleaner than boolean flags or multiple mock implementations:

```rust
#[derive(Debug, Clone, Copy)]
enum MockBehavior {
    /// Accept all requests normally.
    Accept,
    /// Reject registrations.
    Reject,
    /// Return NOT_FOUND for heartbeats (simulates MC not registered).
    NotFound,
    /// Return NOT_FOUND for first heartbeat, then accept (simulates re-registration).
    NotFoundThenAccept,
}

struct MockGcServer {
    behavior: MockBehavior,
    // ... counters, channels, etc.
}

impl MockGcServer {
    fn new_with_behavior(behavior: MockBehavior) -> Self { ... }
    fn accepting() -> Self { Self::new_with_behavior(MockBehavior::Accept) }
    fn rejecting() -> Self { Self::new_with_behavior(MockBehavior::Reject) }
}

#[tonic::async_trait]
impl GlobalControllerService for MockGcServer {
    async fn fast_heartbeat(&self, ...) -> Result<...> {
        match self.behavior {
            MockBehavior::NotFound => Err(Status::not_found("MC not registered")),
            MockBehavior::NotFoundThenAccept => {
                if self.count.fetch_add(1, Ordering::SeqCst) == 0 {
                    Err(Status::not_found("MC not registered"))
                } else {
                    Ok(Response::new(...))
                }
            }
            MockBehavior::Accept | MockBehavior::Reject => Ok(Response::new(...)),
        }
    }
}
```

**Benefits:**
- Single configuration point (constructor)
- Semantic variant names describe test scenarios
- Enables stateful behaviors (NotFoundThenAccept)
- Backward compatible (accepting()/rejecting() helpers)
- Clear in test code: `MockGcServer::new_with_behavior(MockBehavior::NotFound)`

**When to apply:**
- Mock needs 3+ distinct behaviors
- Tests require stateful behavior (first call fails, subsequent succeed)
- Behavior affects multiple RPC methods consistently

---

## Pattern: Spawn-and-Wait Function API with (JoinHandle, Receiver) Tuple
**Added**: 2026-02-02
**Related files**: `crates/common/src/token_manager.rs`

For background tasks that produce continuously-updated values (token managers, config watchers, health monitors), use a function that spawns the task and waits for the first value before returning. Return `(JoinHandle<()>, Receiver)` tuple:

```rust
pub async fn spawn_token_manager(
    config: TokenManagerConfig,
) -> Result<(JoinHandle<()>, TokenReceiver), TokenError> {
    // Create watch channel with sentinel value
    let (sender, mut receiver) = watch::channel(SecretString::from(""));

    // Spawn background task
    let task_handle = tokio::spawn(async move {
        token_refresh_loop(config, sender).await;
    });

    // Wait for first real value before returning
    receiver.changed().await.map_err(|_| TokenError::ChannelClosed)?;

    // Verify value is valid (defensive)
    if receiver.borrow().expose_secret().is_empty() {
        return Err(TokenError::AcquisitionFailed("Empty token".into()));
    }

    Ok((task_handle, TokenReceiver(receiver)))
}
```

**Key properties:**
- Caller gets `JoinHandle` for lifecycle control (abort, await completion)
- Caller gets typed receiver for value access
- Function only returns after first valid value (no "not ready yet" state)
- Sentinel value distinguishes "never set" from "empty"
- `TokenReceiver` wrapper can enforce safe access patterns (clone on read)

**Benefits over struct with internal task:**
- Clear ownership: caller controls task lifetime via handle
- Testable: can create receiver without spawning task
- Composable: caller decides how to manage the handle

---

## Pattern: OnceLock for Test Watch Channel Senders
**Added**: 2026-02-02
**Related files**: `crates/meeting-controller/src/grpc/gc_client.rs`, `crates/meeting-controller/tests/gc_integration.rs`

When tests need a `watch::Receiver` that stays valid for the test duration, use `OnceLock` to hold the sender statically instead of `mem::forget`. This avoids intentional memory leaks while keeping the channel alive:

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

**Why OnceLock over mem::forget:**
- No intentional memory leak (sender is properly cleaned up at process exit)
- Thread-safe initialization (stable since Rust 1.70.0)
- Self-documenting intent: "this is static, not leaked"
- Works correctly with parallel test execution

**When to apply:**
- Test helpers creating watch::Receiver for mock values
- Any test fixture that needs a channel sender to outlive the test function
- Replacing existing `mem::forget(tx)` patterns in test code

---

## Pattern: Metrics Cardinality Control via Path Normalization
**Added**: 2026-02-04
**Related files**: `crates/global-controller/src/observability/metrics.rs`

When recording HTTP metrics with path labels, normalize dynamic path segments to prevent label cardinality explosion. Replace UUIDs, meeting codes, and other high-cardinality values with placeholders:

```rust
fn normalize_endpoint(path: &str) -> String {
    match path {
        "/" | "/health" | "/metrics" | "/api/v1/me" => path.to_string(),
        _ if path.starts_with("/api/v1/meetings/") => {
            let parts: Vec<&str> = path.split('/').collect();
            match parts.len() {
                5 => "/api/v1/meetings/{code}".to_string(),
                6 if parts[5] == "guest-token" => "/api/v1/meetings/{code}/guest-token".to_string(),
                6 if parts[5] == "settings" => "/api/v1/meetings/{id}/settings".to_string(),
                _ => "/other".to_string(),
            }
        }
        _ => "/other".to_string(),  // Unknown paths normalized to bound cardinality
    }
}
```

**Key properties:**
- Known static paths returned as-is (exact match)
- Dynamic segments replaced with `{placeholder}` format
- Unknown paths fall through to `/other` (bounded cardinality)
- Tests should verify all known routes are normalized correctly

**ADR-0011 compliance:**
- Maximum unique label combinations per metric: 1,000
- Total cardinality budget: 5,000,000 time series
- Use indexed values instead of UUIDs for high-cardinality identifiers

This pattern applies to all services implementing metrics (AC, GC, MC, MH).

---

## Pattern: Module-Level Prometheus Documentation
**Added**: 2026-02-05
**Related files**: `crates/meeting-controller/src/actors/metrics.rs`, `crates/meeting-controller/src/observability/metrics.rs`

When adding Prometheus integration to internal tracking structs, document the module-level behavior clearly:

```rust
//! Internal metrics are wired to Prometheus via the observability module:
//! - `ActorMetrics` updates `mc_meetings_active`, `mc_connections_active`, `mc_actor_panics_total`
//! - `MailboxMonitor` updates `mc_actor_mailbox_depth`, `mc_messages_dropped_total`
//! - `ControllerMetrics` is for GC heartbeat reporting only (no Prometheus emission)
```

This pattern clarifies:
1. Which types emit to Prometheus
2. Which metrics each type produces
3. Which types are NOT wired (prevents assumptions about metric availability)

Particularly useful when a struct has methods that look like they should be metrics (e.g., `increment_participants()`) but aren't actually wired. Prevents future developers from assuming automatic Prometheus emission.

---

## Pattern: Atomic Operation Style Consistency
**Added**: 2026-02-05
**Related files**: `crates/meeting-controller/src/actors/metrics.rs`

When using atomic operations to calculate current values, prefer consistent patterns across a module:

```rust
// Pattern 1: fetch_add(1) + 1
let new_depth = self.depth.fetch_add(1, Ordering::Relaxed) + 1;

// Pattern 2: fetch_sub(1).saturating_sub(1)
let new_depth = self.depth.fetch_sub(1, Ordering::Relaxed).saturating_sub(1);
```

Both are correct - `fetch_*` returns the *previous* value. However, mixing styles in the same module reduces clarity. Pick one pattern and document with inline comments explaining the difference between the returned value and the current value. The comment should clarify: "fetch_add returns previous value, adding 1 gives current value after increment."

This is a minor code clarity issue but helps maintainers quickly understand atomic value calculations.

---

## Pattern: Warn vs Debug Log Thresholds in Monitoring
**Added**: 2026-02-05
**Related files**: `crates/meeting-controller/src/actors/metrics.rs`

For mailbox depth monitoring, use a tiered logging strategy:

```rust
if level == MailboxLevel::Critical {
    warn!(..., "Mailbox depth critical");      // High visibility, system overloaded
} else if level == MailboxLevel::Warning {
    debug!(..., "Mailbox depth elevated");     // Debug only, normal operation
}
```

Key insight: Log at WARN when crossing into Critical (> threshold), and at DEBUG when first entering Warning zone. This prevents spamming warn logs during normal operation while ensuring critical issues surface immediately. The boundary condition (exact threshold) is a natural trigger point for the transition log.

**Gotcha to avoid**: Exact threshold checks (`== normal_threshold`) can miss batched updates. If messages arrive in bursts, the exact boundary may be skipped. Document this trade-off or use `>=` comparison instead.

---

## Pattern: Complete Metric Instrumentation for Async RPC Calls
**Added**: 2026-02-10
**Related files**: `crates/meeting-controller/src/grpc/gc_client.rs`

For async RPC calls (gRPC, HTTP), follow the complete observability pattern: record both counter and histogram metrics in both success and error branches, measure duration before the async call:

```rust
pub async fn heartbeat(&self, ...) -> Result<(), McError> {
    // Start timer BEFORE the async call (captures total latency including network)
    let start = Instant::now();

    match client.heartbeat(request).await {
        Ok(response) => {
            let duration = start.elapsed();
            // Record success metrics: both counter and histogram
            record_heartbeat("success", "fast");
            record_heartbeat_latency("fast", duration);
            // ... process response
            Ok(())
        }
        Err(e) => {
            let duration = start.elapsed();
            // Record error metrics: both counter and histogram
            record_heartbeat("error", "fast");
            record_heartbeat_latency("fast", duration);
            // ... handle error
            Err(McError::Grpc(format!("Heartbeat failed: {e}")))
        }
    }
}
```

**Key properties:**
- Timer starts before the async call (captures network latency)
- Counter metric records result status (success/error)
- Histogram metric records latency for both paths
- Pattern works for gRPC, HTTP, and any async operation
- Enables SLO tracking (p99 latency, error rate)

**ADR-0011 compliance:**
- Labels are bounded (`status`: 2 values, `type`: 2-3 values)
- Histogram buckets should match SLO targets (e.g., 0.1s target → buckets include 0.05, 0.1, 0.5)

This pattern ensures complete observability for external dependencies, critical for debugging latency and reliability issues.

---
