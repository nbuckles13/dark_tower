# Meeting Controller Patterns

Reusable patterns discovered and established in the Meeting Controller codebase.

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
