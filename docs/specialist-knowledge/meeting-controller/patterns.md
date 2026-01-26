# Meeting Controller Patterns

Reusable patterns discovered and established in the Meeting Controller codebase.

---

## Pattern: Actor Handle/Task Separation
**Added**: 2026-01-25
**Related files**: `crates/meeting-controller/src/actors/controller.rs`, `crates/meeting-controller/src/actors/meeting.rs`, `crates/meeting-controller/src/actors/connection.rs`

Separate each actor into a Handle struct and internal Task. The Handle exposes the public API (async methods with oneshot channels for request-response), owns the `mpsc::Sender`, and can be cheaply `Clone`d. The Task owns the receiver, runs the message loop, and holds all mutable state. Use `spawn()` to create both and return `(Handle, JoinHandle<()>)`. This enables: safe cloning of handles for broadcasting, clear ownership of actor state, and monitoring via `JoinHandle::is_finished()`.

---

## Pattern: CancellationToken Parent-Child Hierarchy
**Added**: 2026-01-25
**Related files**: `crates/meeting-controller/src/actors/controller.rs`, `crates/meeting-controller/src/actors/meeting.rs`

Use `tokio_util::sync::CancellationToken` with parent-child relationships for graceful shutdown. The controller owns the root token; meetings get `cancel_token.child_token()`; connections get the meeting's child token. When parent cancels, all children cancel automatically. In the message loop, use `tokio::select!` with `cancel_token.cancelled()` as the first branch to handle shutdown. This ensures orderly cleanup: controller cancels, meetings drain, connections close.

---

## Pattern: HMAC-SHA256 with HKDF for Session Binding
**Added**: 2026-01-25
**Related files**: `crates/meeting-controller/src/actors/session.rs`

For session binding tokens per ADR-0023: derive meeting-specific keys via `HKDF-SHA256(master_secret, salt=meeting_id, info="session-binding")`, then compute `HMAC-SHA256(meeting_key, correlation_id || participant_id || nonce)`. Use `ring::hmac::verify()` for constant-time validation. Store nonce and binding together in `StoredBinding` with 30s TTL. On reconnect, validate then rotate by generating new correlation ID and token.

---

## Pattern: Async State Queries via Oneshot Channels
**Added**: 2026-01-25
**Related files**: `crates/meeting-controller/src/actors/controller.rs`, `crates/meeting-controller/src/actors/meeting.rs`

For operations requiring the actor's current state (e.g., `get_status()`, `get_meeting()`), send a message containing a `oneshot::Sender<Result<T, E>>` and await the response. This ensures the response reflects the actual actor state at processing time, not a potentially stale cache. Pattern: `let (tx, rx) = oneshot::channel(); sender.send(Message::Query { respond_to: tx }).await?; rx.await?`

---

## Pattern: tokio::time::pause for Deterministic Time Tests
**Added**: 2026-01-25
**Related files**: `crates/meeting-controller/src/actors/meeting.rs` (tests)

Use `#[tokio::test(start_paused = true)]` for tests involving time-based behavior like grace periods or TTLs. This pauses the Tokio runtime's internal clock, then use `tokio::time::advance(Duration)` to fast-forward time deterministically. Essential for testing the 30-second disconnect grace period without actually waiting. Remember: `tokio::time::sleep()` still works - it advances time instantly.

---

## Pattern: Proto Evolution for Session Binding
**Added**: 2026-01-25
**Related files**: `proto/signaling.proto`, `crates/proto-gen/src/`

When adding session recovery capabilities to protos, extend existing request/response messages rather than creating new RPCs. For `JoinRequest`, add optional `session_token` and `last_sequence_number` fields. For `JoinResponse`, add `session_token`, `expiry_timestamp`, and `recovery_data` fields. This allows clients to opportunistically provide recovery data without breaking existing flows - servers ignore unknown fields, and new clients work with old servers.

---

## Pattern: Two-Tier Mute State Model
**Added**: 2026-01-25
**Related files**: `proto/signaling.proto`, `crates/meeting-controller/src/session/`

Implement mute state with two distinct tiers: self-mute (informational) and host-mute (enforced). Self-mute is a client preference that can be toggled freely - the MC tracks it but doesn't enforce it. Host-mute is authoritative and overrides self-mute - when host-muted, the client MUST be muted regardless of self-mute state. Proto fields: `is_self_muted: bool` (client-controlled), `is_host_muted: bool` (host-controlled). UI should show different indicators for each state.

---

## Pattern: Config Following GC Structure
**Added**: 2026-01-25
**Related files**: `crates/meeting-controller/src/config.rs`, `crates/global-controller/src/config.rs`

Follow the established GC config pattern for consistency:
1. Group required secrets in dedicated section with `SecretString` types
2. Implement `Debug` manually with `[REDACTED]` for sensitive fields
3. Provide `from_vars()` method that loads from environment variables for production
4. Provide `for_testing()` method that returns safe defaults for tests
5. Use typed duration fields (e.g., `session_timeout: Duration`) not raw integers

---

## Pattern: Error Hierarchy with Nested Domain Errors
**Added**: 2026-01-25
**Related files**: `crates/meeting-controller/src/error.rs`

Define top-level `McError` enum containing nested domain-specific error types. Example: `McError::SessionBinding(SessionBindingError)` where `SessionBindingError` has variants like `TokenExpired`, `SequenceGap`, `InvalidNonce`. This provides both high-level error categorization for HTTP/gRPC status mapping and detailed domain errors for logging/debugging. Map to appropriate status codes at API boundaries.

---

## Pattern: Test Utils Builder for Mock Services
**Added**: 2026-01-25
**Related files**: `crates/mc-test-utils/src/lib.rs`

Create dedicated `mc-test-utils` crate with builder pattern for mock services. `MockRedisBuilder` should support:
- `with_session(session_id, session_data)` - pre-populate sessions
- `with_fencing_token(meeting_id, token)` - support fencing scenarios
- `with_nonce(participant_id, nonce)` - replay protection testing
- `build()` -> `MockRedis`

This mirrors `ac-test-utils` pattern and enables both unit and integration tests to use the same mock infrastructure without `#[cfg(test)]` limitations.

---
