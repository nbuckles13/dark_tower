# Code Reviewer - Integration Notes

Notes on working with other specialists in the Dark Tower project.

---

## Integration: Security Specialist Handoff
**Added**: 2026-01-11
**Related files**: `.claude/agents/security.md`

When reviewing security-critical code (crypto, auth, validation), flag findings as MAJOR or CRITICAL for security specialist review. Defense-in-depth recommendations should be explicitly requested if not already implemented. Security specialist should verify cryptographic parameter choices match OWASP/NIST guidance.

---

## Integration: Test Specialist Collaboration
**Added**: 2026-01-11
**Related files**: `.claude/agents/test.md`

After code review, coordinate with test specialist to ensure: boundary conditions have tests, error paths are exercised, security-critical paths have P0 priority tests. For config changes, verify both valid and invalid input tests exist.

---

## Integration: Auth Controller Specialist Context
**Added**: 2026-01-11
**Related files**: `crates/ac-service/`

Auth Controller has established patterns for config, crypto, and validation. When reviewing AC changes, verify pattern consistency with existing code. Key files to check: `config.rs` for configuration patterns, `crypto/mod.rs` for cryptographic operations, `errors.rs` for error handling patterns.

---

## Integration: ADR Compliance Check
**Added**: 2026-01-11
**Related files**: `docs/decisions/`

Cross-reference code changes against existing ADRs. Key ADRs: ADR-0002 (no-panic policy), ADR-0003 (error handling). Flag violations as MAJOR findings requiring remediation before approval.

---

## Integration: Global Controller Service Foundation
**Added**: 2026-01-14
**Related files**: `crates/global-controller/`

GC Phase 1 establishes the foundation for HTTP/3 API gateway. Key patterns for future reviewers:
1. Config loads from environment with sensible defaults (`from_vars()` for testing)
2. AppState holds shared resources (Arc<PgPool>, Config) - must all implement Clone
3. Handlers use State extractor, delegate to services/repositories
4. Error handling maps to HTTP status codes via impl From<GcError> for StatusCode
5. Health checks always return 200 with status field - never error on probe failure
6. Test harness spawns real server instance with JoinHandle for cleanup

When reviewing future GC features (meeting APIs, rate limiting, etc.), ensure they follow these established patterns.

---

## Integration: Test Harness Patterns
**Added**: 2026-01-14
**Related files**: `crates/gc-test-utils/src/server_harness.rs`, `crates/ac-test-utils/src/server_harness.rs`

The GC and AC test harnesses are reusable for all integration tests. Future test specs should:
1. Import the appropriate TestServer from *-test-utils
2. Use `#[sqlx::test(migrations = "../../migrations")]` to get a real database
3. Call `TestServer::spawn(pool).await?` to get a running server
4. Use `server.url()` for HTTP requests
5. Use `server.pool()` for database queries
6. Don't worry about cleanup - Drop impl handles it

---

## Integration: Test Specialist Coordination for DRY Findings
**Added**: 2026-01-15
**Related files**: `crates/ac-service/tests/integration/`

When code review identifies DRY violations in test code (duplicated JWT decoding, repeated assertion patterns), coordinate with Test specialist to:
1. Create shared helper functions in the test-utils crate
2. Update existing tests to use new helpers
3. Document helpers in test harness module docs
4. Ensure helpers return `Result` for ADR-0002 compliance

Flag as tech debt if not immediately addressable per ADR-0019.

---

## Integration: Meeting Controller Service Foundation
**Added**: 2026-01-25
**Updated**: 2026-01-27 (Phase 6c GC integration)
**Related files**: `crates/meeting-controller/`, `crates/mc-test-utils/`

MC Phase 6a/6b establishes the foundation for WebTransport signaling. Key patterns for future reviewers:
1. Config uses builder pattern with `#[must_use]` for fluent test configuration
2. Custom Debug implementations redact sensitive fields (WebTransport secrets, session tokens)
3. Error types follow ADR-0003 with From implementations for clean conversions
4. ADR-0023 references appear in doc comments for traceability
5. mc-test-utils provides MockRedis for session state testing (note: uses std::sync::Mutex - tech debt)

**Actor Hierarchy (Phase 6b)**:
- `MeetingControllerActorHandle` (singleton) supervises N `MeetingActorHandle` instances
- `MeetingActorHandle` supervises N `ConnectionActorHandle` instances
- Handle/Actor separation per ADR-0001: Handle has `mpsc::Sender` + `CancellationToken`, Actor owns state
- State queries must be async to get live values from child actors (see MINOR-001 fix)
- Session binding tokens use HKDF + HMAC-SHA256 per ADR-0023 Section 1

**GC Integration (Phase 6c)**:
- GcClient uses tonic Channel directly (cheap clone, no locking needed) - see `gc_client.rs` module docs
- FencedRedisClient is Clone - uses MultiplexedConnection directly (cheap clone pattern)
- Error variants must match communication protocol (McError::Grpc for gRPC calls, not McError::Redis)
- Estimation constants (e.g., ESTIMATED_PARTICIPANTS_PER_MEETING in mc_service.rs) need doc comments explaining derivation
- Code prepared for Phase 6d uses `#[allow(dead_code)]` with phase reference comment
- All `#[instrument]` attributes use `skip_all` pattern for security (prevents future parameter leaks)
- Error context is preserved in error types (McError::Internal(String)) to aid debugging
- Non-blocking actor design: background cleanup spawned as separate task instead of blocking message loop

