# Code Quality Review

**Reviewer**: Code Quality Reviewer
**Date**: 2026-02-02
**Verdict**: REQUEST_CHANGES

## Summary

The TokenManager integration into Meeting Controller is well-implemented with strong adherence to Rust idioms, proper error handling, and good documentation. The code follows OAuth flow patterns, respects the no-panic policy (ADR-0002) with appropriate `#[expect]` annotations for signal handlers, and demonstrates clean separation of concerns. All sensitive data is protected via `SecretString` with proper Debug redaction. One MINOR finding requires fixing: the hardcoded master secret placeholder should be loaded from config.

## Findings

### BLOCKER
None

### CRITICAL
None

### MAJOR
None

### MINOR

1. **Hardcoded Master Secret Placeholder** (`main.rs:144`)
   - Location: `crates/meeting-controller/src/main.rs:144`
   - Issue: `let master_secret = SecretBox::new(Box::new(vec![0u8; 32])); // TODO: Load from config`
   - Impact: The master secret for session binding tokens is hardcoded to zeros instead of being loaded from config. This works functionally but is not production-ready.
   - Fix Required: Extract master secret from config similar to `binding_token_secret`. Add `MC_MASTER_SECRET` environment variable to config loading, decode from base64, and validate length (>= 32 bytes).

### TECH_DEBT

1. **Missing Endpoint Validation in GcClient** (`gc_client.rs`)
   - Location: `crates/meeting-controller/src/grpc/gc_client.rs:200-211`
   - Issue: The `grpc_endpoint` and `webtransport_endpoint` construction uses `replace("0.0.0.0", "localhost")` which is a development convenience but may not work correctly in all deployment scenarios.
   - Impact: Registration may use incorrect endpoints in Kubernetes or container environments where actual hostnames/IPs should be used.
   - Recommendation: Consider making the advertised endpoints explicit config values rather than deriving them from bind addresses.

2. **Leaking Watch Sender in Tests** (`gc_client.rs:648`, `gc_integration.rs:267`) - ~~RESOLVED~~
   - Location: `crates/meeting-controller/src/grpc/gc_client.rs` and `crates/meeting-controller/tests/gc_integration.rs`
   - Issue: `std::mem::forget(tx);` is used to keep the watch sender alive in test helpers.
   - Impact: Memory is deliberately leaked in tests. While acceptable for tests, this pattern should be documented as test-only.
   - Resolution: Both locations already have explanatory comments: "Keep sender alive by leaking it (acceptable in tests)"

3. **Missing Test for TokenManager Error Paths in Main** (`main.rs`)
   - Location: `crates/meeting-controller/src/main.rs:108-131`
   - Issue: The error handling for `TokenManagerConfig::new_secure` and `spawn_token_manager` timeout is tested conceptually but not via integration tests in this change.
   - Impact: Edge cases like AC unreachable during startup are handled but not verified with integration tests.
   - Recommendation: Add env-tests for TokenManager failure scenarios (AC down, timeout, invalid credentials).

4. **cast_possible_wrap in token_manager.rs** (`token_manager.rs`)
   - Location: `crates/common/src/token_manager.rs:395,459,521`
   - Issue: Uses `#[allow(clippy::cast_possible_wrap)]` for timestamp calculations. While the values are safe in practice, this could be converted to `#[expect]` with reason per ADR-0002.
   - Impact: Minor inconsistency with lint suppression guidelines.
   - Recommendation: Change `#[allow(clippy::cast_possible_wrap)]` to `#[expect(clippy::cast_possible_wrap, reason = "Token expiry timestamps are always positive and within i64 range")]`.

## Observations

### Positive Observations

1. **Excellent Error Handling**: All error paths use proper `Result<T, E>` types with descriptive error variants. The `McError::TokenAcquisition` and `McError::TokenAcquisitionTimeout` errors provide clear context for startup failures.

2. **Strong Security Practices**:
   - `SecretString` used consistently for credentials (client_secret, tokens)
   - Custom Debug impl redacts sensitive fields in `Config`
   - `TokenManagerConfig::new_secure()` enforces HTTPS for AC endpoint
   - OAuth tokens retrieved via `TokenReceiver.token()` with proper `ExposeSecret` pattern

3. **Clean Rust Idioms**:
   - Proper use of `Arc` for shared ownership
   - `tokio::sync::watch` for broadcast token updates
   - Builder pattern for `TokenManagerConfig`
   - Appropriate use of `#[instrument]` for tracing

4. **ADR Compliance**:
   - ADR-0010: OAuth 2.0 client credentials flow correctly implemented
   - ADR-0002: No panics in production paths; `#[expect]` used appropriately for signal handlers with reason
   - Dual heartbeat system (10s fast, 30s comprehensive) per ADR-0010

5. **Good Documentation**:
   - Module-level docs explain purpose and usage
   - Config environment variables documented in doc comments
   - Security notes included where relevant

6. **Well-Structured Test Suite**:
   - Integration tests use mock GC server with configurable behaviors
   - Unit tests cover constants, error types, and edge cases
   - `test-utils` feature enables `TokenReceiver::from_test_channel` for testing

### Code Organization

The changes are well-organized across files:
- `config.rs`: Clean addition of OAuth fields with proper validation
- `main.rs`: Clear startup sequence with token manager spawning before GC registration
- `gc_client.rs`: Token injection via `add_auth()` method, no tight coupling
- `errors.rs`: New error variants for token acquisition failures
- `token_manager.rs`: Self-contained module with comprehensive tests

### Minor Style Notes

- Consistent use of tracing targets (`mc.grpc.gc_client`, `common.token_manager`)
- Appropriate log levels (debug for normal ops, warn for failures, info for lifecycle)
- Good use of structured logging fields
