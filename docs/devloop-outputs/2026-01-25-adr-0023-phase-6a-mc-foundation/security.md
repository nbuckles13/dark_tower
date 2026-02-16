# Security Review: ADR-0023 Phase 6a MC Foundation

**Reviewer**: Security Specialist
**Date**: 2026-01-25
**Files Reviewed**: 12 files (meeting-controller crate, mc-test-utils crate, signaling.proto)

## Summary

Phase 6a implements the foundation for the Meeting Controller: configuration, error types, main.rs skeleton, signaling protocol definitions, and test utilities. This is skeleton/foundation code - actual security-critical implementations (session binding, HMAC validation, nonce handling) are deferred to Phase 6b+.

**Verdict**: APPROVED

The implementation follows secure patterns and properly redacts sensitive data. No security vulnerabilities identified. Several security-positive patterns observed.

## Findings

### Security-Positive Patterns (No Action Required)

1. **Sensitive field redaction in Config Debug** (`config.rs:87-111`)
   - `redis_url` and `binding_token_secret` properly redacted in Debug output
   - Prevents accidental logging of credentials

2. **Client-safe error messages** (`errors.rs:122-138`)
   - `client_message()` method hides internal details
   - Redis errors, config errors, and fenced-out errors return generic "An internal error occurred"
   - JWT validation errors return "Invalid or expired token" (no details)

3. **Session binding error types** (`errors.rs:76-99`)
   - Proper error variants for security scenarios: TokenExpired, InvalidToken, NonceReused, UserIdMismatch
   - Maps to UNAUTHORIZED error code (correct)

4. **Required secrets enforced** (`config.rs:137-140`)
   - `MC_BINDING_TOKEN_SECRET` is mandatory (ConfigError::MissingEnvVar)
   - Prevents startup with missing security configuration

5. **Protocol design for session binding** (`signaling.proto:13-26, 85-99`)
   - Session binding pattern properly documented
   - correlation_id and binding_token fields present for reconnection
   - Comments note binding_token has 30s TTL

6. **Mute state separation** (`signaling.proto:111-149`)
   - Self-mute (informational) vs host-mute (enforced) properly distinguished
   - Host-mute includes muted_by field for audit trail

### TECH_DEBT Items (Document for Later)

1. **TD-SEC-001**: Binding token secret validation (Phase 6b)
   - Current: No validation of MC_BINDING_TOKEN_SECRET format/strength
   - Future: Validate base64 encoding, minimum entropy (256 bits recommended)
   - Location: `config.rs:137-140`

2. **TD-SEC-002**: Test fixture binding token (Phase 6b)
   - Current: `TestBindingToken` uses placeholder "test-token-{uuid}"
   - Future: Use real HMAC-SHA256 in test fixtures for realistic testing
   - Location: `mc-test-utils/src/fixtures/mod.rs:144-147`

3. **TD-SEC-003**: Rate limiting on mock services (Phase 6b+)
   - Current: Mock GC/MH have no rate limiting
   - Future: Test rate limiting behavior with mocks that simulate throttling
   - Location: `mc-test-utils/src/mock_gc.rs`, `mc-test-utils/src/mock_mh.rs`

## Files Reviewed

| File | Status | Notes |
|------|--------|-------|
| `meeting-controller/src/lib.rs` | OK | Documentation only |
| `meeting-controller/src/config.rs` | OK | Proper redaction, required secrets |
| `meeting-controller/src/errors.rs` | OK | Client-safe messages, proper error codes |
| `meeting-controller/src/main.rs` | OK | Skeleton, tracing init |
| `meeting-controller/Cargo.toml` | OK | ring for crypto |
| `proto/signaling.proto` | OK | Session binding pattern |
| `mc-test-utils/src/lib.rs` | OK | Re-exports only |
| `mc-test-utils/src/mock_gc.rs` | OK | Test utility |
| `mc-test-utils/src/mock_mh.rs` | OK | Test utility |
| `mc-test-utils/src/mock_redis.rs` | OK | Test utility, proper nonce tracking |
| `mc-test-utils/src/fixtures/mod.rs` | OK | Test fixtures |
| `mc-test-utils/Cargo.toml` | OK | Dependencies appropriate |

## Checklist

- [x] Authentication patterns: Proper JWT + binding token design in proto
- [x] Secret handling: Config redacts sensitive fields
- [x] Error messages: Client-safe, no internal detail leakage
- [x] Input validation: Deferred to Phase 6b (skeleton code)
- [x] Cryptographic choices: ring crate for HMAC (Phase 6b impl)
- [x] Injection vulnerabilities: N/A for skeleton code
- [x] Session management: Proper error types defined

## Verdict

**APPROVED** - No blockers, no critical issues. Foundation code follows secure patterns. Actual security-critical implementations (HMAC validation, nonce checking, binding token verification) are properly deferred to Phase 6b with correct error types already defined.