**Code Quality Review Insights (2026-01-28)**:
After Phase 6c code quality fixes (Meeting Controller) and GC guard fixes, verified that:
- Error hiding violations fixed by adding context to error variants (MC: 31, GC: 7)
- Instrument skip-all pattern switched from denylist to allowlist for forward compatibility (MC: 16, GC: 16)
- Actor blocking (MC: 1 violation) fixed by spawning background tasks instead of awaiting in message loop
- All changes maintain ADR-0002 (no panics) and ADR-0023 compliance
- SecretBox migration for master_secret adds security property (memory zeroing) without behavioral change

**GC Error Handling Pattern (2026-01-28)**:
GC code quality fixes established consistent error context patterns:
- Configuration errors: Include invalid value and parse error in message
- User input errors (UUID parsing): Log at debug level, return generic user-facing message
- Internal errors: Preserve context in error variant, log server-side, return generic message to client
- gRPC validation errors: Include field name in error message for clarity

The GC fix was cleaner than MC because GcError::Internal was already a String variant in most locations - only 3 additional updates needed in ac_client.rs.

When reviewing future MC features (session management, participant coordination), ensure they follow these established patterns and reference ADR-0023 sections.

**Phase 6c GC Integration Patterns (2026-01-31)**:
Round 3 refactor (Iteration 3) established clean task ownership patterns:
- Unified GC task owns gc_client directly (no Arc) - single consumer, no sharing needed
- Never-exit resilience: registration loop retries forever, heartbeat errors trigger re-registration
- NOT_FOUND detection returns McError::NotRegistered for heartbeat loop to handle
- Single tokio::select! for dual heartbeat intervals (fast + comprehensive)
- handle_heartbeat_error() encapsulates re-registration logic cleanly

Round 4 (test infrastructure) established MockBehavior enum pattern:
- Four variants: Accept, Reject, NotFound, NotFoundThenAccept
- Stateful behavior via atomic counters (first call fails, subsequent succeed)
- Backward-compatible helpers (accepting(), rejecting())
- Enables comprehensive re-registration testing (NOT_FOUND -> attempt_reregistration -> subsequent heartbeats work)

These patterns simplified code from Round 2 (removed Arc, unified tasks) and provided excellent test coverage.

---

## Integration: Common Crate Shared Utilities
**Added**: 2026-02-02
**Related files**: `crates/common/src/token_manager.rs`, `crates/common/src/secret.rs`, `crates/common/src/jwt.rs`

The common crate provides shared utilities used across multiple services. Key modules for code reviewers:

1. **token_manager.rs** - OAuth 2.0 client credentials flow with automatic refresh
   - Uses `tokio::sync::watch` for thread-safe token access (not `Arc<Mutex<>>`)
   - Spawn-and-wait API: `spawn_token_manager()` returns `(JoinHandle, TokenReceiver)`
   - Exponential backoff on failures (1s -> 30s max)
   - Custom Debug implementations redact secrets

2. **secret.rs** - `SecretString` wrapper preventing accidental logging
   - All credentials/tokens MUST use this type
   - Verify custom Debug implementations use `[REDACTED]`

3. **jwt.rs** - JWT validation utilities and constants
   - Size limits, clock skew tolerance, algorithm enforcement

When reviewing code that needs token management, check if it can use `TokenManager` from common crate instead of implementing its own. Services (MC, MH) should share this implementation rather than duplicating OAuth 2.0 logic.

---

## Integration: Observability Specialist (Prometheus Wiring)
**Added**: 2026-02-05
**Related files**: `crates/meeting-controller/src/actors/metrics.rs`, `crates/meeting-controller/src/observability/metrics.rs`

When reviewing code that implements internal metrics, coordinate with observability specialist on Prometheus wiring strategy. Key considerations:

1. **Module-level documentation**: Clarify which structs ARE wired and which are NOT (prevents assumptions)
2. **Naming conventions**: Metric names should follow ADR-0023 naming (e.g., `mc_` prefix for Meeting Controller)
3. **Label cardinality**: Ensure bounded labels per ADR-0011 (e.g., actor_type has 3 values max)
4. **Emission frequency**: High-frequency updates (per-message) may require different patterns than low-frequency (per-meeting)

When a struct has increment/decrement methods that aren't wired to Prometheus, flag as documentation gap. The `ControllerMetrics.increment_participants()` pattern (internal-only, not Prometheus) is valid but must be explicitly documented to prevent assumptions about metric availability.

Ask observability specialist:
- Is this metric needed in Prometheus dashboards?
- Should internal tracking be separate from Prometheus emission (current pattern with two metrics structs)?
- Are there cardinality or performance concerns with the proposed metric?
